//! Red Tomato — 番茄工作法桌面应用（Rust + egui）

mod app;
mod pomodoro;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([380.0, 420.0])
            .with_title("番")
            .with_decorations(false) // 无系统标题栏，与钉住模式一致，仅保留自定义顶栏
            .with_icon(egui::IconData::default()),
        ..Default::default()
    };
    eframe::run_native(
        "番",
        options,
        Box::new(|cc| Ok(Box::new(app::RedTomatoApp::new(cc)))),
    )
}
