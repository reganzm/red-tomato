# Red Tomato 代码结构说明

便于快速理解项目各模块职责与数据流。

---

## 一、项目总览

```
red-tomato/
├── Cargo.toml          # 依赖与构建配置
├── build.rs            # 构建脚本：生成 icon.ico 并嵌入 Windows exe
└── src/
    ├── main.rs         # 入口：窗口配置、图标、启动 eframe
    ├── app.rs          # 主界面与状态（UI、持久化、钉住/紧凑模式）
    ├── pomodoro.rs     # 番茄钟逻辑（阶段、计时、开始/暂停/结束）
    └── db.rs           # SQLite：专注记录表与读写
```

- **eframe/egui**：负责窗口和所有 UI 绘制。
- **chrono**：时间与北京时区。
- **rusqlite**：专注历史持久化；**eframe persistence**：当前任务 + 番茄钟会话状态。

---

## 二、入口与窗口：`main.rs`

- **`make_app_icon()`**  
  生成 48×48 番茄红圆形 RGBA，供 eframe 用作窗口/任务栏图标。
- **`main()`**  
  - 用 `eframe::NativeOptions` 配置：无系统标题栏、初始尺寸、标题「番」、图标。  
  - `eframe::run_native(..., RedTomatoApp::new(cc))` 创建并运行主应用。

**`windows_subsystem = "windows"`**：Release 下不弹控制台黑窗。

---

## 三、番茄钟核心逻辑：`pomodoro.rs`

纯状态与计时，不涉及 UI。

### 3.1 类型

| 类型 | 含义 |
|------|------|
| **Phase** | 当前阶段：`Focus` / `ShortBreak` / `LongBreak` |
| **TimerState** | 计时状态：`Idle` / `Running` / `Paused` |
| **PomodoroConfig** | 配置：专注/短休息/长休息时长（秒）、几个番茄后长休息 |
| **PomodoroState** | 当前阶段、状态、剩余秒数、本阶段总秒数、已完成番茄数、上次 tick 时间等 |

### 3.2 主要方法

- **`start()`**：按当前阶段设总时长与剩余时间，进入 `Running`。
- **`toggle_pause()`**：在 `Running` ↔ `Paused` 间切换。
- **`stop()`**：回到 `Idle`，剩余/总时长清 0。
- **`set_phase(phase)`**：切换阶段并 `stop()`。
- **`tick(now)`**：每帧调用，若为 `Running` 则根据时间差扣减剩余秒数；若归零则调用 `on_phase_finished()`。
- **`on_phase_finished()`**（内部）：  
  设置 `finished_phase`、`last_completed_focus_duration_secs`（仅专注结束时有值），  
  更新阶段（专注→短/长休息，休息→专注），番茄数在专注结束时 +1，满 N 个后进入长休息并清零。
- **`reset_pomodoros_and_stop()`**：番茄数置 0、阶段置 Focus、并 `stop()`（供「重置/完成」使用）。
- **`take_finished_phase()` / `take_last_completed_focus_duration()`**：供 UI 取走「本帧刚结束的阶段」和「刚完成专注的时长」，用于提示音与写入 SQLite。

数据流：**UI 每帧调用 `tick(Utc::now())` → 内部更新剩余时间与阶段 → UI 读 `remaining_display()`、`progress()`、`take_*` 做显示与副作用**。

---

## 四、数据库层：`db.rs`

专注记录持久化与迁移。

- **路径**：`data_dir()/red_tomato.db`，`data_dir()` 来自 `dirs::data_local_dir()/red-tomato`（可复制整个目录迁移）。
- **表**：`focus_records (id, task, duration_secs, completed_at, completed_pomodoros)`。
- **API**：  
  - `open_and_init()`：打开/创建 DB 并执行建表。  
  - `insert_focus_record(...)`：插入一条完成记录。  
  - `load_focus_records(conn, limit)`：按 `completed_at DESC` 取记录，`limit=0` 表示全部。

不保存「当前任务 / 当前阶段 / 是否运行」等会话状态，这些由 eframe storage 负责。

---

## 五、主应用与 UI：`app.rs`

实现 `eframe::App`，持有界面状态和番茄钟状态，并桥接持久化。

### 5.1 核心数据结构

- **RedTomatoApp**  
  - `pomo: PomodoroState`：番茄钟状态。  
  - `current_task: String`：当前任务文案。  
  - `focus_history: Vec<FocusRecord>`：从 SQLite 加载的专注历史（统计用）。  
  - 钉住/紧凑相关：`compact`, `pinned`, `pin_applied`, `compact_size_applied`, `full_restore_applied`, `full_no_decorations_applied` 等。  
  - 弹窗：`show_about`, `show_statistics`。  
  - Windows 特例：`system_menu_removed`（去掉标题栏系统菜单）。
- **FocusRecord**  
  与 DB 一行对应：`task`, `duration_secs`, `completed_at`, `completed_pomodoros`。  
- **PersistedState**  
  仅会话状态（当前任务、阶段、状态、剩余/总秒数、番茄数），序列化为 JSON 存 eframe storage，**不**包含 `focus_history`（历史在 SQLite）。

### 5.2 生命周期与持久化

- **`RedTomatoApp::new(cc)`**  
  - 设置中文字体。  
  - 从 `cc.storage` 读 JSON 恢复 `PersistedState`（任务、阶段、状态、剩余时间、番茄数）；若为 Running 则改为 Paused。  
  - 调用 `load_focus_history_from_db()` 从 SQLite 拉取专注历史。
- **`update(ctx, frame)`**（每帧）  
  - `pomo.tick(Utc::now())`。  
  - 若 `take_finished_phase() == Focus`：播提示音，取 `take_last_completed_focus_duration()`，写 SQLite 并 push 到 `focus_history`（北京时区 `completed_at`）。  
  - 根据 `pinned`/`compact` 应用钉住、无标题栏、窗口尺寸等。  
  - Windows 下可选去掉系统菜单。  
  - 根据 `compact` 调用 `ui_compact` 或 `ui_full`；若需要则显示关于/统计窗口。
- **`save(storage)`**  
  将当前会话状态（不含 `focus_history`）序列化为 JSON 写入 eframe storage。

### 5.3 UI 拆分

- **`ui_full(ctx)`**  
  非钉住模式：顶栏（钉住 + 关闭）、当前任务输入、阶段文案、大计时器、进度条、开始/暂停、重置、完成、阶段选择、番茄数圆圈、关于/统计链接。
- **`ui_compact(ctx)`**  
  钉住模式：小窗、钉住/关闭、可选当前任务摘要、计时器、阶段、进度条、开始/暂停。
- **`ui_about(ctx)`**  
  关于窗口：应用名、数据路径（SQLite 所在目录）。
- **`ui_statistics(ctx)`**  
  统计窗口：从 `focus_history` 按时间逆序、同任务番茄数累计、番茄数从 1 开始显示；刷新时重新从 SQLite 加载。

### 5.4 辅助函数（节选）

- **主题与布局**：`white_text_theme` 常量、`PIN_MARGIN`、`COMPACT_*`、`FULL_SIZE`。  
- **字体**：`setup_chinese_fonts`。  
- **时间**：`beijing_now_rfc3339()`（北京时区 RFC3339）。  
- **阶段/状态**：`phase_to_str` / `phase_from_str`、`state_to_str` / `state_from_str`（与 JSON 互转）。  
- **钉住**：`pin_position_top_right`、`apply_pin`、`apply_unpin`。  
- **绘制**：`paint_subtle_pattern`、`paint_pomodoro_circles`、`centered_button`。  
- **音效**：`play_phase_finished_sound()`（Windows Beep）。  
- **统计**：`focus_rows_sorted_with_cumulative_tomatoes`（按时间逆序 + 同任务累计番茄数）。  
- **Windows**：`try_remove_system_menu`（去掉标题栏系统菜单）。

理解顺序建议：先 **pomodoro**（状态如何变化），再 **db**（历史如何落盘），最后 **app**（如何每帧 tick、何时写 SQLite、何时读/写 storage、两种 UI 与弹窗）。

---

## 六、构建脚本：`build.rs`

仅在 **Windows** 下执行：

1. 在项目根目录生成 **icon.ico**（16/32/48 三种尺寸的番茄红圆）。  
2. 用 **winres** 将 icon.ico 嵌入 exe，供任务栏/资源管理器显示。

其他平台不执行，不影响编译。

---

## 七、数据流简图

```
用户操作（开始/暂停/重置/完成/阶段切换）
    → RedTomatoApp 调用 pomo.start() / stop() / toggle_pause() / set_phase() / reset_pomodoros_and_stop()
    → 每帧 update() 里 pomo.tick(now)
    → 若专注结束：写 SQLite + push focus_history，播提示音
    → 关闭/定时：save(storage) 写当前任务 + 番茄钟会话状态（JSON）
下次启动：new(cc) 从 storage 恢复会话，从 SQLite 加载 focus_history
```

按「入口 → 番茄钟逻辑 → 数据库 → 主应用与 UI」这条线读代码，会最容易理解整体结构。
