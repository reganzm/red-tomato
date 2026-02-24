//! egui 主界面：计时显示、阶段选择、开始/暂停、番茄数

use eframe::egui;
use egui::emath::NumExt;
use chrono::Utc;
use raw_window_handle::HasWindowHandle;
use std::sync::Arc;

use crate::pomodoro::{Phase, PomodoroState, TimerState};

/// 桌面右上角边距（逻辑像素）
const PIN_MARGIN: f32 = 16.0;

/// White Text 主题色（参考 OnePomodoro WhiteTextView.xaml.cs）
mod white_text_theme {
    /// 专注/番茄阶段：红 PointLight
    pub const FOCUS_RGB: (u8, u8, u8) = (217, 17, 83);
    /// 休息阶段：蓝 PointLight
    pub const RELAX_RGB: (u8, u8, u8) = (255, 193, 7); // 黄色
    /// 深色背景（接近黑）
    pub const BG_RGB: (u8, u8, u8) = (18, 18, 24);
    /// 主文字白
    pub const TEXT_WHITE: (u8, u8, u8) = (255, 255, 255);
    /// 次要文字
    pub const TEXT_DIM: (u8, u8, u8) = (200, 200, 210);
}

/// 紧凑 overlay 尺寸（保证「继续」「停止」等按钮完整显示）
const COMPACT_WIDTH: f32 = 300.0;
const COMPACT_HEIGHT: f32 = 165.0;

/// 设置中文字体，避免中文乱码。优先使用系统自带字体。
fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    #[cfg(windows)]
    let system_font_paths = [
        r"C:\Windows\Fonts\msyh.ttc",   // 微软雅黑
        r"C:\Windows\Fonts\simhei.ttf", // 黑体
        r"C:\Windows\Fonts\simsun.ttc",  // 宋体
    ];

    #[cfg(not(windows))]
    let system_font_paths: [&str; 0] = [];

    for path in system_font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            fonts.font_data.insert(
                "chinese".to_owned(),
                Arc::new(egui::FontData::from_static(leaked)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "chinese".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }

    // 非 Windows 或系统字体未找到时，使用内置后备字体（仅基本拉丁字符，中文仍可能方框）
    // 可后续将 Noto Sans SC 等放入 assets 并 include_bytes 以支持跨平台中文
    #[allow(unused)]
    if let Some(embedded) = option_env!("RED_TOMATO_FONT_PATH") {
        if let Ok(bytes) = std::fs::read(embedded) {
            let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            fonts.font_data.insert(
                "chinese".to_owned(),
                Arc::new(egui::FontData::from_static(leaked)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".to_owned());
            ctx.set_fonts(fonts);
        }
    }
}

/// 完整模式默认窗口尺寸
const FULL_SIZE: (f32, f32) = (380.0, 420.0);

pub struct RedTomatoApp {
    pub pomo: PomodoroState,
    compact: bool,
    pinned: bool,
    pin_applied: bool,
    compact_size_applied: bool,
    /// 从紧凑回到完整时，是否已恢复尺寸
    full_restore_applied: bool,
    /// 非钉住模式下是否已去掉系统标题栏（与钉住模式一致，仅保留自定义顶栏）
    full_no_decorations_applied: bool,
    /// 是否已去掉标题栏左上角系统菜单（仅 Windows 非紧凑模式，有标题栏时用）
    system_menu_removed: bool,
    /// 是否显示「关于」窗口
    show_about: bool,
}

impl Default for RedTomatoApp {
    fn default() -> Self {
        Self {
            pomo: PomodoroState::default(),
            compact: false,
            pinned: false,
            pin_applied: false,
            compact_size_applied: false,
            full_restore_applied: true,
            full_no_decorations_applied: false,
            system_menu_removed: false,
            show_about: false,
        }
    }
}

/// Windows：去掉标题栏左上角系统菜单（点击图标时的下拉菜单）
#[cfg(windows)]
fn try_remove_system_menu(frame: &eframe::Frame) -> bool {
    use std::ffi::c_void;
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_STYLE, WS_SYSMENU};

    let opt = frame.window_handle().ok();
    let handle = match opt.as_ref() {
        Some(h) => h.as_ref(),
        None => return false,
    };
    let hwnd: isize = match handle {
        RawWindowHandle::Win32(w) => w.hwnd.get(),
        _ => return false,
    };
    if hwnd == 0 {
        return false;
    }
    let style = unsafe { GetWindowLongPtrW(hwnd as *mut c_void, GWL_STYLE) };
    if style == 0 {
        return false;
    }
    let new_style = style & !(WS_SYSMENU as isize);
    if new_style == style {
        return true; // 已经去掉
    }
    unsafe { SetWindowLongPtrW(hwnd as *mut c_void, GWL_STYLE, new_style) };
    true
}

#[cfg(not(windows))]
fn try_remove_system_menu(_frame: &eframe::Frame) -> bool {
    false
}

/// 计算窗口钉在桌面右上角时的位置
fn pin_position_top_right(ctx: &egui::Context) -> Option<egui::Pos2> {
    ctx.input(|i| {
        let outer_rect = i.viewport().outer_rect?;
        let size = outer_rect.size();
        let monitor_size = i.viewport().monitor_size?;
        if 1.0 < monitor_size.x && 1.0 < monitor_size.y {
            let x = monitor_size.x - size.x - PIN_MARGIN;
            let y = PIN_MARGIN;
            Some(egui::pos2(x, y))
        } else {
            None
        }
    })
}

/// 应用 pin 状态：置顶 + 移到右上角。返回是否成功应用了位置（用于重试）
fn apply_pin(ctx: &egui::Context) -> bool {
    use egui::viewport::{ViewportCommand, WindowLevel};
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
    if let Some(pos) = pin_position_top_right(ctx) {
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        true
    } else {
        false
    }
}

/// 取消 pin：恢复普通窗口层级
fn apply_unpin(ctx: &egui::Context) {
    use egui::viewport::{ViewportCommand, WindowLevel};
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
}

/// 绘制 subtle 几何背景（类似 WhiteText 的深色质感）
fn paint_subtle_pattern(ui: &mut egui::Ui, rect: egui::Rect) {
    let painter = ui.painter();
    let step = 16.0;
    let r = 1.2;
    let alpha = 12u8;
    let color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
    let mut y = rect.min.y;
    while y < rect.max.y {
        let mut x = rect.min.x + (step * 0.5 * ((y - rect.min.y) / step).floor() % 2.0);
        while x < rect.max.x {
            painter.circle(egui::Pos2::new(x, y), r, color, egui::Stroke::NONE);
            x += step;
        }
        y += step;
    }
}

/// 番茄数：一排小圆形，已完成的填色（番茄红），未完成的描边
fn paint_pomodoro_circles(ui: &mut egui::Ui, n: u32, done: u32) {
    const RADIUS: f32 = 8.0;
    const SPACING: f32 = 6.0;
    let size = egui::vec2(
        n as f32 * (RADIUS * 2.0 + SPACING) - SPACING,
        RADIUS * 2.0,
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let filled_color = egui::Color32::from_rgb(217, 17, 83); // 番茄红
    let stroke_color = egui::Color32::from_rgb(80, 80, 90);
    let stroke = egui::Stroke::new(1.5, stroke_color);
    for i in 0..n {
        let cx = rect.min.x + RADIUS + i as f32 * (RADIUS * 2.0 + SPACING);
        let cy = rect.center().y;
        let center = egui::Pos2::new(cx, cy);
        if i < done {
            painter.circle_filled(center, RADIUS, filled_color);
            painter.circle_stroke(center, RADIUS, stroke);
        } else {
            painter.circle_stroke(center, RADIUS, stroke);
        }
    }
}

/// 带文字居中显示的按钮，返回 Response（与 egui::Button 一致便于 .clicked()）
fn centered_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>, size: egui::Vec2) -> egui::Response {
    let size = size.at_least(egui::vec2(ui.spacing().interact_size.x, ui.spacing().interact_size.y));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.style().interact(&response);
    let expanded = rect.expand(visuals.expansion);
    ui.painter().rect_filled(expanded, visuals.corner_radius, visuals.bg_fill);
    ui.painter().rect_stroke(
        expanded,
        visuals.corner_radius,
        visuals.bg_stroke,
        egui::StrokeKind::Outside,
    );
    let widget_text: egui::WidgetText = text.into();
    let galley = widget_text.into_galley(ui, None, rect.width() - 8.0, egui::TextStyle::Button);
    let pos = rect.center() - galley.size() / 2.0;
    let text_color = if ui.is_enabled() {
        ui.visuals().text_color()
    } else {
        ui.visuals().gray_out(ui.visuals().text_color())
    };
    ui.painter().galley(pos, galley, text_color);
    response
}

/// 番茄/休息阶段结束时播放系统提示音
fn play_phase_finished_sound() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", "[Console]::Beep(800, 300)"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("echo").arg("\x07").status();
    }
}

impl RedTomatoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_chinese_fonts(&cc.egui_ctx);
        Self::default()
    }

    fn phase_label(phase: Phase) -> &'static str {
        match phase {
            Phase::Focus => "专注",
            Phase::ShortBreak => "短休息",
            Phase::LongBreak => "长休息",
        }
    }
}

impl eframe::App for RedTomatoApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.pomo.tick(Utc::now());
        if self.pomo.take_finished_phase().is_some() {
            play_phase_finished_sound();
        }
        ctx.request_repaint();

        // 应用 pin：默认钉在右上角并置顶（首帧可能无 monitor 信息，会下一帧重试）
        if self.pinned && !self.pin_applied {
            self.pin_applied = apply_pin(ctx);
        }

        // 紧凑模式（钉到右上角）：小窗 + 无标题栏
        if self.compact && !self.compact_size_applied {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                COMPACT_WIDTH,
                COMPACT_HEIGHT,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            self.compact_size_applied = true;
            self.full_no_decorations_applied = false;
            self.system_menu_removed = false;
        }

        // 非钉住模式：也去掉系统标题栏，只保留自定义顶栏（钉子+关闭）
        if !self.compact && !self.full_no_decorations_applied {
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            self.full_no_decorations_applied = true;
        }

        // 从紧凑回到完整模式：恢复窗口尺寸（不恢复系统标题栏）
        if !self.compact && !self.full_restore_applied {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                FULL_SIZE.0,
                FULL_SIZE.1,
            )));
            self.full_restore_applied = true;
            self.full_no_decorations_applied = false; // 下一帧会再次发 Decorations(false)
            self.system_menu_removed = false;
        }

        // 非紧凑模式下去掉标题栏左上角系统菜单（仅 Windows，只执行一次）
        if !self.compact && !self.system_menu_removed && try_remove_system_menu(frame) {
            self.system_menu_removed = true;
        }

        if self.compact {
            self.ui_compact(ctx);
        } else {
            self.ui_full(ctx);
        }

        // 关于窗口（点击导航栏「关于」后展示）
        if self.show_about {
            self.ui_about(ctx);
        }
    }
}

impl RedTomatoApp {
    /// 关于窗口
    fn ui_about(&mut self, ctx: &egui::Context) {
        use white_text_theme::TEXT_DIM;
        egui::Window::new("关于")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("Red Tomato 红番茄")
                            .size(18.0)
                            .color(egui::Color32::from_rgb(TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2)),
                    );
                    ui.label(
                        egui::RichText::new("科学工作法")
                            .size(14.0)
                            .color(egui::Color32::from_rgb(TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2)),
                    );
                    ui.add_space(16.0);
                    if ui.button("确定").clicked() {
                        self.show_about = false;
                    }
                });
            });
    }

    fn ui_full(&mut self, ctx: &egui::Context) {
        use white_text_theme::BG_RGB;

        // 进度条颜色：专注绿、短休息黄、长休息红
        let (r, g, b) = match self.pomo.phase {
            Phase::Focus => (100, 220, 130),       // 绿色
            Phase::ShortBreak => (255, 193, 7),    // 黄色
            Phase::LongBreak => (217, 17, 83),     // 红色
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(BG_RGB.0, BG_RGB.1, BG_RGB.2)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    // 顶行：与钉住模式一致，仅钉子图标 + 关闭按钮（.frame(false) 无边框）
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new("📌").frame(false))
                            .on_hover_text("钉到桌面右上角")
                            .clicked()
                        {
                            self.pinned = true;
                            self.compact = true;
                            self.compact_size_applied = false;
                            self.pin_applied = false;
                        }
                        ui.add_space(ui.available_width() - 40.0);
                        if ui
                            .add(egui::Button::new("×").frame(false))
                            .on_hover_text("关闭")
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);

                    // 所处阶段文案，颜色与进度条一致（随阶段切换：绿/蓝/红）
                    ui.label(
                        egui::RichText::new(Self::phase_label(self.pomo.phase))
                            .color(egui::Color32::from_rgb(r, g, b))
                            .size(18.0),
                    );
                    ui.add_space(8.0);

                    // 大计时器（白字 + 红/蓝 accent 风格）
                    ui.label(
                        egui::RichText::new(self.pomo.remaining_display())
                            .color(egui::Color32::from_rgb(255, 255, 255))
                            .size(56.0)
                            .monospace(),
                    );
                    ui.add_space(4.0);

                    // 进度条（红/蓝）
                    let progress = self.pomo.progress();
                    let bar = egui::ProgressBar::new(progress)
                        .desired_width(280.0)
                        .fill(egui::Color32::from_rgb(r, g, b));
                    ui.add(bar);
                    ui.add_space(20.0);

                    // 开始/暂停、停止 同一行（文字居中）；钉住已移至左上角钉子图标
                    let btn_size = egui::vec2(88.0, 36.0);
                    ui.horizontal(|ui| {
                        let (label, action) = match self.pomo.state {
                            TimerState::Idle => ("开始", 0u8),
                            TimerState::Running => ("暂停", 1u8),
                            TimerState::Paused => ("继续", 2u8),
                        };
                        if centered_button(ui, label, btn_size).on_hover_text(match action {
                            0 => "开始计时",
                            1 => "暂停",
                            _ => "继续",
                        }).clicked() {
                            match action {
                                0 => self.pomo.start(),
                                1 | 2 => self.pomo.toggle_pause(),
                                _ => {}
                            }
                        }
                        if self.pomo.state != TimerState::Idle {
                            if centered_button(ui, "停止", btn_size).clicked() {
                                self.pomo.stop();
                            }
                        }
                    });
                    ui.add_space(24.0);

                    // 阶段选择（仅 Idle 时可切换）
                    ui.horizontal(|ui| {
                        ui.label("阶段：");
                        for phase in [Phase::Focus, Phase::ShortBreak, Phase::LongBreak] {
                            let selected = self.pomo.phase == phase && self.pomo.state == TimerState::Idle;
                            let label = Self::phase_label(phase);
                            let btn = egui::Button::new(label);
                            let resp = ui.add_enabled(self.pomo.state == TimerState::Idle, btn);
                            if resp.clicked() {
                                self.pomo.set_phase(phase);
                            }
                            if selected {
                                resp.highlight();
                            }
                        }
                    });
                    ui.add_space(12.0);

                    // 番茄数：与「阶段：」相同字体格式（普通 label）
                    ui.horizontal(|ui| {
                        ui.label("番茄数 ");
                        let n = self.pomo.config.pomodoros_before_long;
                        let done = self.pomo.completed_pomodoros;
                        paint_pomodoro_circles(ui, n, done);
                    });
                    ui.add_space(8.0);
                    if ui.link("关于").clicked() {
                        self.show_about = true;
                    }
                    ui.add_space(12.0);
                });
            });
    }

    fn ui_compact(&mut self, ctx: &egui::Context) {
        use white_text_theme::{BG_RGB, TEXT_WHITE};

        // 进度条颜色：专注绿、短休息黄、长休息红
        let (accent_r, accent_g, accent_b) = match self.pomo.phase {
            Phase::Focus => (100, 220, 130),       // 绿色
            Phase::ShortBreak => (255, 193, 7),    // 黄色
            Phase::LongBreak => (217, 17, 83),     // 红色
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(BG_RGB.0, BG_RGB.1, BG_RGB.2)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                // 背景几何图案（类似 WhiteText 的质感）
                paint_subtle_pattern(ui, rect);

                ui.vertical_centered(|ui| {
                    // 顶行：取消钉住（左，钉子图标）+ 关闭（右）
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new("📌").frame(false))
                            .on_hover_text("取消钉住，恢复完整窗口")
                            .clicked()
                        {
                            self.pinned = false;
                            self.compact = false;
                            self.compact_size_applied = false;
                            self.full_restore_applied = false;
                            apply_unpin(ctx);
                        }
                        ui.add_space(ui.available_width() - 40.0);
                        if ui
                            .add(egui::Button::new("×").frame(false))
                            .on_hover_text("关闭")
                            .clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(2.0);

                    // 大号白字计时（White Text 风格）
                    ui.label(
                        egui::RichText::new(self.pomo.remaining_display())
                            .color(egui::Color32::from_rgb(TEXT_WHITE.0, TEXT_WHITE.1, TEXT_WHITE.2))
                            .size(42.0)
                            .monospace(),
                    );
                    ui.add_space(2.0);

                    // 所处阶段文案，颜色与进度条一致（随阶段切换：绿/蓝/红）
                    let phase_text = match self.pomo.phase {
                        Phase::Focus => "专注",
                        Phase::ShortBreak => "短休息",
                        Phase::LongBreak => "长休息",
                    };
                    ui.label(
                        egui::RichText::new(phase_text)
                            .color(egui::Color32::from_rgb(accent_r, accent_g, accent_b))
                            .size(14.0),
                    );
                    ui.add_space(8.0);

                    // 进度条（红/蓝 accent），宽度略小于窗口以留出边距
                    let progress = self.pomo.progress();
                    let bar_width = (ui.available_width() - 24.0).at_least(200.0);
                    let bar = egui::ProgressBar::new(progress)
                        .desired_width(bar_width)
                        .fill(egui::Color32::from_rgb(accent_r, accent_g, accent_b));
                    ui.add(bar);
                    ui.add_space(6.0);

                    // 开始/暂停（一个按钮切换）+ 停止（文字居中），按可用宽度分配避免裁切
                    let compact_btn = egui::vec2(72.0, 28.0);
                    ui.horizontal(|ui| {
                        let available = ui.available_width();
                        let need = compact_btn.x * 2.0 + 12.0;
                        let btn_w = if available >= need { compact_btn.x } else { ((available - 12.0) / 2.0).at_least(44.0) };
                        let btn_size = egui::vec2(btn_w, compact_btn.y);
                        let (label, action) = match self.pomo.state {
                            TimerState::Idle => ("开始", 0u8),
                            TimerState::Running => ("暂停", 1u8),
                            TimerState::Paused => ("继续", 2u8),
                        };
                        if centered_button(ui, label, btn_size).clicked() {
                            if action == 0 {
                                self.pomo.start();
                            } else {
                                self.pomo.toggle_pause();
                            }
                        }
                        if self.pomo.state != TimerState::Idle {
                            if centered_button(ui, "停止", btn_size).clicked() {
                                self.pomo.stop();
                            }
                        }
                    });
                });
            });
    }
}
