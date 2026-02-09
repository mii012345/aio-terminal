use eframe::egui::{self, Color32, FontId, Rect};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

pub struct Terminal {
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    rows: u16,
    cols: u16,
    id: usize,
}

static NEXT_TERM_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Terminal {
    pub fn new(rows: u16, cols: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new_default_prog();
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let parser_clone = parser.clone();

        // Background thread to read PTY output
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut p) = parser_clone.lock() {
                            p.process(&buf[..n]);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let id = NEXT_TERM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Ok(Self {
            parser,
            writer: Arc::new(Mutex::new(writer)),
            _child: child,
            rows,
            cols,
            id,
        })
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows != self.rows || cols != self.cols {
            self.rows = rows;
            self.cols = cols;
            if let Ok(mut p) = self.parser.lock() {
                p.set_size(rows, cols);
            }
            // TODO: also resize the PTY master fd (portable-pty MasterPty::resize)
        }
    }

    pub fn write_input(&self, data: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(data);
            let _ = w.flush();
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui, rect: Rect) {
        // Background
        ui.painter()
            .rect_filled(rect, 0.0, crate::theme::TERMINAL_BG);

        let font = FontId::monospace(14.0);
        let char_width = 8.4_f32;
        let line_height = 17.0_f32;

        // Calculate visible size and resize if needed
        let visible_cols = ((rect.width() - 4.0) / char_width).floor().max(1.0) as u16;
        let visible_rows = ((rect.height() - 4.0) / line_height).floor().max(1.0) as u16;
        self.resize(visible_rows, visible_cols);

        // Handle keyboard input - unique ID per terminal instance
        let unique_id = ui.id().with(("terminal_input", self.id));
        let response = ui.interact(rect, unique_id, egui::Sense::click());
        if response.clicked() {
            ui.memory_mut(|mem| mem.request_focus(unique_id));
        }

        let has_focus = ui.memory(|mem| mem.has_focus(unique_id));

        if has_focus {
            ui.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) => {
                            self.write_input(text.as_bytes());
                        }
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            let seq = key_to_escape(*key, modifiers);
                            if !seq.is_empty() {
                                self.write_input(&seq);
                            }
                        }
                        _ => {}
                    }
                }
            });
        }

        // Render cells from vt100
        if let Ok(parser) = self.parser.lock() {
            let screen = parser.screen();
            // TODO: scrollback rendering

            for row in 0..visible_rows {
                for col in 0..visible_cols {
                    if let Some(cell) = screen.cell(row, col) {
                        let ch = cell.contents();
                        if ch.is_empty() || ch == " " {
                            continue;
                        }

                        let fg = vt100_color_to_egui(cell.fgcolor(), true);
                        let pos = egui::pos2(
                            rect.left() + 2.0 + col as f32 * char_width,
                            rect.top() + 2.0 + row as f32 * line_height,
                        );
                        ui.painter().text(
                            pos,
                            egui::Align2::LEFT_TOP,
                            &ch,
                            font.clone(),
                            fg,
                        );
                    }
                }
            }

            // Draw cursor if focused
            if has_focus {
                let (cursor_row, cursor_col) = screen.cursor_position();
                let cursor_rect = Rect::from_min_size(
                    egui::pos2(
                        rect.left() + 2.0 + cursor_col as f32 * char_width,
                        rect.top() + 2.0 + cursor_row as f32 * line_height,
                    ),
                    egui::vec2(char_width, line_height),
                );
                ui.painter().rect_filled(
                    cursor_rect,
                    0.0,
                    Color32::from_rgba_premultiplied(200, 200, 200, 128),
                );
            }
        }

        // Focus indicator border
        if has_focus {
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, crate::theme::ACCENT),
                egui::StrokeKind::Outside,
            );
        }

        // Request repaint for terminal updates
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
}

fn vt100_color_to_egui(color: vt100::Color, is_fg: bool) -> Color32 {
    match color {
        vt100::Color::Default => {
            if is_fg {
                Color32::from_rgb(36, 36, 36)
            } else {
                crate::theme::TERMINAL_BG
            }
        }
        vt100::Color::Idx(i) => ansi_256_to_color32(i),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

fn ansi_256_to_color32(idx: u8) -> Color32 {
    const BASIC: [(u8, u8, u8); 16] = [
        (0, 0, 0),       (205, 49, 49),    (13, 188, 121),   (229, 229, 16),
        (36, 114, 200),   (188, 63, 188),   (17, 168, 205),   (229, 229, 229),
        (102, 102, 102),  (241, 76, 76),    (35, 209, 139),   (245, 245, 67),
        (59, 142, 234),   (214, 112, 214),  (41, 184, 219),   (255, 255, 255),
    ];

    if idx < 16 {
        let (r, g, b) = BASIC[idx as usize];
        return Color32::from_rgb(r, g, b);
    }

    if idx < 232 {
        let idx = idx - 16;
        let r = (idx / 36) % 6;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let to_val = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
        return Color32::from_rgb(to_val(r), to_val(g), to_val(b));
    }

    let v = 8 + 10 * (idx - 232);
    Color32::from_rgb(v, v, v)
}

fn key_to_escape(key: egui::Key, modifiers: &egui::Modifiers) -> Vec<u8> {
    if modifiers.ctrl {
        match key {
            egui::Key::C => return vec![3],
            egui::Key::D => return vec![4],
            egui::Key::Z => return vec![26],
            egui::Key::L => return vec![12],
            egui::Key::A => return vec![1],
            egui::Key::E => return vec![5],
            egui::Key::K => return vec![11],
            egui::Key::U => return vec![21],
            egui::Key::W => return vec![23],
            _ => {}
        }
    }

    match key {
        egui::Key::Enter => vec![13],
        egui::Key::Tab => vec![9],
        egui::Key::Backspace => vec![127],
        egui::Key::Escape => vec![27],
        egui::Key::ArrowUp => b"\x1b[A".to_vec(),
        egui::Key::ArrowDown => b"\x1b[B".to_vec(),
        egui::Key::ArrowRight => b"\x1b[C".to_vec(),
        egui::Key::ArrowLeft => b"\x1b[D".to_vec(),
        egui::Key::Home => b"\x1b[H".to_vec(),
        egui::Key::End => b"\x1b[F".to_vec(),
        egui::Key::PageUp => b"\x1b[5~".to_vec(),
        egui::Key::PageDown => b"\x1b[6~".to_vec(),
        egui::Key::Delete => b"\x1b[3~".to_vec(),
        _ => vec![],
    }
}
