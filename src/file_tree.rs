use eframe::egui::{self, Rect};
use std::path::{Path, PathBuf};

pub struct FileTree {
    pub root: PathBuf,
    expanded: std::collections::HashSet<PathBuf>,
}

impl FileTree {
    pub fn new(root: PathBuf) -> Self {
        let mut expanded = std::collections::HashSet::new();
        expanded.insert(root.clone());
        Self { root, expanded }
    }

    pub fn render(&mut self, ui: &mut egui::Ui, rect: Rect) {
        ui.painter()
            .rect_filled(rect, 0.0, crate::theme::BG_SURFACE);

        let child_ui_rect = rect.shrink(4.0);
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(child_ui_rect));

        egui::ScrollArea::vertical()
            .id_salt("file_tree_scroll")
            .show(&mut child_ui, |ui| {
                self.render_dir(ui, &self.root.clone(), 0);
            });
    }

    fn render_dir(&mut self, ui: &mut egui::Ui, path: &Path, depth: usize) {
        let entries = self.read_dir_filtered(path);

        for entry in entries {
            let is_dir = entry.is_dir();
            let name = entry
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let indent = depth as f32 * 16.0;
            let icon = if is_dir {
                if self.expanded.contains(&entry) {
                    "▼ 📁"
                } else {
                    "▶ 📁"
                }
            } else {
                "  📄"
            };

            ui.horizontal(|ui| {
                ui.add_space(indent);
                let label = format!("{} {}", icon, name);
                let resp = ui.selectable_label(false, &label);
                if resp.clicked() && is_dir {
                    if self.expanded.contains(&entry) {
                        self.expanded.remove(&entry);
                    } else {
                        self.expanded.insert(entry.clone());
                    }
                }
                // TODO: clicking files should open them in editor (Phase 3)
            });

            if is_dir && self.expanded.contains(&entry) {
                self.render_dir(ui, &entry, depth + 1);
            }
        }
    }

    fn read_dir_filtered(&self, path: &Path) -> Vec<PathBuf> {
        // Use ignore crate for .gitignore support
        let mut entries = Vec::new();

        let walker = ignore::WalkBuilder::new(path)
            .max_depth(Some(1))
            .hidden(false)
            .build();

        for result in walker {
            if let Ok(entry) = result {
                let p = entry.into_path();
                if p == path {
                    continue;
                }
                entries.push(p);
            }
        }

        // Sort: dirs first, then alphabetical
        entries.sort_by(|a, b| {
            let a_dir = a.is_dir();
            let b_dir = b.is_dir();
            b_dir.cmp(&a_dir).then_with(|| {
                a.file_name()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .cmp(&b.file_name().unwrap_or_default().to_ascii_lowercase())
            })
        });

        entries
    }
}
