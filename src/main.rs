//! Red Tomato — 番茄工作法桌面应用（Rust + egui）

// 使用 Windows 图形子系统，运行时不弹出黑色控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod db;
mod pomodoro;

/// 生成应用图标：番茄红圆形，透明背景（48×48，任务栏/窗口更清晰）
fn make_app_icon() -> egui::IconData {
    const W: u32 = 48;
    const H: u32 = 48;
    // 番茄红（与 app 中进度条/番茄数一致）
    const R: u8 = 217;
    const G: u8 = 17;
    const B: u8 = 83;
    let cx = (W as f32) * 0.5;
    let cy = (H as f32) * 0.5;
    let r = (W.min(H) as f32) * 0.44;
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let dx = (x as f32) + 0.5 - cx;
            let dy = (y as f32) + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r {
                let a = 255u8;
                rgba.extend_from_slice(&[R, G, B, a]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    egui::IconData {
        rgba,
        width: W,
        height: H,
    }
}

fn main() -> eframe::Result<()> {
    let icon = make_app_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([380.0, 420.0])
            .with_title("番")
            .with_decorations(false) // 无系统标题栏，与钉住模式一致，仅保留自定义顶栏
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "番",
        options,
        Box::new(|cc| Ok(Box::new(app::RedTomatoApp::new(cc)))),
    )
}
