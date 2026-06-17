use log::{Level, LevelFilter, Log, Metadata, Record};
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

const ACTIVE_LOG_FILE: &str = "entropy.log";
const MAX_LOG_BYTES: u64 = 1_000_000;
const ROTATED_LOG_COUNT: usize = 3;

static LOGGER: DiagnosticsLogger = DiagnosticsLogger {
    enabled: AtomicBool::new(false),
    file: Mutex::new(None),
};

struct DiagnosticsLogger {
    enabled: AtomicBool,
    file: Mutex<Option<std::fs::File>>,
}

impl Log for DiagnosticsLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level()
            <= max_level_for_target(self.enabled.load(Ordering::Relaxed), metadata.target())
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z");
        let line = format!(
            "{now} {:<5} {}: {}\n",
            record.level(),
            record.target(),
            record.args()
        );

        let _ = std::io::stderr().write_all(line.as_bytes());

        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        if let Ok(mut file) = self.file.lock() {
            if let Some(file) = file.as_mut() {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            if let Some(file) = file.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

pub(crate) fn init(enabled: bool) {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
    set_enabled(enabled);
    log::info!(
        "Entropy v{} starting on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if enabled {
        log::info!("Diagnostics log: {}", display_path(&active_log_path()));
        log::info!("Config dir: {}", display_path(&entropy_config_dir()));
    }
}

pub(crate) fn set_enabled(enabled: bool) {
    let opened = if enabled {
        match open_log_file() {
            Ok(file) => Some(file),
            Err(err) => {
                let _ = writeln!(std::io::stderr(), "Failed to open diagnostics log: {err}");
                None
            }
        }
    } else {
        None
    };

    if let Ok(mut file) = LOGGER.file.lock() {
        *file = opened;
    }

    LOGGER.enabled.store(enabled, Ordering::Relaxed);
    log::set_max_level(if enabled {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    });

    log::info!(
        "Diagnostics mode {}",
        if enabled { "enabled" } else { "disabled" }
    );
}

pub(crate) fn settings_file_enabled() -> bool {
    #[derive(serde::Deserialize)]
    struct StartupDiagnosticsSettings {
        #[serde(default)]
        diagnostics_enabled: bool,
    }

    std::fs::read_to_string(entropy_config_dir().join("app_settings.json"))
        .ok()
        .and_then(|data| serde_json::from_str::<StartupDiagnosticsSettings>(&data).ok())
        .map(|settings| settings.diagnostics_enabled)
        .unwrap_or(false)
}

pub(crate) fn active_log_path() -> std::path::PathBuf {
    diagnostics_log_dir().join(ACTIVE_LOG_FILE)
}

pub(crate) fn active_log_path_display() -> String {
    display_path(&active_log_path())
}

fn entropy_config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy")
}

fn diagnostics_log_dir() -> std::path::PathBuf {
    let dir = entropy_config_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn open_log_file() -> std::io::Result<std::fs::File> {
    rotate_logs_if_needed()?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(active_log_path())
}

fn rotate_logs_if_needed() -> std::io::Result<()> {
    let active = active_log_path();
    if active
        .metadata()
        .map(|metadata| metadata.len() <= MAX_LOG_BYTES)
        .unwrap_or(true)
    {
        return Ok(());
    }

    for idx in (1..=ROTATED_LOG_COUNT).rev() {
        let from = rotated_log_path(idx);
        if !from.exists() {
            continue;
        }
        if idx == ROTATED_LOG_COUNT {
            let _ = std::fs::remove_file(&from);
        } else {
            let _ = std::fs::rename(&from, rotated_log_path(idx + 1));
        }
    }

    std::fs::rename(active, rotated_log_path(1))
}

fn rotated_log_path(index: usize) -> std::path::PathBuf {
    diagnostics_log_dir().join(format!("entropy.{index}.log"))
}

fn max_level_for_target(diagnostics_enabled: bool, target: &str) -> Level {
    if !diagnostics_enabled {
        return Level::Info;
    }

    if target == "entropy" || target.starts_with("entropy::") {
        Level::Debug
    } else {
        Level::Warn
    }
}

fn display_path(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        let home = home.to_string_lossy();
        if let Some(rest) = raw.strip_prefix(home.as_ref()) {
            return format!("~{rest}");
        }
    }
    raw.into_owned()
}
