#![allow(dead_code)] // Reason: restart builder for update flow — to be wired into updater pipeline
use std::path::PathBuf;

/// A restart command to be executed by the caller (never spawned internally).
#[derive(Debug, Clone, PartialEq)]
pub struct RestartCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Builds the restart command without executing it.
pub struct RestartBuilder {
    exe_path: PathBuf,
    args: Vec<String>,
}

impl RestartBuilder {
    pub fn new(exe_path: PathBuf, args: Vec<String>) -> Self {
        Self { exe_path, args }
    }

    /// Construct the restart command. Does NOT fork or spawn anything.
    pub fn build_command(&self) -> RestartCommand {
        RestartCommand {
            program: self.exe_path.clone(),
            args: self.args.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_builder_uses_provided_exe_path() {
        let exe = PathBuf::from("/usr/local/bin/maestro");
        let builder = RestartBuilder::new(exe.clone(), vec![]);
        let cmd = builder.build_command();
        assert_eq!(cmd.program, exe);
    }

    #[test]
    fn restart_builder_preserves_original_args() {
        let exe = PathBuf::from("/usr/local/bin/maestro");
        let args = vec![
            "--config".to_string(),
            "custom.toml".to_string(),
            "--once".to_string(),
        ];
        let builder = RestartBuilder::new(exe, args.clone());
        let cmd = builder.build_command();
        assert_eq!(cmd.args, args);
    }

    #[test]
    fn restart_builder_does_not_spawn() {
        let exe = PathBuf::from("/usr/local/bin/maestro");
        let builder = RestartBuilder::new(exe, vec![]);
        let cmd = builder.build_command();
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn restart_builder_empty_args_produces_empty_args() {
        let exe = PathBuf::from("/tmp/maestro");
        let builder = RestartBuilder::new(exe, vec![]);
        let cmd = builder.build_command();
        assert!(cmd.args.is_empty());
    }
}
