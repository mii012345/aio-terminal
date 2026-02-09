use eframe::egui::{self, Color32, Visuals};

pub struct Theme;

// Zed-inspired dark colors
pub const BG_BASE: Color32 = Color32::from_rgb(30, 30, 30);
pub const BG_SURFACE: Color32 = Color32::from_rgb(37, 37, 37);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(45, 45, 45);
pub const BORDER: Color32 = Color32::from_rgb(60, 60, 60);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(220, 220, 220);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(140, 140, 140);
pub const ACCENT: Color32 = Color32::from_rgb(80, 150, 255);
pub const TAB_ACTIVE: Color32 = Color32::from_rgb(50, 50, 50);
pub const TAB_INACTIVE: Color32 = Color32::from_rgb(35, 35, 35);
pub const TERMINAL_BG: Color32 = Color32::from_rgb(24, 24, 24);

impl Theme {
    pub fn apply(ctx: &egui::Context) {
        let mut visuals = Visuals::dark();
        visuals.panel_fill = BG_BASE;
        visuals.window_fill = BG_SURFACE;
        visuals.faint_bg_color = BG_ELEVATED;
        visuals.widgets.noninteractive.bg_fill = BG_SURFACE;
        visuals.widgets.inactive.bg_fill = BG_ELEVATED;
        visuals.selection.bg_fill = ACCENT.linear_multiply(0.3);
        ctx.set_visuals(visuals);
    }
}
