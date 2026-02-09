use eframe::egui::{self, Color32, FontId, Rect};
use std::path::PathBuf;

/// Unique editor instance ID
pub type EditorId = usize;

#[derive(Clone, Debug)]
struct UndoEntry {
    content: String,
    cursor: usize,
}

pub struct Editor {
    pub id: EditorId,
    pub file_path: Option<PathBuf>,
    pub content: String,
    pub cursor: usize,         // byte offset
    pub selection_anchor: Option<usize>, // byte offset for selection start
    pub scroll_offset: f32,    // vertical scroll in pixels
    pub modified: bool,
    pub line_count: usize,

    // Search
    pub search_open: bool,
    pub search_query: String,
    pub search_matches: Vec<(usize, usize)>, // (start, end) byte offsets
    pub search_current: usize,

    // Undo/Redo
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
    last_snapshot_content: String,
}

impl Editor {
    pub fn new(id: EditorId) -> Self {
        Self {
            id,
            file_path: None,
            content: String::new(),
            cursor: 0,
            selection_anchor: None,
            scroll_offset: 0.0,
            modified: false,
            line_count: 1,
            search_open: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_snapshot_content: String::new(),
        }
    }

    pub fn new_empty(id: EditorId) -> Self {
        Self::new(id)
    }

    pub fn open_file(id: EditorId, path: PathBuf) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(&path)?;
        let line_count = content.lines().count().max(1);
        let snapshot = content.clone();
        Ok(Self {
            id,
            file_path: Some(path),
            content,
            cursor: 0,
            selection_anchor: None,
            scroll_offset: 0.0,
            modified: false,
            line_count,
            search_open: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_snapshot_content: snapshot,
        })
    }

    pub fn title(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        if self.modified {
            format!("● {}", name)
        } else {
            name
        }
    }

    pub fn save(&mut self) -> Result<(), std::io::Error> {
        if let Some(ref path) = self.file_path {
            std::fs::write(path, &self.content)?;
            self.modified = false;
        } else {
            // Untitled — show save dialog
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("untitled.txt")
                .save_file()
            {
                std::fs::write(&path, &self.content)?;
                self.file_path = Some(path);
                self.modified = false;
            }
        }
        Ok(())
    }

    fn snapshot_undo(&mut self) {
        if self.content != self.last_snapshot_content {
            self.undo_stack.push(UndoEntry {
                content: self.last_snapshot_content.clone(),
                cursor: self.cursor,
            });
            self.last_snapshot_content = self.content.clone();
            self.redo_stack.clear();
        }
    }

    fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(UndoEntry {
                content: self.content.clone(),
                cursor: self.cursor,
            });
            self.content = entry.content.clone();
            self.cursor = entry.cursor.min(self.content.len());
            self.last_snapshot_content = self.content.clone();
            self.update_line_count();
            self.modified = true;
        }
    }

    fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(UndoEntry {
                content: self.content.clone(),
                cursor: self.cursor,
            });
            self.content = entry.content.clone();
            self.cursor = entry.cursor.min(self.content.len());
            self.last_snapshot_content = self.content.clone();
            self.update_line_count();
            self.modified = true;
        }
    }

    fn update_line_count(&mut self) {
        self.line_count = self.content.lines().count().max(1);
        if self.content.ends_with('\n') {
            self.line_count += 1;
        }
    }

    fn cursor_line_col(&self) -> (usize, usize) {
        let before = &self.content[..self.cursor.min(self.content.len())];
        let line = before.matches('\n').count();
        let col = before.rfind('\n').map(|p| self.cursor - p - 1).unwrap_or(self.cursor);
        (line, col)
    }

    fn line_start(&self, line: usize) -> usize {
        let mut offset = 0;
        for (i, l) in self.content.split('\n').enumerate() {
            if i == line {
                return offset;
            }
            offset += l.len() + 1;
        }
        self.content.len()
    }

    fn line_end(&self, line: usize) -> usize {
        let start = self.line_start(line);
        let rest = &self.content[start..];
        start + rest.find('\n').unwrap_or(rest.len())
    }

    fn total_lines(&self) -> usize {
        self.content.split('\n').count()
    }

    fn delete_selection(&mut self) -> bool {
        if let Some(anchor) = self.selection_anchor.take() {
            let start = anchor.min(self.cursor);
            let end = anchor.max(self.cursor);
            let start = start.min(self.content.len());
            let end = end.min(self.content.len());
            self.snapshot_undo();
            self.content.replace_range(start..end, "");
            self.cursor = start;
            self.modified = true;
            self.update_line_count();
            true
        } else {
            false
        }
    }

    fn selected_text(&self) -> Option<String> {
        self.selection_anchor.map(|anchor| {
            let start = anchor.min(self.cursor);
            let end = anchor.max(self.cursor);
            self.content[start.min(self.content.len())..end.min(self.content.len())].to_string()
        })
    }

    fn insert_text(&mut self, text: &str) {
        self.delete_selection();
        self.snapshot_undo();
        let pos = self.cursor.min(self.content.len());
        self.content.insert_str(pos, text);
        self.cursor = pos + text.len();
        self.modified = true;
        self.update_line_count();
    }

    fn move_cursor_left(&mut self, shift: bool) {
        if !shift {
            self.selection_anchor = None;
        } else if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        if self.cursor > 0 {
            // Move back one char properly
            let prev = self.content[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor = prev;
        }
    }

    fn move_cursor_right(&mut self, shift: bool) {
        if !shift {
            self.selection_anchor = None;
        } else if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        if self.cursor < self.content.len() {
            let next = self.content[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.content.len());
            self.cursor = next;
        }
    }

    fn move_cursor_up(&mut self, shift: bool) {
        if !shift {
            self.selection_anchor = None;
        } else if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        let (line, col) = self.cursor_line_col();
        if line > 0 {
            let new_start = self.line_start(line - 1);
            let new_end = self.line_end(line - 1);
            let line_len = new_end - new_start;
            self.cursor = new_start + col.min(line_len);
        }
    }

    fn move_cursor_down(&mut self, shift: bool) {
        if !shift {
            self.selection_anchor = None;
        } else if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        let (line, col) = self.cursor_line_col();
        let total = self.total_lines();
        if line + 1 < total {
            let new_start = self.line_start(line + 1);
            let new_end = self.line_end(line + 1);
            let line_len = new_end - new_start;
            self.cursor = new_start + col.min(line_len);
        }
    }

    fn update_search(&mut self) {
        self.search_matches.clear();
        if self.search_query.is_empty() {
            return;
        }
        let query = &self.search_query.clone();
        let mut start = 0;
        while let Some(pos) = self.content[start..].find(query.as_str()) {
            let abs = start + pos;
            self.search_matches.push((abs, abs + query.len()));
            start = abs + query.len().max(1);
        }
        if self.search_current >= self.search_matches.len() {
            self.search_current = 0;
        }
    }

    fn jump_to_search_match(&mut self) {
        if let Some(&(start, end)) = self.search_matches.get(self.search_current) {
            self.cursor = end;
            self.selection_anchor = Some(start);
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let font = FontId::monospace(14.0);
        let char_width = 8.4_f32;
        let line_height = 17.0_f32;
        let gutter_width = 50.0_f32;

        // Background
        ui.painter().rect_filled(rect, 0.0, crate::theme::BG_SURFACE);

        // Search bar at top if open
        let (search_rect, content_rect) = if self.search_open {
            let search_h = 28.0;
            let sr = Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), search_h));
            let cr = Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + search_h),
                rect.right_bottom(),
            );
            (Some(sr), cr)
        } else {
            (None, rect)
        };

        // Draw search bar
        if let Some(sr) = search_rect {
            ui.painter().rect_filled(sr, 0.0, crate::theme::BG_ELEVATED);
            let search_id = ui.id().with(("editor_search", self.id));
            let text_rect = Rect::from_min_size(
                egui::pos2(sr.left() + 8.0, sr.top() + 4.0),
                egui::vec2(sr.width() - 16.0, 20.0),
            );

            // Simple search input via text painter
            ui.painter().text(
                egui::pos2(sr.left() + 8.0, sr.center().y),
                egui::Align2::LEFT_CENTER,
                if self.search_query.is_empty() {
                    "Search..."
                } else {
                    &self.search_query
                },
                FontId::proportional(13.0),
                if self.search_query.is_empty() {
                    crate::theme::TEXT_SECONDARY
                } else {
                    crate::theme::TEXT_PRIMARY
                },
            );

            // Match count
            if !self.search_query.is_empty() {
                let info = format!(
                    "{}/{}",
                    if self.search_matches.is_empty() { 0 } else { self.search_current + 1 },
                    self.search_matches.len()
                );
                ui.painter().text(
                    egui::pos2(sr.right() - 8.0, sr.center().y),
                    egui::Align2::RIGHT_CENTER,
                    &info,
                    FontId::proportional(12.0),
                    crate::theme::TEXT_SECONDARY,
                );
            }
        }

        let gutter_rect = Rect::from_min_size(
            content_rect.left_top(),
            egui::vec2(gutter_width, content_rect.height()),
        );
        let text_rect = Rect::from_min_max(
            egui::pos2(content_rect.left() + gutter_width, content_rect.top()),
            content_rect.right_bottom(),
        );

        // Gutter background
        ui.painter()
            .rect_filled(gutter_rect, 0.0, crate::theme::BG_ELEVATED);

        // Handle focus and input
        let unique_id = ui.id().with(("editor_input", self.id));
        let response = ui.interact(rect, unique_id, egui::Sense::click());

        if response.clicked() {
            ui.memory_mut(|mem| mem.request_focus(unique_id));

            // Calculate click position to set cursor
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x >= text_rect.left() {
                    let col = ((pos.x - text_rect.left()) / char_width).floor() as usize;
                    let row = ((pos.y - text_rect.top() + self.scroll_offset) / line_height).floor() as usize;
                    let row = row.min(self.total_lines().saturating_sub(1));
                    let start = self.line_start(row);
                    let end = self.line_end(row);
                    let line_len = end - start;
                    self.cursor = start + col.min(line_len);
                    self.selection_anchor = None;
                }
            }
        }

        let has_focus = ui.memory(|mem| mem.has_focus(unique_id));

        if has_focus {
            let mut needs_search_update = false;

            ui.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) => {
                            if self.search_open {
                                // If search is open, and we're focused, type into search
                                // Actually we handle search input separately below
                            }
                            // Normal text insertion
                            if !self.search_open {
                                self.insert_text(text);
                            }
                        }
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            let cmd = modifiers.mac_cmd || modifiers.ctrl;

                            if cmd && *key == egui::Key::S {
                                let _ = self.save();
                            } else if cmd && *key == egui::Key::F {
                                self.search_open = !self.search_open;
                                if !self.search_open {
                                    self.search_matches.clear();
                                }
                            } else if cmd && *key == egui::Key::Z {
                                if modifiers.shift {
                                    self.redo();
                                } else {
                                    self.undo();
                                }
                            } else if cmd && *key == egui::Key::A {
                                // Select all
                                self.selection_anchor = Some(0);
                                self.cursor = self.content.len();
                            } else if cmd && *key == egui::Key::C {
                                // Copy
                                if let Some(text) = self.selected_text() {
                                    ui.ctx().copy_text(text);
                                }
                            } else if cmd && *key == egui::Key::X {
                                // Cut
                                if let Some(text) = self.selected_text() {
                                    ui.ctx().copy_text(text);
                                    self.delete_selection();
                                }
                            } else if cmd && *key == egui::Key::V {
                                // Paste handled via Event::Paste
                            } else if self.search_open {
                                // Search mode key handling
                                match key {
                                    egui::Key::Escape => {
                                        self.search_open = false;
                                        self.search_matches.clear();
                                    }
                                    egui::Key::Enter => {
                                        if !self.search_matches.is_empty() {
                                            self.search_current = (self.search_current + 1) % self.search_matches.len();
                                            self.jump_to_search_match();
                                        }
                                    }
                                    egui::Key::Backspace => {
                                        self.search_query.pop();
                                        needs_search_update = true;
                                    }
                                    _ => {}
                                }
                            } else {
                                // Normal editing mode
                                match key {
                                    egui::Key::ArrowLeft => self.move_cursor_left(modifiers.shift),
                                    egui::Key::ArrowRight => self.move_cursor_right(modifiers.shift),
                                    egui::Key::ArrowUp => self.move_cursor_up(modifiers.shift),
                                    egui::Key::ArrowDown => self.move_cursor_down(modifiers.shift),
                                    egui::Key::Home => {
                                        if !modifiers.shift { self.selection_anchor = None; }
                                        else if self.selection_anchor.is_none() { self.selection_anchor = Some(self.cursor); }
                                        let (line, _) = self.cursor_line_col();
                                        self.cursor = self.line_start(line);
                                    }
                                    egui::Key::End => {
                                        if !modifiers.shift { self.selection_anchor = None; }
                                        else if self.selection_anchor.is_none() { self.selection_anchor = Some(self.cursor); }
                                        let (line, _) = self.cursor_line_col();
                                        self.cursor = self.line_end(line);
                                    }
                                    egui::Key::Enter => {
                                        self.insert_text("\n");
                                    }
                                    egui::Key::Tab => {
                                        self.insert_text("    ");
                                    }
                                    egui::Key::Backspace => {
                                        if !self.delete_selection() && self.cursor > 0 {
                                            self.snapshot_undo();
                                            let prev = self.content[..self.cursor]
                                                .char_indices()
                                                .last()
                                                .map(|(i, _)| i)
                                                .unwrap_or(0);
                                            self.content.replace_range(prev..self.cursor, "");
                                            self.cursor = prev;
                                            self.modified = true;
                                            self.update_line_count();
                                        }
                                    }
                                    egui::Key::Delete => {
                                        if !self.delete_selection() && self.cursor < self.content.len() {
                                            self.snapshot_undo();
                                            let next = self.content[self.cursor..]
                                                .char_indices()
                                                .nth(1)
                                                .map(|(i, _)| self.cursor + i)
                                                .unwrap_or(self.content.len());
                                            self.content.replace_range(self.cursor..next, "");
                                            self.modified = true;
                                            self.update_line_count();
                                        }
                                    }
                                    egui::Key::Escape => {
                                        self.selection_anchor = None;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        egui::Event::Paste(text) => {
                            if self.search_open {
                                self.search_query.push_str(text);
                                needs_search_update = true;
                            } else {
                                self.insert_text(text);
                            }
                        }
                        _ => {}
                    }
                }

                // Handle text input to search when search is open
                if self.search_open {
                    for event in &i.events {
                        if let egui::Event::Text(text) = event {
                            self.search_query.push_str(text);
                            needs_search_update = true;
                        }
                    }
                }
            });

            if needs_search_update {
                self.update_search();
                if !self.search_matches.is_empty() {
                    self.jump_to_search_match();
                }
            }
        }

        // Scroll handling
        ui.input(|i| {
            if rect.contains(i.pointer.hover_pos().unwrap_or_default()) {
                let scroll_delta = i.smooth_scroll_delta.y;
                self.scroll_offset = (self.scroll_offset - scroll_delta).max(0.0);
                let max_scroll = (self.total_lines() as f32 * line_height - content_rect.height()).max(0.0);
                self.scroll_offset = self.scroll_offset.min(max_scroll);
            }
        });

        // Ensure cursor is visible
        let (cursor_line, _cursor_col) = self.cursor_line_col();
        let cursor_y = cursor_line as f32 * line_height;
        if cursor_y < self.scroll_offset {
            self.scroll_offset = cursor_y;
        } else if cursor_y + line_height > self.scroll_offset + content_rect.height() {
            self.scroll_offset = cursor_y + line_height - content_rect.height();
        }

        // Render lines
        let first_visible = (self.scroll_offset / line_height).floor() as usize;
        let visible_lines = (content_rect.height() / line_height).ceil() as usize + 1;

        // Get syntax colors for the file
        let highlights = get_highlights(&self.content, self.file_path.as_ref());

        let lines: Vec<&str> = self.content.split('\n').collect();
        let selection_range = self.selection_anchor.map(|a| {
            let start = a.min(self.cursor);
            let end = a.max(self.cursor);
            (start, end)
        });

        let mut byte_offset_at_line_start = 0;
        for i in 0..first_visible.min(lines.len()) {
            byte_offset_at_line_start += lines[i].len() + 1;
        }

        // Use a clipped painter for content area
        let painter = ui.painter().with_clip_rect(content_rect);

        for vis_idx in 0..visible_lines {
            let line_idx = first_visible + vis_idx;
            if line_idx >= lines.len() {
                break;
            }

            let y = content_rect.top() + vis_idx as f32 * line_height;

            // Line number
            let line_num = format!("{:>4}", line_idx + 1);
            painter.text(
                egui::pos2(gutter_rect.left() + 4.0, y),
                egui::Align2::LEFT_TOP,
                &line_num,
                font.clone(),
                crate::theme::TEXT_SECONDARY,
            );

            let line = lines[line_idx];
            let line_byte_start = byte_offset_at_line_start;
            let line_byte_end = line_byte_start + line.len();

            // Draw selection highlight
            if let Some((sel_start, sel_end)) = selection_range {
                if sel_start < line_byte_end && sel_end > line_byte_start {
                    let col_start = if sel_start > line_byte_start {
                        sel_start - line_byte_start
                    } else {
                        0
                    };
                    let col_end = if sel_end < line_byte_end {
                        sel_end - line_byte_start
                    } else {
                        line.len()
                    };
                    let sel_rect = Rect::from_min_size(
                        egui::pos2(text_rect.left() + col_start as f32 * char_width, y),
                        egui::vec2((col_end - col_start) as f32 * char_width, line_height),
                    );
                    painter.rect_filled(
                        sel_rect,
                        0.0,
                        crate::theme::ACCENT.linear_multiply(0.15),
                    );
                }
            }

            // Draw search match highlights
            for &(ms, me) in &self.search_matches {
                if ms < line_byte_end && me > line_byte_start {
                    let col_start = ms.saturating_sub(line_byte_start);
                    let col_end = (me - line_byte_start).min(line.len());
                    let hl_rect = Rect::from_min_size(
                        egui::pos2(text_rect.left() + col_start as f32 * char_width, y),
                        egui::vec2((col_end - col_start) as f32 * char_width, line_height),
                    );
                    painter.rect_filled(
                        hl_rect,
                        2.0,
                        Color32::from_rgba_premultiplied(255, 200, 0, 60),
                    );
                }
            }

            // Draw text with syntax highlighting
            render_highlighted_line(
                &painter,
                &font,
                char_width,
                text_rect.left(),
                y,
                line,
                line_byte_start,
                &highlights,
            );

            byte_offset_at_line_start = line_byte_end + 1; // +1 for '\n'
        }

        // Draw cursor
        if has_focus {
            let (c_line, c_col) = self.cursor_line_col();
            if c_line >= first_visible && c_line < first_visible + visible_lines {
                let vis = c_line - first_visible;
                let cx = text_rect.left() + c_col as f32 * char_width;
                let cy = content_rect.top() + vis as f32 * line_height;
                let cursor_rect = Rect::from_min_size(
                    egui::pos2(cx, cy),
                    egui::vec2(2.0, line_height),
                );
                painter.rect_filled(cursor_rect, 0.0, crate::theme::ACCENT);
            }
        }

        // Focus border
        if has_focus {
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, crate::theme::ACCENT),
                egui::StrokeKind::Outside,
            );
        }
    }
}

/// Simple syntax highlight token
#[derive(Clone)]
struct HighlightSpan {
    start: usize, // byte offset in content
    end: usize,
    color: Color32,
}

/// Extension-based keyword highlighting
/// TODO: Replace with tree-sitter for proper AST-based highlighting
fn get_highlights(content: &str, path: Option<&PathBuf>) -> Vec<HighlightSpan> {
    let ext = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let (keywords, types, constants) = match ext {
        "rs" => (
            &["fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait",
              "for", "while", "loop", "if", "else", "match", "return", "self", "Self",
              "crate", "super", "where", "async", "await", "move", "ref", "type", "const",
              "static", "unsafe", "extern", "as", "in", "break", "continue", "dyn"][..],
            &["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64",
              "u128", "usize", "f32", "f64", "bool", "char", "str", "String",
              "Vec", "Option", "Result", "Box", "Rc", "Arc", "HashMap", "HashSet"][..],
            &["true", "false", "None", "Some", "Ok", "Err"][..],
        ),
        "py" => (
            &["def", "class", "import", "from", "if", "elif", "else", "for", "while",
              "return", "yield", "with", "as", "try", "except", "finally", "raise",
              "pass", "break", "continue", "and", "or", "not", "in", "is", "lambda",
              "global", "nonlocal", "assert", "del", "async", "await"][..],
            &["int", "float", "str", "bool", "list", "dict", "tuple", "set", "None",
              "bytes", "type", "object"][..],
            &["True", "False", "None"][..],
        ),
        "js" | "ts" | "jsx" | "tsx" => (
            &["function", "const", "let", "var", "if", "else", "for", "while", "do",
              "return", "class", "extends", "new", "this", "super", "import", "export",
              "default", "from", "try", "catch", "finally", "throw", "async", "await",
              "yield", "switch", "case", "break", "continue", "typeof", "instanceof",
              "of", "in", "delete", "void"][..],
            &["string", "number", "boolean", "any", "void", "never", "unknown",
              "interface", "type", "enum", "namespace"][..],
            &["true", "false", "null", "undefined", "NaN", "Infinity"][..],
        ),
        "json" => (
            &[][..],
            &[][..],
            &["true", "false", "null"][..],
        ),
        _ => return Vec::new(),
    };

    let keyword_color = Color32::from_rgb(198, 120, 221);  // purple
    let type_color = Color32::from_rgb(229, 192, 123);     // yellow
    let constant_color = Color32::from_rgb(209, 154, 102); // orange
    let string_color = Color32::from_rgb(152, 195, 121);   // green
    let comment_color = Color32::from_rgb(150, 150, 150);  // gray
    let number_color = Color32::from_rgb(209, 154, 102);   // orange

    let mut spans = Vec::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        // Line comments
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            let start = i;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            spans.push(HighlightSpan { start, end: i, color: comment_color });
            continue;
        }

        // Hash comments (Python)
        if b == b'#' && (ext == "py") {
            let start = i;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            spans.push(HighlightSpan { start, end: i, color: comment_color });
            continue;
        }

        // Strings
        if b == b'"' || b == b'\'' {
            let quote = b;
            let start = i;
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i < len { i += 1; }
            spans.push(HighlightSpan { start, end: i, color: string_color });
            continue;
        }

        // Numbers
        if b.is_ascii_digit() && (i == 0 || !bytes[i-1].is_ascii_alphanumeric()) {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_') {
                i += 1;
            }
            spans.push(HighlightSpan { start, end: i, color: number_color });
            continue;
        }

        // Identifiers / keywords
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &content[start..i];
            if keywords.contains(&word) {
                spans.push(HighlightSpan { start, end: i, color: keyword_color });
            } else if types.contains(&word) {
                spans.push(HighlightSpan { start, end: i, color: type_color });
            } else if constants.contains(&word) {
                spans.push(HighlightSpan { start, end: i, color: constant_color });
            }
            continue;
        }

        i += 1;
    }

    spans
}

fn render_highlighted_line(
    painter: &egui::Painter,
    font: &FontId,
    char_width: f32,
    x_start: f32,
    y: f32,
    line: &str,
    line_byte_start: usize,
    highlights: &[HighlightSpan],
) {
    if line.is_empty() {
        return;
    }

    let line_byte_end = line_byte_start + line.len();

    let relevant: Vec<&HighlightSpan> = highlights
        .iter()
        .filter(|s| s.start < line_byte_end && s.end > line_byte_start)
        .collect();

    if relevant.is_empty() {
        painter.text(
            egui::pos2(x_start, y),
            egui::Align2::LEFT_TOP,
            line,
            font.clone(),
            crate::theme::TEXT_PRIMARY,
        );
        return;
    }

    let default_color = crate::theme::TEXT_PRIMARY;
    let mut col = 0;
    for (byte_idx, ch) in line.char_indices() {
        let abs_pos = line_byte_start + byte_idx;
        let color = relevant
            .iter()
            .find(|s| abs_pos >= s.start && abs_pos < s.end)
            .map(|s| s.color)
            .unwrap_or(default_color);

        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        painter.text(
            egui::pos2(x_start + col as f32 * char_width, y),
            egui::Align2::LEFT_TOP,
            s,
            font.clone(),
            color,
        );
        col += 1;
    }
}
