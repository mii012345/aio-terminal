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
}

impl AioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Theme::apply(&cc.egui_ctx);

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
        }
    }

    fn open_file_in_editor(&mut self, path: PathBuf) {
        // Check if already open
        for (_id, editor) in &self.editors {
            if editor.file_path.as_ref() == Some(&path) {
                return;
            }
        }

        let id = self.next_editor_id;
        self.next_editor_id += 1;

        match Editor::open_file(id, path) {
            Ok(editor) => {
                self.editors.insert(id, editor);
                Self::add_tab_to_pane(&mut self.pane_root, TabContent::Editor(id));
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
        ctx.input(|i| {
            let cmd = i.modifiers.mac_cmd || i.modifiers.ctrl;
            if cmd && i.key_pressed(egui::Key::O) {
                open_folder_requested = true;
            }
        });

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

        let file_to_open = self.file_tree.take_pending_open();

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(crate::theme::BG_BASE))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

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
                                TabContent::Terminal(id) => {
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
