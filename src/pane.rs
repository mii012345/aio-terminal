use eframe::egui;

/// What kind of content a tab holds
#[derive(Clone, Debug)]
pub enum TabContent {
    Terminal(usize), // terminal instance id
    FileTree,
    Editor(usize),   // editor instance id
    ClaudeCode(usize), // Claude Code terminal instance id
    Codex(usize),      // Codex terminal instance id
}

impl TabContent {
    pub fn title(&self) -> String {
        match self {
            TabContent::Terminal(id) => format!("Terminal {}", id),
            TabContent::FileTree => "Files".to_string(),
            TabContent::Editor(id) => format!("Editor {}", id),
            TabContent::ClaudeCode(_) => "Claude Code".to_string(),
            TabContent::Codex(_) => "Codex".to_string(),
        }
    }

    pub fn title_with_editors(&self, editors: &std::collections::HashMap<usize, crate::editor::Editor>) -> String {
        match self {
            TabContent::Editor(id) => {
                editors.get(id).map(|e| e.title()).unwrap_or_else(|| format!("Editor {}", id))
            }
            _ => self.title(),
        }
    }
}

/// A leaf pane with tabs
#[derive(Clone, Debug)]
pub struct LeafPane {
    pub tabs: Vec<TabContent>,
    pub active_tab: usize,
}

impl LeafPane {
    pub fn new(tab: TabContent) -> Self {
        Self {
            tabs: vec![tab],
            active_tab: 0,
        }
    }

    pub fn active(&self) -> Option<&TabContent> {
        self.tabs.get(self.active_tab)
    }
}

/// Pane tree node
#[derive(Clone, Debug)]
pub enum PaneNode {
    Leaf(LeafPane),
    HSplit {
        left: Box<PaneNode>,
        right: Box<PaneNode>,
        ratio: f32,
    },
    VSplit {
        top: Box<PaneNode>,
        bottom: Box<PaneNode>,
        ratio: f32,
    },
}

impl PaneNode {
    pub fn leaf(tab: TabContent) -> Self {
        PaneNode::Leaf(LeafPane::new(tab))
    }

    pub fn hsplit(left: PaneNode, right: PaneNode, ratio: f32) -> Self {
        PaneNode::HSplit {
            left: Box::new(left),
            right: Box::new(right),
            ratio,
        }
    }

    pub fn vsplit(top: PaneNode, bottom: PaneNode, ratio: f32) -> Self {
        PaneNode::VSplit {
            top: Box::new(top),
            bottom: Box::new(bottom),
            ratio,
        }
    }
}

const DIVIDER_WIDTH: f32 = 4.0;

/// Render the pane tree. Returns which TabContent is visible at each leaf for the app to draw.
/// `draw_leaf` is called for each visible leaf with its rect and content.
pub fn render_pane_tree(
    ui: &mut egui::Ui,
    node: &mut PaneNode,
    rect: egui::Rect,
    draw_leaf: &mut dyn FnMut(&mut egui::Ui, egui::Rect, &mut LeafPane),
) {
    match node {
        PaneNode::Leaf(leaf) => {
            draw_leaf(ui, rect, leaf);
        }
        PaneNode::HSplit {
            left,
            right,
            ratio,
        } => {
            let split_x = rect.left() + rect.width() * *ratio;
            let divider = egui::Rect::from_min_max(
                egui::pos2(split_x - DIVIDER_WIDTH / 2.0, rect.top()),
                egui::pos2(split_x + DIVIDER_WIDTH / 2.0, rect.bottom()),
            );

            // Resize handle
            let id = ui.id().with("hsplit").with(rect.left() as i32);
            let response = ui.interact(divider, id, egui::Sense::drag());
            if response.dragged() {
                let delta = response.drag_delta().x;
                *ratio = ((*ratio * rect.width() + delta) / rect.width()).clamp(0.1, 0.9);
            }
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
            }

            // Draw divider
            ui.painter()
                .rect_filled(divider, 0.0, crate::theme::BORDER);

            let left_rect = egui::Rect::from_min_max(rect.left_top(), egui::pos2(split_x - DIVIDER_WIDTH / 2.0, rect.bottom()));
            let right_rect = egui::Rect::from_min_max(egui::pos2(split_x + DIVIDER_WIDTH / 2.0, rect.top()), rect.right_bottom());

            render_pane_tree(ui, left, left_rect, draw_leaf);
            render_pane_tree(ui, right, right_rect, draw_leaf);
        }
        PaneNode::VSplit {
            top,
            bottom,
            ratio,
        } => {
            let split_y = rect.top() + rect.height() * *ratio;
            let divider = egui::Rect::from_min_max(
                egui::pos2(rect.left(), split_y - DIVIDER_WIDTH / 2.0),
                egui::pos2(rect.right(), split_y + DIVIDER_WIDTH / 2.0),
            );

            let id = ui.id().with("vsplit").with(rect.top() as i32);
            let response = ui.interact(divider, id, egui::Sense::drag());
            if response.dragged() {
                let delta = response.drag_delta().y;
                *ratio = ((*ratio * rect.height() + delta) / rect.height()).clamp(0.1, 0.9);
            }
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
            }

            ui.painter()
                .rect_filled(divider, 0.0, crate::theme::BORDER);

            let top_rect = egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), split_y - DIVIDER_WIDTH / 2.0));
            let bottom_rect = egui::Rect::from_min_max(egui::pos2(rect.left(), split_y + DIVIDER_WIDTH / 2.0), rect.right_bottom());

            render_pane_tree(ui, top, top_rect, draw_leaf);
            render_pane_tree(ui, bottom, bottom_rect, draw_leaf);
        }
    }
}

/// Draw tab bar for a leaf pane, returns the remaining rect for content
pub fn draw_tab_bar(ui: &mut egui::Ui, rect: egui::Rect, leaf: &mut LeafPane) -> egui::Rect {
    let tab_height = 28.0;
    let tab_rect = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), tab_height));

    // Background
    ui.painter()
        .rect_filled(tab_rect, 0.0, crate::theme::TAB_INACTIVE);

    // Use rect position as unique pane identifier
    let pane_id = (rect.left() as i32, rect.top() as i32);

    let mut x = tab_rect.left() + 4.0;
    for (i, tab) in leaf.tabs.iter().enumerate() {
        let title = tab.title();
        let text_width = title.len() as f32 * 7.5 + 16.0;
        let this_tab = egui::Rect::from_min_size(egui::pos2(x, tab_rect.top()), egui::vec2(text_width, tab_height));

        let bg = if i == leaf.active_tab {
            crate::theme::TAB_ACTIVE
        } else {
            crate::theme::TAB_INACTIVE
        };
        ui.painter().rect_filled(this_tab, 2.0, bg);

        let id = ui.id().with("tab").with(pane_id).with(i);
        let resp = ui.interact(this_tab, id, egui::Sense::click());
        if resp.clicked() {
            leaf.active_tab = i;
        }

        let color = if i == leaf.active_tab {
            crate::theme::TEXT_PRIMARY
        } else {
            crate::theme::TEXT_SECONDARY
        };
        ui.painter().text(
            this_tab.center(),
            egui::Align2::CENTER_CENTER,
            &title,
            egui::FontId::proportional(13.0),
            color,
        );

        x += text_width + 2.0;
    }

    // Content area below tabs
    egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + tab_height),
        rect.right_bottom(),
    )
}

/// Draw tab bar with editor-aware titles
pub fn draw_tab_bar_with_editors(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    leaf: &mut LeafPane,
    editors: &std::collections::HashMap<usize, crate::editor::Editor>,
) -> egui::Rect {
    let tab_height = 28.0;
    let tab_rect = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), tab_height));

    ui.painter()
        .rect_filled(tab_rect, 0.0, crate::theme::TAB_INACTIVE);

    let pane_id = (rect.left() as i32, rect.top() as i32);

    let mut x = tab_rect.left() + 4.0;
    for (i, tab) in leaf.tabs.iter().enumerate() {
        let title = tab.title_with_editors(editors);
        let text_width = title.len() as f32 * 7.5 + 16.0;
        let this_tab = egui::Rect::from_min_size(egui::pos2(x, tab_rect.top()), egui::vec2(text_width, tab_height));

        let bg = if i == leaf.active_tab {
            crate::theme::TAB_ACTIVE
        } else {
            crate::theme::TAB_INACTIVE
        };
        ui.painter().rect_filled(this_tab, 2.0, bg);

        let id = ui.id().with("tab_e").with(pane_id).with(i);
        let resp = ui.interact(this_tab, id, egui::Sense::click());
        if resp.clicked() {
            leaf.active_tab = i;
        }

        let color = if i == leaf.active_tab {
            crate::theme::TEXT_PRIMARY
        } else {
            crate::theme::TEXT_SECONDARY
        };
        ui.painter().text(
            this_tab.center(),
            egui::Align2::CENTER_CENTER,
            &title,
            egui::FontId::proportional(13.0),
            color,
        );

        x += text_width + 2.0;
    }

    egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + tab_height),
        rect.right_bottom(),
    )
}
