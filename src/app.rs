use crate::file_tree::FileTree;
use crate::pane::{self, PaneNode, TabContent};
use crate::terminal::Terminal;
use crate::theme::Theme;
use eframe::egui;
use std::collections::HashMap;

pub struct AioApp {
    pane_root: PaneNode,
    terminals: HashMap<usize, Terminal>,
    file_tree: FileTree,
    next_terminal_id: usize,
}

impl AioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Theme::apply(&cc.egui_ctx);

        // Set up fonts - ensure monospace is available
        // egui includes a default monospace font

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));

        let mut terminals = HashMap::new();

        // Create initial terminal
        let term0 = Terminal::new(24, 80).expect("Failed to create terminal");
        terminals.insert(0, term0);

        // Default layout:
        // HSplit(FileTree | VSplit(Terminal 0 top, Terminal 1 bottom))
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
            file_tree: FileTree::new(cwd),
            next_terminal_id: 2,
        }
    }
}

impl eframe::App for AioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle keyboard shortcuts
        // TODO: Cmd+T new terminal tab, Cmd+W close tab

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(crate::theme::BG_BASE))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

                // We need to split borrows: pane_root vs terminals/file_tree
                let terminals = &mut self.terminals;
                let file_tree = &mut self.file_tree;

                pane::render_pane_tree(
                    ui,
                    &mut self.pane_root,
                    rect,
                    &mut |ui, rect, leaf| {
                        let content_rect = pane::draw_tab_bar(ui, rect, leaf);

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
                            }
                        }
                    },
                );
            });
    }
}
