//! Reads and writes the `behavior.caveman_mode` flag in `.claude/settings.json`.
//!
//! The settings file is shared with Claude Code. Unknown top-level keys
//! (`mcpServers`, `env`, `hooks`, ...) are preserved verbatim through
//! a `serde_json::Value` round-trip — the toggle only mutates
//! `behavior.caveman_mode`.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CavemanModeState {
    ExplicitTrue,
    ExplicitFalse,
    Default,
    Error(String),
}

impl CavemanModeState {
    pub fn label(&self) -> Cow<'static, str> {
        match self {
            Self::ExplicitTrue => Cow::Borrowed("true"),
            Self::ExplicitFalse => Cow::Borrowed("false"),
            Self::Default => Cow::Borrowed("false (default)"),
            Self::Error(msg) => Cow::Owned(format!("<error: {}>", msg)),
        }
    }

    pub fn is_toggleable(&self) -> bool {
        !matches!(self, Self::Error(_))
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::ExplicitTrue => Some(true),
            Self::ExplicitFalse | Self::Default => Some(false),
            Self::Error(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CavemanWriteError {
    Io(String),
    Serialise(String),
    SymlinkNotSupported(PathBuf),
    ParentMissing(PathBuf),
}

impl std::fmt::Display for CavemanWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "io: {}", m),
            Self::Serialise(m) => write!(f, "malformed settings.json: {}", m),
            Self::SymlinkNotSupported(p) => {
                write!(f, "symlink target unreachable: {}", p.display())
            }
            Self::ParentMissing(p) => write!(f, "no parent directory for {}", p.display()),
        }
    }
}

impl std::error::Error for CavemanWriteError {}

pub trait SettingsStore: Send {
    fn load_caveman_mode(&self) -> CavemanModeState;
    fn save_caveman_mode(&self, new_value: bool) -> Result<(), CavemanWriteError>;
}

pub struct FsSettingsStore {
    path: PathBuf,
}

impl FsSettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl SettingsStore for FsSettingsStore {
    fn load_caveman_mode(&self) -> CavemanModeState {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => parse_caveman_mode_from_str(&raw),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => CavemanModeState::Default,
            Err(e) => CavemanModeState::Error(format!("read failed: {}", e)),
        }
    }

    fn save_caveman_mode(&self, new_value: bool) -> Result<(), CavemanWriteError> {
        let target = resolve_target(&self.path)?;

        let existing_value = match std::fs::read_to_string(&target) {
            Ok(raw) => Some(
                serde_json::from_str::<Value>(&raw)
                    .map_err(|e| CavemanWriteError::Serialise(e.to_string()))?,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(CavemanWriteError::Io(e.to_string())),
        };

        let mutated = apply_caveman_toggle(existing_value.as_ref(), new_value)?;
        let bytes = serde_json::to_vec_pretty(&Value::Object(mutated))
            .map_err(|e| CavemanWriteError::Serialise(e.to_string()))?;

        atomic_write(&target, &bytes)
    }
}

/// If `path` is a symlink, follow it to the target. The symlink itself stays
/// intact because the eventual rename happens in the target's directory.
/// A dangling symlink yields `SymlinkNotSupported`.
fn resolve_target(path: &Path) -> Result<PathBuf, CavemanWriteError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => std::fs::canonicalize(path)
            .map_err(|_| CavemanWriteError::SymlinkNotSupported(path.to_path_buf())),
        _ => Ok(path.to_path_buf()),
    }
}

fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), CavemanWriteError> {
    use std::io::Write;

    let parent = target
        .parent()
        .ok_or_else(|| CavemanWriteError::ParentMissing(target.to_path_buf()))?;
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent).map_err(|e| CavemanWriteError::Io(e.to_string()))?;
    }

    let filename = target
        .file_name()
        .ok_or_else(|| CavemanWriteError::ParentMissing(target.to_path_buf()))?
        .to_string_lossy()
        .into_owned();
    // O_EXCL + UUID nonce: symlink-hijack and pre-created-file scenarios
    // fail closed instead of clobbering an attacker-placed entry. Pattern
    // matches `src/prd/store.rs:atomic_temp_path`.
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let tmp = parent.join(format!(".{}.tmp.{}", filename, nonce));

    {
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)
            .map_err(|e| CavemanWriteError::Io(e.to_string()))?;
        f.write_all(bytes)
            .map_err(|e| CavemanWriteError::Io(e.to_string()))?;
        f.sync_all()
            .map_err(|e| CavemanWriteError::Io(e.to_string()))?;
    }

    if let Err(e) = std::fs::rename(&tmp, target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(CavemanWriteError::Io(e.to_string()));
    }

    #[cfg(unix)]
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }

    Ok(())
}

/// Pure parsing core. Visible to tests so unit cases avoid the filesystem.
pub(crate) fn parse_caveman_mode(value: &Value) -> CavemanModeState {
    let Some(root) = value.as_object() else {
        return CavemanModeState::Error("settings root is not an object".to_string());
    };
    let Some(behavior) = root.get("behavior") else {
        return CavemanModeState::Default;
    };
    let Some(behavior_obj) = behavior.as_object() else {
        return CavemanModeState::Error("behavior key is not an object".to_string());
    };
    match behavior_obj.get("caveman_mode") {
        None => CavemanModeState::Default,
        Some(Value::Bool(true)) => CavemanModeState::ExplicitTrue,
        Some(Value::Bool(false)) => CavemanModeState::ExplicitFalse,
        Some(_) => CavemanModeState::Error("caveman_mode is not a boolean".to_string()),
    }
}

pub(crate) fn parse_caveman_mode_from_str(raw: &str) -> CavemanModeState {
    match serde_json::from_str::<Value>(raw) {
        Ok(v) => parse_caveman_mode(&v),
        Err(e) => CavemanModeState::Error(format!("invalid json: {}", e)),
    }
}

/// Pure mutation core. Returns the JSON object that should be written.
/// Errors only when the existing file is structurally incompatible
/// (e.g. `behavior` present but not an object).
pub(crate) fn apply_caveman_toggle(
    existing: Option<&Value>,
    new_value: bool,
) -> Result<Map<String, Value>, CavemanWriteError> {
    let mut root = match existing {
        Some(Value::Object(map)) => map.clone(),
        Some(_) => {
            return Err(CavemanWriteError::Serialise(
                "settings root is not an object".to_string(),
            ));
        }
        None => Map::new(),
    };

    let behavior_entry = root
        .entry("behavior".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let behavior = behavior_entry
        .as_object_mut()
        .ok_or_else(|| CavemanWriteError::Serialise("behavior key is not an object".to_string()))?;
    behavior.insert("caveman_mode".to_string(), Value::Bool(new_value));
    Ok(root)
}
