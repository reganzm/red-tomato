//! 番茄工作法状态与计时逻辑

use chrono::{DateTime, Utc};

/// 番茄钟阶段
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// 专注工作（默认 25 分钟）
    Focus,
    /// 短休息（默认 5 分钟）
    ShortBreak,
    /// 长休息（4 个番茄后，默认 15 分钟）
    LongBreak,
}

/// 计时器状态
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    Running,
    Paused,
    Idle,
}

/// 番茄工作法配置（单位：秒）
#[derive(Clone, Debug)]
pub struct PomodoroConfig {
    pub focus_secs: i64,
    pub short_break_secs: i64,
    pub long_break_secs: i64,
    pub pomodoros_before_long: u32,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            // 调试用：专注 1 分钟，短休息 30 秒，长休息 50 秒；正式可改为 25*60, 5*60, 15*60
            focus_secs: 60*25,
            short_break_secs: 60*5,
            long_break_secs: 15*60,
            pomodoros_before_long: 4,

            // focus_secs: 20,
            // short_break_secs: 5,
            // long_break_secs: 10,
            // pomodoros_before_long: 4,
        }
    }
}

/// 番茄钟核心状态
pub struct PomodoroState {
    pub config: PomodoroConfig,
    pub phase: Phase,
    pub state: TimerState,
    pub remaining_secs: i64,
    pub phase_total_secs: i64,
    pub completed_pomodoros: u32,
    pub last_tick_at: Option<DateTime<Utc>>,
    /// 本帧刚结束的阶段（用于触发提示音等），取走后清空
    pub finished_phase: Option<Phase>,
    /// 刚完成的一次专注的时长（秒），供记录历史用，取走后清空
    pub last_completed_focus_duration_secs: Option<i64>,
}

impl Default for PomodoroState {
    fn default() -> Self {
        Self {
            config: PomodoroConfig::default(),
            phase: Phase::Focus,
            state: TimerState::Idle,
            remaining_secs: 0,
            phase_total_secs: 0,
            completed_pomodoros: 0,
            last_tick_at: None,
            finished_phase: None,
            last_completed_focus_duration_secs: None,
        }
    }
}

impl PomodoroState {
    pub fn new(config: PomodoroConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// 开始当前阶段
    pub fn start(&mut self) {
        let total = match self.phase {
            Phase::Focus => self.config.focus_secs,
            Phase::ShortBreak => self.config.short_break_secs,
            Phase::LongBreak => self.config.long_break_secs,
        };
        self.phase_total_secs = total;
        self.remaining_secs = total;
        self.state = TimerState::Running;
        self.last_tick_at = Some(Utc::now());
    }

    /// 暂停 / 继续
    pub fn toggle_pause(&mut self) {
        match self.state {
            TimerState::Running => {
                self.state = TimerState::Paused;
                self.last_tick_at = None;
            }
            TimerState::Paused => {
                self.state = TimerState::Running;
                self.last_tick_at = Some(Utc::now());
            }
            TimerState::Idle => {}
        }
    }

    /// 停止当前阶段，回到 Idle
    pub fn stop(&mut self) {
        self.state = TimerState::Idle;
        self.remaining_secs = 0;
        self.phase_total_secs = 0;
        self.last_tick_at = None;
    }

    /// 重置番茄数、阶段回到专注，并停止（用于「重置」/「完成」按钮）
    pub fn reset_pomodoros_and_stop(&mut self) {
        self.completed_pomodoros = 0;
        self.phase = Phase::Focus;
        self.stop();
    }

    /// 选择阶段并进入 Idle（用户可再点开始）
    pub fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
        self.stop();
    }

    /// 每秒由 UI 调用，推进计时并处理阶段结束
    pub fn tick(&mut self, now: DateTime<Utc>) {
        if self.state != TimerState::Running {
            return;
        }
        let Some(last) = self.last_tick_at else { return };
        let elapsed = (now - last).num_seconds();
        if elapsed <= 0 {
            return;
        }
        self.last_tick_at = Some(now);
        self.remaining_secs = (self.remaining_secs - elapsed).max(0);

        if self.remaining_secs <= 0 {
            self.on_phase_finished();
        }
    }

    fn on_phase_finished(&mut self) {
        let just_finished = self.phase;
        let total_secs = self.phase_total_secs;
        self.finished_phase = Some(just_finished);
        self.state = TimerState::Idle;
        self.remaining_secs = 0;
        self.phase_total_secs = 0;
        self.last_tick_at = None;
        if just_finished == Phase::Focus {
            self.last_completed_focus_duration_secs = Some(total_secs);
        }

        match self.phase {
            Phase::Focus => {
                self.completed_pomodoros += 1;
                if self.completed_pomodoros >= self.config.pomodoros_before_long {
                    self.phase = Phase::LongBreak;
                    self.completed_pomodoros = 0;
                } else {
                    self.phase = Phase::ShortBreak;
                }
            }
            Phase::ShortBreak | Phase::LongBreak => {
                self.phase = Phase::Focus;
            }
        }
    }

    /// 剩余时间格式化为 "MM:SS"
    pub fn remaining_display(&self) -> String {
        let s = self.remaining_secs.max(0);
        let m = s / 60;
        let s = s % 60;
        format!("{:02}:{:02}", m, s)
    }

    /// 取走“刚结束的阶段”（用于播提示音等），取走后清空
    pub fn take_finished_phase(&mut self) -> Option<Phase> {
        self.finished_phase.take()
    }

    /// 取走刚完成的一次专注的时长（秒），用于记录历史，取走后清空
    pub fn take_last_completed_focus_duration(&mut self) -> Option<i64> {
        self.last_completed_focus_duration_secs.take()
    }

    /// 当前阶段进度 0.0..=1.0
    pub fn progress(&self) -> f32 {
        if self.phase_total_secs <= 0 {
            return 0.0;
        }
        let remaining = self.remaining_secs.max(0);
        let elapsed = self.phase_total_secs - remaining;
        (elapsed as f32 / self.phase_total_secs as f32).min(1.0)
    }
}
