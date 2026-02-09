use crate::editor::Editor;
use crate::file_tree::FileTree;
use crate::pane::{self, PaneNode, TabContent};
use crate::terminal::Terminal;
use crate::theme::Theme;
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct AioApp {
    pane_root: PaneNode,
    terminals: HashMap<usize, Terminal>,
    editors: HashMap<usize, Editor>,
    file_tree: FileTree,
    next_terminal_id: usize,
    next_editor_id: usize,
    pending_open_folder: Option<PathBuf>,
    pending_focus: Option<TabContent>,
    /// Tab that should grab keyboard focus on next render
    focus_grab: Option<TabContent>,
}

impl AioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Theme::apply(&cc.egui_ctx);

        // Load Japanese font from system
        let mut fonts = egui::FontDefinitions::default();
        let jp_font_paths = [
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/Library/Fonts/Arial Unicode.ttf",
            // Linux fallbacks
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ];
        for path in &jp_font_paths {
            if let Ok(data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "jp_font".to_owned(),
                    egui::FontData::from_owned(data).into(),
                );
                // Add as fallback to both proportional and monospace
                if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    family.push("jp_font".to_owned());
                }
                if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                    family.push("jp_font".to_owned());
                }
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));

        let mut terminals = HashMap::new();
        let term0 = Terminal::new(24, 80).expect("Failed to create terminal");
        terminals.insert(0, term0);
        let term1 = Terminal::new(24, 80).expect("Failed to create terminal");
        terminals.insert(1, term1);

        let layout = PaneNode::hsplit(
            PaneNode::leaf(TabContent::FileTree),
            PaneNode::vsplit(
                PaneNode::leaf(TabContent::Terminal(0)),
                PaneNode::leaf(TabContent::Terminal(1)),
                0.6,
            ),
            0.2,
        );

        Self {
            pane_root: layout,
            terminals,
            editors: HashMap::new(),
            file_tree: FileTree::new(cwd),
            next_terminal_id: 2,
            next_editor_id: 0,
            pending_open_folder: None,
            pending_focus: None,
            focus_grab: None,
        }
    }

    fn open_file_in_editor(&mut self, path: PathBuf) {
        // Check if already open — focus existing tab
        for (id, editor) in &self.editors {
            if editor.file_path.as_ref() == Some(&path) {
                self.pending_focus = Some(TabContent::Editor(*id));
                return;
            }
        }

        let id = self.next_editor_id;
        self.next_editor_id += 1;

        match Editor::open_file(id, path) {
            Ok(editor) => {
                self.editors.insert(id, editor);
                let tab = TabContent::Editor(id);
                Self::add_tab_to_pane(&mut self.pane_root, tab.clone());
                self.pending_focus = Some(tab);
            }
            Err(e) => {
                eprintln!("Failed to open file: {}", e);
            }
        }
    }

    fn add_tab_to_pane(node: &mut PaneNode, content: TabContent) {
        if Self::try_add_tab(node, &content) {
            return;
        }
        Self::force_add_tab(node, content);
    }

    fn try_add_tab(node: &mut PaneNode, content: &TabContent) -> bool {
        match node {
            PaneNode::Leaf(leaf) => {
                if !matches!(leaf.tabs.first(), Some(TabContent::FileTree)) {
                    leaf.tabs.push(content.clone());
                    leaf.active_tab = leaf.tabs.len() - 1;
                    return true;
                }
                false
            }
            PaneNode::HSplit { left, right, .. } => {
                Self::try_add_tab(right, content) || Self::try_add_tab(left, content)
            }
            PaneNode::VSplit { top, bottom, .. } => {
                Self::try_add_tab(top, content) || Self::try_add_tab(bottom, content)
            }
        }
    }

    fn close_active_tab(
        node: &mut PaneNode,
        terminals: &mut HashMap<usize, Terminal>,
        editors: &mut HashMap<usize, Editor>,
    ) {
        // Find the first leaf and close its active tab
        match node {
            PaneNode::Leaf(leaf) => {
                if leaf.tabs.len() > 1 {
                    let removed = leaf.tabs.remove(leaf.active_tab);
                    if leaf.active_tab >= leaf.tabs.len() {
                        leaf.active_tab = leaf.tabs.len().saturating_sub(1);
                    }
                    // Clean up resources
                    match removed {
                        TabContent::Terminal(id) | TabContent::ClaudeCode(id) | TabContent::Codex(id) => { terminals.remove(&id); }
                        TabContent::Editor(id) => { editors.remove(&id); }
                        _ => {}
                    }
                } else if leaf.tabs.len() == 1 {
                    // Don't close the last tab in a pane (keep at least the pane)
                    let removed = &leaf.tabs[0];
                    match removed {
                        TabContent::Terminal(id) | TabContent::ClaudeCode(id) | TabContent::Codex(id) => { terminals.remove(id); }
                        TabContent::Editor(id) => { editors.remove(id); }
                        _ => {}
                    }
                    // Replace with an empty terminal
                    // For now just keep it
                }
            }
            // Close from the rightmost/bottom-most leaf first (most likely focused)
            PaneNode::HSplit { right, .. } => Self::close_active_tab(right, terminals, editors),
            PaneNode::VSplit { top, .. } => Self::close_active_tab(top, terminals, editors),
        }
    }

    fn focus_tab(node: &mut PaneNode, target: &TabContent) -> bool {
        match node {
            PaneNode::Leaf(leaf) => {
                for (i, tab) in leaf.tabs.iter().enumerate() {
                    if std::mem::discriminant(tab) == std::mem::discriminant(target) {
                        let matches = match (tab, target) {
                            (TabContent::Terminal(a), TabContent::Terminal(b)) => a == b,
                            (TabContent::Editor(a), TabContent::Editor(b)) => a == b,
                            (TabContent::FileTree, TabContent::FileTree) => true,
                            (TabContent::ClaudeCode(a), TabContent::ClaudeCode(b)) => a == b,
                            (TabContent::Codex(a), TabContent::Codex(b)) => a == b,
                            _ => false,
                        };
                        if matches {
                            leaf.active_tab = i;
                            return true;
                        }
                    }
                }
                false
            }
            PaneNode::HSplit { left, right, .. } => {
                Self::focus_tab(left, target) || Self::focus_tab(right, target)
            }
            PaneNode::VSplit { top, bottom, .. } => {
                Self::focus_tab(top, target) || Self::focus_tab(bottom, target)
            }
        }
    }

    fn force_add_tab(node: &mut PaneNode, content: TabContent) {
        match node {
            PaneNode::Leaf(leaf) => {
                leaf.tabs.push(content);
                leaf.active_tab = leaf.tabs.len() - 1;
            }
            PaneNode::HSplit { left, .. } => Self::force_add_tab(left, content),
            PaneNode::VSplit { top, .. } => Self::force_add_tab(top, content),
        }
    }
}

impl eframe::App for AioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut open_folder_requested = false;
        let mut close_tab_requested = false;
        let mut new_terminal_requested = false;
        let mut new_file_requested = false;
        let mut new_claude_requested = false;
        let mut new_codex_requested = false;
        ctx.input(|i| {
            let cmd = i.modifiers.mac_cmd || i.modifiers.ctrl;
            if cmd && i.modifiers.shift && i.key_pressed(egui::Key::C) {
                new_claude_requested = true;
            } else if cmd && i.modifiers.shift && i.key_pressed(egui::Key::X) {
                new_codex_requested = true;
            } else if cmd && i.key_pressed(egui::Key::O) {
                open_folder_requested = true;
            } else if cmd && i.key_pressed(egui::Key::W) {
                close_tab_requested = true;
            } else if cmd && i.key_pressed(egui::Key::T) {
                new_terminal_requested = true;
            } else if cmd && i.key_pressed(egui::Key::N) {
                new_file_requested = true;
            }
        });

        if close_tab_requested {
            Self::close_active_tab(&mut self.pane_root, &mut self.terminals, &mut self.editors);
        }

        if new_terminal_requested {
            let id = self.next_terminal_id;
            self.next_terminal_id += 1;
            if let Ok(term) = Terminal::new(24, 80) {
                self.terminals.insert(id, term);
                let tab = TabContent::Terminal(id);
                Self::add_tab_to_pane(&mut self.pane_root, tab.clone());
                self.pending_focus = Some(tab);
            }
        }

        if new_file_requested {
            let id = self.next_editor_id;
            self.next_editor_id += 1;
            let editor = Editor::new_empty(id);
            self.editors.insert(id, editor);
            let tab = TabContent::Editor(id);
            Self::add_tab_to_pane(&mut self.pane_root, tab.clone());
            self.pending_focus = Some(tab);
        }

        if new_claude_requested {
            let id = self.next_terminal_id;
            self.next_terminal_id += 1;
            if let Ok(term) = Terminal::with_command(24, 80, "claude", &[], &[]) {
                self.terminals.insert(id, term);
                let tab = TabContent::ClaudeCode(id);
                Self::add_tab_to_pane(&mut self.pane_root, tab.clone());
                self.pending_focus = Some(tab);
            }
        }

        if new_codex_requested {
            let id = self.next_terminal_id;
            self.next_terminal_id += 1;
            if let Ok(term) = Terminal::with_command(24, 80, "codex", &[], &[]) {
                self.terminals.insert(id, term);
                let tab = TabContent::Codex(id);
                Self::add_tab_to_pane(&mut self.pane_root, tab.clone());
                self.pending_focus = Some(tab);
            }
        }

        if open_folder_requested {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                self.pending_open_folder = Some(path);
            }
        }

        if let Some(folder) = self.pending_open_folder.take() {
            self.file_tree = FileTree::new(folder.clone());
            let name = folder.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| folder.to_string_lossy().to_string());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                format!("AiO Terminal — {}", name),
            ));
        }

        // Handle pending focus — switch to the tab in whichever pane contains it
        if let Some(target) = self.pending_focus.take() {
            Self::focus_tab(&mut self.pane_root, &target);
            self.focus_grab = Some(target);
        }

        let file_to_open = self.file_tree.take_pending_open();

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(crate::theme::BG_BASE))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

                // Set grab_focus on the target terminal/editor
                if let Some(ref target) = self.focus_grab.take() {
                    match target {
                        TabContent::Terminal(id) | TabContent::ClaudeCode(id) | TabContent::Codex(id) => {
                            if let Some(term) = self.terminals.get_mut(id) {
                                term.grab_focus = true;
                            }
                        }
                        TabContent::Editor(id) => {
                            if let Some(editor) = self.editors.get_mut(id) {
                                editor.grab_focus = true;
                            }
                        }
                        _ => {}
                    }
                }

                let terminals = &mut self.terminals;
                let file_tree = &mut self.file_tree;
                let editors = &mut self.editors;

                pane::render_pane_tree(
                    ui,
                    &mut self.pane_root,
                    rect,
                    &mut |ui, rect, leaf| {
                        let content_rect = pane::draw_tab_bar_with_editors(ui, rect, leaf, editors);

                        if let Some(tab) = leaf.active().cloned() {
                            match tab {
                                TabContent::Terminal(id) | TabContent::ClaudeCode(id) | TabContent::Codex(id) => {
                                    if let Some(term) = terminals.get_mut(&id) {
                                        term.render(ui, content_rect);
                                    }
                                }
                                TabContent::FileTree => {
                                    file_tree.render(ui, content_rect);
                                }
                                TabContent::Editor(id) => {
                                    if let Some(editor) = editors.get_mut(&id) {
                                        editor.render(ui, content_rect);
                                    }
                                }
                            }
                        }
                    },
                );
            });

        if let Some(path) = file_to_open {
            self.open_file_in_editor(path);
        }
    }
}
