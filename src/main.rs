mod app;
mod file_tree;
mod pane;
mod terminal;
mod theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("AiO Terminal"),
        ..Default::default()
    };

    eframe::run_native(
        "AiO Terminal",
        options,
        Box::new(|cc| Ok(Box::new(app::AioApp::new(cc)))),
    )
}
