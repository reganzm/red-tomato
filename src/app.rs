//! egui 主界面：计时显示、阶段选择、开始/暂停、番茄数、任务与专注历史持久化

use eframe::egui;
use egui::emath::NumExt;
use chrono::{FixedOffset, Utc};
use raw_window_handle::HasWindowHandle;
use serde::{Deserialize, Serialize};
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

/// 紧凑 overlay 尺寸（保证进度条+「开始/暂停」按钮完整显示，留足垂直空间以兼容高 DPI/缩放）
const COMPACT_WIDTH: f32 = 300.0;
const COMPACT_HEIGHT: f32 = 228.0;

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

/// 存储键：任务 + 番茄钟状态 + 专注历史（JSON）
const STORAGE_KEY_STATE: &str = "red_tomato_state";

/// 北京时区 UTC+8（专注记录完成时间用）
fn beijing_now_rfc3339() -> String {
    let beijing = FixedOffset::east_opt(8 * 3600).unwrap();
    Utc::now().with_timezone(&beijing).to_rfc3339()
}

/// 单条专注记录：用于按时间统计做了哪些任务（与 SQLite focus_records 表一致）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FocusRecord {
    pub task: String,
    pub duration_secs: i64,
    /// 完成时间 ISO 8601
    pub completed_at: String,
    /// 完成时的番茄数（本周期内）
    pub completed_pomodoros: u32,
}

/// 持久化到 eframe storage 的会话状态（专注历史存 SQLite，不在此）
#[derive(Serialize, Deserialize)]
struct PersistedState {
    current_task: String,
    phase: String,
    state: String,
    remaining_secs: i64,
    phase_total_secs: i64,
    completed_pomodoros: u32,
}

fn phase_to_str(p: Phase) -> &'static str {
    match p {
        Phase::Focus => "Focus",
        Phase::ShortBreak => "ShortBreak",
        Phase::LongBreak => "LongBreak",
    }
}
fn phase_from_str(s: &str) -> Phase {
    match s {
        "ShortBreak" => Phase::ShortBreak,
        "LongBreak" => Phase::LongBreak,
        _ => Phase::Focus,
    }
}
fn state_to_str(s: TimerState) -> &'static str {
    match s {
        TimerState::Idle => "Idle",
        TimerState::Running => "Running",
        TimerState::Paused => "Paused",
    }
}
fn state_from_str(s: &str) -> TimerState {
    match s {
        "Running" => TimerState::Running,
        "Paused" => TimerState::Paused,
        _ => TimerState::Idle,
    }
}

pub struct RedTomatoApp {
    pub pomo: PomodoroState,
    /// 当前专注任务（本番茄要完成的事），与番茄工作法关联
    pub current_task: String,
    /// 专注历史：每次完成一个番茄记录一条，用于按时间统计
    pub focus_history: Vec<FocusRecord>,
    /// 是否显示「统计」窗口
    show_statistics: bool,
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
            current_task: String::new(),
            focus_history: Vec::new(),
            show_statistics: false,
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

/// 取消 pin：恢复普通窗口层级并立即恢复完整窗口尺寸，避免下一帧仍用紧凑尺寸绘制完整界面
fn apply_unpin(ctx: &egui::Context) {
    use egui::viewport::{ViewportCommand, WindowLevel};
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(FULL_SIZE.0, FULL_SIZE.1)));
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
        let mut app = Self::default();
        if let Some(storage) = cc.storage {
            if let Some(json) = storage.get_string(STORAGE_KEY_STATE) {
                if let Ok(p) = serde_json::from_str::<PersistedState>(&json) {
                    app.current_task = p.current_task;
                    app.pomo.phase = phase_from_str(&p.phase);
                    let loaded_state = state_from_str(&p.state);
                    app.pomo.state = if loaded_state == TimerState::Running {
                        TimerState::Paused
                    } else {
                        loaded_state
                    };
                    app.pomo.remaining_secs = p.remaining_secs;
                    app.pomo.phase_total_secs = p.phase_total_secs;
                    app.pomo.completed_pomodoros = p.completed_pomodoros;
                }
            }
        }
        app.load_focus_history_from_db();
        app
    }

    /// 从 SQLite 加载专注历史（启动时与统计窗口刷新时用）
    fn load_focus_history_from_db(&mut self) {
        if let Ok(conn) = crate::db::open_and_init() {
            if let Ok(rows) = crate::db::load_focus_records(&conn, 0) {
                self.focus_history = rows
                    .into_iter()
                    .map(|r| FocusRecord {
                        task: r.task,
                        duration_secs: r.duration_secs,
                        completed_at: r.completed_at,
                        completed_pomodoros: r.completed_pomodoros,
                    })
                    .collect();
            }
        }
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
        if self.pomo.take_finished_phase() == Some(Phase::Focus) {
            play_phase_finished_sound();
            if let Some(duration_secs) = self.pomo.take_last_completed_focus_duration() {
                let completed_at = beijing_now_rfc3339();
                let completed_pomodoros = self.pomo.completed_pomodoros;
                let task = self.current_task.clone();
                if let Ok(conn) = crate::db::open_and_init() {
                    let _ = crate::db::insert_focus_record(
                        &conn,
                        &task,
                        duration_secs,
                        &completed_at,
                        completed_pomodoros,
                    );
                }
                self.focus_history.insert(
                    0,
                    FocusRecord {
                        task,
                        duration_secs,
                        completed_at,
                        completed_pomodoros,
                    },
                );
            }
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
        // 统计窗口：按时间列出做了哪些任务、专注时长
        if self.show_statistics {
            self.ui_statistics(ctx);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let p = PersistedState {
            current_task: self.current_task.clone(),
            phase: phase_to_str(self.pomo.phase).to_string(),
            state: state_to_str(self.pomo.state).to_string(),
            remaining_secs: self.pomo.remaining_secs,
            phase_total_secs: self.pomo.phase_total_secs,
            completed_pomodoros: self.pomo.completed_pomodoros,
        };
        if let Ok(json) = serde_json::to_string(&p) {
            storage.set_string(STORAGE_KEY_STATE, json);
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
                    ui.add_space(8.0);
                    let db_path = crate::db::db_path();
                    ui.label(
                        egui::RichText::new("数据 (SQLite)：")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2)),
                    );
                    ui.label(
                        egui::RichText::new(db_path.to_string_lossy().as_ref())
                            .size(11.0)
                            .color(egui::Color32::from_rgb(TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2)),
                    );
                    ui.add_space(16.0);
                    if ui.button("确定").clicked() {
                        self.show_about = false;
                    }
                });
            });
    }

    /// 统计窗口：按完成时间逆序、同任务番茄数累计、番茄数从 1 开始
    fn ui_statistics(&mut self, ctx: &egui::Context) {
        use white_text_theme::TEXT_DIM;
        egui::Window::new("统计 · 专注记录")
            .default_width(460.0)
            .default_height(320.0)
            .show(ctx, |ui| {
                ui.label("数据保存在 SQLite，路径见「关于」；复制该目录即可迁移。");
                ui.add_space(4.0);
                if self.focus_history.is_empty() {
                    ui.label("暂无记录。完成专注后这里会按时间显示任务、时长与番茄数。");
                } else {
                    ui.label("完成时间 · 专注时长 · 番茄数(同任务累计) · 任务");
                    ui.add_space(6.0);
                    let rows = Self::focus_rows_sorted_with_cumulative_tomatoes(&self.focus_history);
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                        for (r, tomato_display) in rows {
                            let mins = r.duration_secs / 60;
                            let secs = r.duration_secs % 60;
                            let duration = format!("{:02}:{:02}", mins, secs);
                            let completed = r.completed_at.chars().take(19).collect::<String>();
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(completed.as_str())
                                        .color(egui::Color32::from_rgb(TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2))
                                        .size(12.0),
                                );
                                ui.label(" · ");
                                ui.label(duration);
                                ui.label(" · ");
                                ui.label(format!("🍅{}", tomato_display));
                                ui.label(" · ");
                                ui.label(if r.task.is_empty() { "(无任务)" } else { r.task.as_str() });
                            });
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("刷新").clicked() {
                        self.load_focus_history_from_db();
                    }
                    if ui.button("关闭").clicked() {
                        self.show_statistics = false;
                    }
                });
            });
    }

    /// 按完成时间逆序排列，并计算同任务番茄数累计（番茄数从 1 开始，0 按 1 计）
    fn focus_rows_sorted_with_cumulative_tomatoes(
        history: &[FocusRecord],
    ) -> Vec<(&FocusRecord, u32)> {
        let mut list: Vec<_> = history.iter().map(|r| (r, r.completed_at.as_str())).collect();
        list.sort_by(|a, b| a.1.cmp(b.1)); // 时间正序（最旧在前）
        let mut task_cumulative: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let mut with_sum: Vec<(&FocusRecord, u32)> = Vec::with_capacity(list.len());
        for (r, _) in list {
            let add = if r.completed_pomodoros == 0 { 1 } else { r.completed_pomodoros };
            let sum = task_cumulative.entry(r.task.clone()).or_insert(0);
            *sum += add;
            with_sum.push((r, *sum));
        }
        with_sum.sort_by(|a, b| b.0.completed_at.cmp(&a.0.completed_at)); // 时间逆序（最新在前）
        with_sum
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
                    ui.add_space(12.0);

                    // 当前任务：与番茄钟关联，专注时明确「在做哪件事」
                    ui.horizontal(|ui| {
                        ui.label("当前任务：");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.current_task)
                                .desired_width(240.0)
                                .hint_text("输入本番茄要完成的事…"),
                        );
                    });
                    ui.add_space(8.0);

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

                    // 开始/暂停、重置、完成 同一行（文字居中）
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
                        if centered_button(ui, "重置", btn_size).on_hover_text("清空当前任务并重置番茄数").clicked() {
                            self.current_task.clear();
                            self.pomo.reset_pomodoros_and_stop();
                        }
                        if centered_button(ui, "完成", btn_size).on_hover_text("完成当前任务并重置，开始下一项").clicked() {
                            self.current_task.clear();
                            self.pomo.reset_pomodoros_and_stop();
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
                    ui.horizontal(|ui| {
                        if ui.link("关于").clicked() {
                            self.show_about = true;
                        }
                        ui.label(" ");
                        if ui.link("统计").clicked() {
                            self.show_statistics = true;
                        }
                    });
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
                            self.full_restore_applied = true; // apply_unpin 内已发 InnerSize，避免下一帧重复
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

                    // 钉住模式下显示当前任务（若有），便于专注时看到「在做哪件事」
                    if !self.current_task.is_empty() {
                        let truncate_len = 18;
                        let display = if self.current_task.chars().count() > truncate_len {
                            format!("{}…", self.current_task.chars().take(truncate_len).collect::<String>())
                        } else {
                            self.current_task.clone()
                        };
                        ui.label(
                            egui::RichText::new(display)
                                .color(egui::Color32::from_rgb(TEXT_WHITE.0, TEXT_WHITE.1, TEXT_WHITE.2))
                                .size(12.0),
                        );
                        ui.add_space(2.0);
                    }

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

                    // 开始/暂停（一个按钮切换），按可用宽度分配
                    let compact_btn = egui::vec2(72.0, 28.0);
                    ui.horizontal(|ui| {
                        let (label, action) = match self.pomo.state {
                            TimerState::Idle => ("开始", 0u8),
                            TimerState::Running => ("暂停", 1u8),
                            TimerState::Paused => ("继续", 2u8),
                        };
                        if centered_button(ui, label, compact_btn).clicked() {
                            if action == 0 {
                                self.pomo.start();
                            } else {
                                self.pomo.toggle_pause();
                            }
                        }
                    });
                });
            });
    }
}
