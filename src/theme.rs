use eframe::egui::{self, Color32, Visuals};

pub struct Theme;

// Light theme colors
pub const BG_BASE: Color32 = Color32::from_rgb(250, 250, 250);
pub const BG_SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(243, 243, 243);
pub const BORDER: Color32 = Color32::from_rgb(218, 218, 218);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(36, 36, 36);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(120, 120, 120);
pub const ACCENT: Color32 = Color32::from_rgb(0, 122, 255);
pub const TAB_ACTIVE: Color32 = Color32::from_rgb(255, 255, 255);
pub const TAB_INACTIVE: Color32 = Color32::from_rgb(238, 238, 238);
pub const TERMINAL_BG: Color32 = Color32::from_rgb(255, 255, 255);

impl Theme {
    pub fn apply(ctx: &egui::Context) {
        let mut visuals = Visuals::light();
        visuals.panel_fill = BG_BASE;
        visuals.window_fill = BG_SURFACE;
        visuals.faint_bg_color = BG_ELEVATED;
        visuals.widgets.noninteractive.bg_fill = BG_SURFACE;
        visuals.widgets.inactive.bg_fill = BG_ELEVATED;
        visuals.selection.bg_fill = ACCENT.linear_multiply(0.15);
        ctx.set_visuals(visuals);
    }
}
