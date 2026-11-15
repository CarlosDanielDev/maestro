//! Subcommands that open the TUI (`maestro`, `run`, `resume`) and the `App`
//! bootstrap they share. The headless commands live in `maestro-cli` (#1307).

mod dashboard;
mod resume;
pub(crate) mod run;
pub(crate) use maestro_tui::setup;

pub use dashboard::cmd_dashboard;
pub use resume::cmd_resume;
pub use run::cmd_run;
