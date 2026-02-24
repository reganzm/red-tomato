//! SQLite 持久化：任务与专注记录，便于迁移与长期保存

use rusqlite::Connection;

/// 数据库文件名（放在应用数据目录下）
pub const DB_FILENAME: &str = "red_tomato.db";

/// 应用数据目录（可迁移：复制此目录下的 .db 即可）
pub fn data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("red-tomato")
}

pub fn db_path() -> std::path::PathBuf {
    data_dir().join(DB_FILENAME)
}

/// 打开数据库并创建表（若不存在）
pub fn open_and_init() -> Result<Connection, rusqlite::Error> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(&path)?;
    init_schema(&conn)?;
    Ok(conn)
}

/// 创建 focus_records 表
fn init_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS focus_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task TEXT NOT NULL,
            duration_secs INTEGER NOT NULL,
            completed_at TEXT NOT NULL,
            completed_pomodoros INTEGER NOT NULL
        );
        "#,
    )?;
    Ok(())
}

/// 单条专注记录（与表结构一致）
pub struct FocusRow {
    pub id: i64,
    pub task: String,
    pub duration_secs: i64,
    pub completed_at: String,
    pub completed_pomodoros: u32,
}

/// 插入一条专注记录
pub fn insert_focus_record(
    conn: &Connection,
    task: &str,
    duration_secs: i64,
    completed_at: &str,
    completed_pomodoros: u32,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO focus_records (task, duration_secs, completed_at, completed_pomodoros) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![task, duration_secs, completed_at, completed_pomodoros as i64],
    )?;
    Ok(())
}

/// 按完成时间倒序加载记录（最新在前），limit 0 表示全部
pub fn load_focus_records(conn: &Connection, limit: u32) -> Result<Vec<FocusRow>, rusqlite::Error> {
    let limit_val = if limit > 0 { limit as i64 } else { 1_000_000 };
    let mut stmt = conn.prepare(
        "SELECT id, task, duration_secs, completed_at, completed_pomodoros FROM focus_records ORDER BY completed_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit_val], |row| {
        Ok(FocusRow {
            id: row.get(0)?,
            task: row.get(1)?,
            duration_secs: row.get(2)?,
            completed_at: row.get(3)?,
            completed_pomodoros: row.get::<_, i64>(4)? as u32,
        })
    })?;
    rows.collect()
}
