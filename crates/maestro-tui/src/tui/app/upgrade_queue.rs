//! #1267: `u` queues the upgrade while the fleet is busy; the queued
//! upgrade starts on the first tick the fleet is idle. "Busy" is the quit
//! path's live-work check (#1260), so both agree on what a restart would
//! cut off.

use std::time::Instant;

use crate::tui::activity_log::LogLevel;
use crate::tui::app::App;
use crate::tui::ds::toast::{Severity, Toast};
use crate::tui::input_handler::has_live_work;
use crate::updater::{ReleaseInfo, UpgradeState};

impl App {
    /// `u` on the update chip: download now when the fleet is idle, else
    /// queue it. `u` on a queued upgrade unqueues it.
    pub(crate) fn press_upgrade(&mut self) {
        match std::mem::replace(&mut self.upgrade_state, UpgradeState::Hidden) {
            UpgradeState::Available(info) if has_live_work(self) => {
                self.log_update("Upgrade queued: it starts when the fleet is idle. u cancels.");
                self.upgrade_state = UpgradeState::Queued(info);
            }
            UpgradeState::Available(info) => self.start_upgrade(info),
            UpgradeState::Queued(info) => {
                self.log_update("Queued upgrade cancelled.");
                self.upgrade_state = UpgradeState::Available(info);
            }
            other => self.upgrade_state = other,
        }
    }

    /// Called once per tick from `check_completions`.
    pub(crate) fn start_queued_upgrade_if_idle(&mut self) {
        let UpgradeState::Queued(info) = &self.upgrade_state else {
            return;
        };
        if has_live_work(self) {
            return;
        }
        let info = info.clone();
        let version = crate::tui::screens::sanitize_for_terminal(&info.version);
        self.push_toast(
            Toast::new(
                Severity::Info,
                format!("Fleet idle: upgrading to {version}"),
            ),
            Instant::now(),
        );
        self.start_upgrade(info);
    }

    fn start_upgrade(&mut self, info: ReleaseInfo) {
        self.upgrade_state = UpgradeState::Downloading {
            version: info.version.clone(),
        };
        crate::tui::background_tasks::spawn_upgrade_download(self.data_tx.clone(), info);
    }

    fn log_update(&mut self, message: &str) {
        self.activity_log
            .push_simple("UPDATE".into(), message.into(), LogLevel::Info);
    }
}

#[cfg(test)]
#[path = "upgrade_queue_tests.rs"]
mod tests;
