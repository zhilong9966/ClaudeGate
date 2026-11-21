//! 文件日志：按天写入 <日志目录>/claudegate-YYYY-MM-DD.log。
//! 目录、级别可在设置中修改，保存后立即生效；超过保留天数的日志在启动和改配置时清理。
use crate::config::LogCfg;
use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

struct FileLogger {
    inner: Mutex<Inner>,
}

struct Inner {
    dir: PathBuf,
    level: LevelFilter,
    /// 当前打开的日志文件（日期, 句柄），跨天自动切换
    file: Option<(String, File)>,
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

const PREFIX: &str = "claudegate-";

fn level_of(s: &str) -> LevelFilter {
    if s.eq_ignore_ascii_case("debug") { LevelFilter::Debug } else { LevelFilter::Info }
}

/// 启动时调用一次
pub fn init(cfg: &LogCfg) {
    let logger = LOGGER.get_or_init(|| FileLogger {
        inner: Mutex::new(Inner { dir: PathBuf::from(&cfg.dir), level: level_of(&cfg.level), file: None }),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }
    apply(cfg);
}

/// 配置变化后调用：切换目录 / 级别，并清理过期日志
pub fn apply(cfg: &LogCfg) {
    let Some(l) = LOGGER.get() else { return };
    {
        let mut g = l.inner.lock().unwrap();
        let dir = PathBuf::from(&cfg.dir);
        if g.dir != dir {
            g.dir = dir;
            g.file = None;
        }
        g.level = level_of(&cfg.level);
        let _ = std::fs::create_dir_all(&g.dir);
    }
    cleanup(Path::new(&cfg.dir), cfg.retention_days);
}

fn cleanup(dir: &Path, retention_days: u64) {
    if retention_days == 0 {
        return;
    }
    let cutoff = Local::now().date_naive() - chrono::Duration::days(retention_days as i64);
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(date) = name.strip_prefix(PREFIX).and_then(|s| s.strip_suffix(".log")) else { continue };
        if let Ok(d) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            if d < cutoff {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, m: &Metadata) -> bool {
        // 本程序的日志按配置级别输出；第三方库只记录警告和错误
        let lvl = self.inner.lock().map(|g| g.level).unwrap_or(LevelFilter::Info);
        if m.target().starts_with("claudegate") { m.level() <= lvl } else { m.level() <= Level::Warn }
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let now = Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let target = match r.target() {
            "claudegate_lib" => "app",
            t => t.strip_prefix("claudegate_lib::").unwrap_or(t),
        };
        let line = format!("{} [{:<5}] {}: {}\n", now.format("%Y-%m-%d %H:%M:%S%.3f"), r.level(), target, r.args());
        #[cfg(debug_assertions)]
        eprint!("{line}");

        let Ok(mut g) = self.inner.lock() else { return };
        if g.file.as_ref().map(|(d, _)| d != &today).unwrap_or(true) {
            let path = g.dir.join(format!("{PREFIX}{today}.log"));
            let _ = std::fs::create_dir_all(&g.dir);
            g.file = OpenOptions::new().create(true).append(true).open(path).ok().map(|mut f| {
                // 新文件写入 UTF-8 BOM，Windows 记事本 / PowerShell 打开中文不乱码
                if f.metadata().map(|m| m.len() == 0).unwrap_or(false) {
                    let _ = f.write_all(b"\xEF\xBB\xBF");
                }
                (today, f)
            });
        }
        if let Some((_, f)) = g.file.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some((_, f)) = g.file.as_mut() {
                let _ = f.flush();
            }
        }
    }
}
