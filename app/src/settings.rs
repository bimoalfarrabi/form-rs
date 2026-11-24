//! Small persisted settings file.
//!
//! Deliberately hand-rolled: one `key=value` per line, no dependency, and a
//! missing or unreadable file is never an error.

use std::path::PathBuf;

const FILE_NAME: &str = "config.txt";

fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
}

pub fn path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("form-rs").join(FILE_NAME))
}

fn read() -> Vec<(String, String)> {
    let Some(path) = path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once('=')
                .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        })
        .collect()
}

fn write(entries: &[(String, String)]) -> std::io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = String::from("# Form settings\n");
    for (key, value) in entries {
        text.push_str(&format!("{key}={value}\n"));
    }
    std::fs::write(path, text)
}

pub fn get(key: &str) -> Option<String> {
    read().into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

pub fn set(key: &str, value: Option<&str>) {
    let mut entries = read();
    entries.retain(|(k, _)| k != key);
    if let Some(value) = value {
        entries.push((key.to_string(), value.to_string()));
    }
    let _ = write(&entries);
}

/// The ffmpeg the user picked by hand, if any.
pub fn ffmpeg_path() -> Option<PathBuf> {
    get("ffmpeg").map(PathBuf::from).filter(|p| p.is_file())
}

pub fn set_ffmpeg_path(path: Option<&std::path::Path>) {
    set(
        "ffmpeg",
        path.and_then(|p| p.to_str()).filter(|s| !s.is_empty()),
    );
}
