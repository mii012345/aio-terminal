use crate::terminal::Terminal;
use eframe::egui::{self, Color32, CornerRadius, FontId, Rect, Vec2};
use std::sync::{Arc, Mutex};

/// Message types in the chat view
#[derive(Clone, Debug)]
enum MessageKind {
    User,
    Assistant,
    Tool { name: String, collapsed: bool },
}

#[derive(Clone, Debug)]
struct ChatMessage {
    kind: MessageKind,
    content: String,
}

pub struct AgentView {
    terminal: Terminal,
    messages: Vec<ChatMessage>,
    input_buf: String,
    /// Raw output buffer from PTY, accumulated between parses
    raw_buf: Arc<Mutex<Vec<u8>>>,
    /// Track what we've already processed from the terminal
    last_screen_text: String,
    /// Whether we're currently accumulating assistant output
    accumulating_assistant: bool,
    id: usize,
    pub grab_focus: bool,
    /// Show raw terminal view (toggle)
    show_raw: bool,
}

static NEXT_AV_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl AgentView {
    pub fn new(terminal: Terminal) -> Self {
        let id = NEXT_AV_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            terminal,
            messages: Vec::new(),
            input_buf: String::new(),
            raw_buf: Arc::new(Mutex::new(Vec::new())),
            last_screen_text: String::new(),
            accumulating_assistant: false,
            id,
            grab_focus: false,
            show_raw: false,
        }
    }

    /// Get the underlying terminal (for resize, etc)
    pub fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }

    /// Poll terminal screen for new output and parse into messages
    fn poll_output(&mut self) {
        let current_text = self.terminal.screen_text();
        if current_text == self.last_screen_text {
            return;
        }

        // Find the new content
        let new_content = if current_text.len() > self.last_screen_text.len()
            && current_text.starts_with(&self.last_screen_text)
        {
            current_text[self.last_screen_text.len()..].to_string()
        } else if current_text != self.last_screen_text {
            // Screen was rewritten (scroll, clear, etc.) - take the diff approach
            // Just look at what changed
            let new = current_text.clone();
            // Find common prefix length
            let common = self
                .last_screen_text
                .chars()
                .zip(new.chars())
                .take_while(|(a, b)| a == b)
                .count();
            if common < new.len() {
                new[common..].to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        self.last_screen_text = current_text;

        if new_content.trim().is_empty() {
            return;
        }

        // Parse the new content into messages
        self.parse_new_content(&new_content);
    }

    fn parse_new_content(&mut self, content: &str) {
        let lines: Vec<&str> = content.lines().collect();

        for line in &lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Append newline to current assistant message if accumulating
                if self.accumulating_assistant {
                    if let Some(last) = self.messages.last_mut() {
                        if matches!(last.kind, MessageKind::Assistant) {
                            last.content.push('\n');
                        }
                    }
                }
                continue;
            }

            // Detect tool usage patterns
            if is_tool_line(trimmed) {
                let tool_name = extract_tool_name(trimmed);
                self.accumulating_assistant = false;
                self.messages.push(ChatMessage {
                    kind: MessageKind::Tool {
                        name: tool_name,
                        collapsed: true,
                    },
                    content: trimmed.to_string(),
                });
                continue;
            }

            // Detect prompt (user input echo) - lines starting with ❯ or > 
            if trimmed.starts_with("❯ ") || trimmed.starts_with("> ") || trimmed.starts_with("$ ") {
                self.accumulating_assistant = false;
                // Don't add as user message - we already add on input
                continue;
            }

            // Otherwise accumulate as assistant output
            if self.accumulating_assistant {
                if let Some(last) = self.messages.last_mut() {
                    if matches!(last.kind, MessageKind::Assistant) {
                        last.content.push('\n');
                        last.content.push_str(trimmed);
                        continue;
                    }
                }
            }

            // Start new assistant message
            self.accumulating_assistant = true;
            self.messages.push(ChatMessage {
                kind: MessageKind::Assistant,
                content: trimmed.to_string(),
            });
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui, rect: Rect) {
        // Toggle button for raw/chat view
        let toggle_rect = Rect::from_min_size(
            egui::pos2(rect.right() - 70.0, rect.top() + 2.0),
            Vec2::new(66.0, 20.0),
        );
        let toggle_id = ui.id().with(("av_toggle", self.id));
        let toggle_resp = ui.interact(toggle_rect, toggle_id, egui::Sense::click());
        if toggle_resp.clicked() {
            self.show_raw = !self.show_raw;
        }
        let toggle_label = if self.show_raw { "💬 Chat" } else { "🖥 Raw" };
        ui.painter().rect_filled(
            toggle_rect,
            CornerRadius::same(3),
            Color32::from_rgb(220, 220, 220),
        );
        ui.painter().text(
            toggle_rect.center(),
            egui::Align2::CENTER_CENTER,
            toggle_label,
            FontId::proportional(11.0),
            crate::theme::TEXT_PRIMARY,
        );

        if self.show_raw {
            self.terminal.render(ui, rect);
            return;
        }

        // Poll for new output
        self.poll_output();

        // Background
        ui.painter()
            .rect_filled(rect, 0.0, crate::theme::BG_BASE);

        let input_height = 36.0;
        let chat_rect = Rect::from_min_max(
            rect.left_top(),
            egui::pos2(rect.right(), rect.bottom() - input_height - 4.0),
        );
        let input_rect = Rect::from_min_max(
            egui::pos2(rect.left() + 4.0, rect.bottom() - input_height),
            egui::pos2(rect.right() - 4.0, rect.bottom() - 4.0),
        );

        // Render chat messages in scroll area
        let area_id = ui.id().with(("agent_chat_scroll", self.id));
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(chat_rect));
        egui::ScrollArea::vertical()
            .id_salt(area_id)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(&mut child_ui, |ui| {
                ui.set_min_width(chat_rect.width() - 16.0);
                ui.add_space(8.0);

                let max_bubble_width = (chat_rect.width() - 32.0) * 0.75;

                for (i, msg) in self.messages.iter_mut().enumerate() {
                    let msg_id = ui.id().with(("msg", i));
                    match &mut msg.kind {
                        MessageKind::User => {
                            render_user_message(ui, &msg.content, max_bubble_width);
                        }
                        MessageKind::Assistant => {
                            render_assistant_message(ui, &msg.content, max_bubble_width);
                        }
                        MessageKind::Tool {
                            name, collapsed, ..
                        } => {
                            render_tool_message(ui, name, &msg.content, collapsed, msg_id);
                        }
                    }
                    ui.add_space(6.0);
                }

                if self.messages.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Waiting for agent output...")
                                .color(crate::theme::TEXT_SECONDARY)
                                .size(14.0),
                        );
                    });
                }
            });

        // Input area
        ui.painter().rect_filled(
            input_rect,
            CornerRadius::same(6),
            Color32::from_rgb(245, 245, 245),
        );
        ui.painter().rect_stroke(
            input_rect,
            CornerRadius::same(6),
            egui::Stroke::new(1.0, crate::theme::BORDER),
            egui::StrokeKind::Outside,
        );

        // Text input
        let text_rect = Rect::from_min_max(
            egui::pos2(input_rect.left() + 8.0, input_rect.top() + 2.0),
            egui::pos2(input_rect.right() - 8.0, input_rect.bottom() - 2.0),
        );

        let input_id = ui.id().with(("agent_input", self.id));
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        let te = egui::TextEdit::singleline(&mut self.input_buf)
            .font(FontId::proportional(14.0))
            .desired_width(text_rect.width())
            .hint_text("Type a message...")
            .frame(false)
            .id(input_id);
        let response = child_ui.add(te);

        if self.grab_focus {
            response.request_focus();
            self.grab_focus = false;
        }

        // Handle enter
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let text = self.input_buf.trim().to_string();
            if !text.is_empty() {
                // Add as user message
                self.messages.push(ChatMessage {
                    kind: MessageKind::User,
                    content: text.clone(),
                });
                // Send to PTY
                self.terminal.write_input(text.as_bytes());
                self.terminal.write_input(b"\r");
                self.accumulating_assistant = false;
                self.input_buf.clear();
            }
            response.request_focus();
        }

        // Request repaint
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn render_user_message(ui: &mut egui::Ui, content: &str, max_width: f32) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        ui.set_max_width(max_width);
        egui::Frame::new()
            .fill(Color32::from_rgb(0, 122, 255))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_max_width(max_width - 24.0);
                ui.label(
                    egui::RichText::new(content)
                        .color(Color32::WHITE)
                        .size(13.0),
                );
            });
    });
}

fn render_assistant_message(ui: &mut egui::Ui, content: &str, max_width: f32) {
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(Color32::from_rgb(235, 235, 235))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_max_width(max_width - 24.0);
                render_markdown_simple(ui, content);
            });
    });
}

fn render_tool_message(
    ui: &mut egui::Ui,
    name: &str,
    content: &str,
    collapsed: &mut bool,
    _id: egui::Id,
) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(255, 243, 224))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_max_width(400.0);
                let header = format!("🔧 {}", name);
                if ui
                    .add(egui::Label::new(
                        egui::RichText::new(&header)
                            .color(Color32::from_rgb(230, 126, 34))
                            .size(12.0)
                            .strong(),
                    ).sense(egui::Sense::click()))
                    .clicked()
                {
                    *collapsed = !*collapsed;
                }
                if !*collapsed {
                    ui.label(
                        egui::RichText::new(content)
                            .color(crate::theme::TEXT_SECONDARY)
                            .size(11.0)
                            .monospace(),
                    );
                }
            });
    });
}

/// Simple markdown rendering: bold, code blocks, inline code, lists
fn render_markdown_simple(ui: &mut egui::Ui, text: &str) {
    let mut in_code_block = false;
    let mut code_buf = String::new();

    for line in text.lines() {
        if line.starts_with("```") {
            if in_code_block {
                // End code block
                ui.label(
                    egui::RichText::new(&code_buf)
                        .monospace()
                        .size(12.0)
                        .color(crate::theme::TEXT_PRIMARY)
                        .background_color(Color32::from_rgb(240, 240, 240)),
                );
                code_buf.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            if !code_buf.is_empty() {
                code_buf.push('\n');
            }
            code_buf.push_str(line);
            continue;
        }

        // List items
        if line.starts_with("- ") || line.starts_with("* ") {
            ui.label(
                egui::RichText::new(format!("  • {}", &line[2..]))
                    .size(13.0)
                    .color(crate::theme::TEXT_PRIMARY),
            );
            continue;
        }

        // Numbered list
        if line.len() > 2 && line.chars().next().map_or(false, |c| c.is_ascii_digit()) && line.contains(". ") {
            ui.label(
                egui::RichText::new(format!("  {}", line))
                    .size(13.0)
                    .color(crate::theme::TEXT_PRIMARY),
            );
            continue;
        }

        // Normal text with inline formatting
        render_inline_text(ui, line);
    }

    if in_code_block && !code_buf.is_empty() {
        ui.label(
            egui::RichText::new(&code_buf)
                .monospace()
                .size(12.0)
                .color(crate::theme::TEXT_PRIMARY)
                .background_color(Color32::from_rgb(240, 240, 240)),
        );
    }
}

fn render_inline_text(ui: &mut egui::Ui, line: &str) {
    // Simple: just render as-is with bold detection
    // Could be enhanced but keeping it simple
    if line.contains("**") {
        // Has bold markers - render with bold
        let parts: Vec<&str> = line.split("**").collect();
        let mut job = egui::text::LayoutJob::default();
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            let format = if i % 2 == 1 {
                egui::TextFormat {
                    font_id: FontId::proportional(13.0),
                    color: crate::theme::TEXT_PRIMARY,
                    ..Default::default()
                }
            } else {
                egui::TextFormat {
                    font_id: FontId::proportional(13.0),
                    color: crate::theme::TEXT_PRIMARY,
                    ..Default::default()
                }
            };
            // For bold parts (odd index)
            let format = if i % 2 == 1 {
                egui::TextFormat {
                    font_id: FontId::new(13.0, egui::FontFamily::Proportional),
                    color: crate::theme::TEXT_PRIMARY,
                    ..Default::default()
                }
            } else {
                format
            };
            job.append(part, 0.0, format);
        }
        ui.label(job);
    } else if line.contains('`') {
        // Has inline code
        let parts: Vec<&str> = line.split('`').collect();
        let mut job = egui::text::LayoutJob::default();
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            let format = if i % 2 == 1 {
                egui::TextFormat {
                    font_id: FontId::monospace(12.0),
                    color: crate::theme::TEXT_PRIMARY,
                    background: Color32::from_rgb(240, 240, 240),
                    ..Default::default()
                }
            } else {
                egui::TextFormat {
                    font_id: FontId::proportional(13.0),
                    color: crate::theme::TEXT_PRIMARY,
                    ..Default::default()
                }
            };
            job.append(part, 0.0, format);
        }
        ui.label(job);
    } else {
        ui.label(
            egui::RichText::new(line)
                .size(13.0)
                .color(crate::theme::TEXT_PRIMARY),
        );
    }
}

fn is_tool_line(line: &str) -> bool {
    let patterns = [
        "Read(", "Write(", "Edit(", "Bash(", "bash(",
        "MultiEdit(", "Glob(", "Grep(", "LS(",
        "TodoRead(", "TodoWrite(",
        "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", // spinner
    ];
    patterns.iter().any(|p| line.contains(p))
}

fn extract_tool_name(line: &str) -> String {
    let tools = [
        "Read", "Write", "Edit", "Bash", "bash",
        "MultiEdit", "Glob", "Grep", "LS",
        "TodoRead", "TodoWrite",
    ];
    for t in &tools {
        if line.contains(&format!("{}(", t)) {
            return t.to_string();
        }
    }
    "Tool".to_string()
}
