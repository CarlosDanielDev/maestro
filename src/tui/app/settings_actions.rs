use crate::tui::app::App;

impl App {
    pub fn with_settings_store(
        mut self,
        store: Box<dyn crate::settings::SettingsStore + Send>,
    ) -> Self {
        self.settings_store = Some(store);
        self
    }

    /// Read the current `behavior.caveman_mode` from `.claude/settings.json`.
    /// Returns `Default` when no store is wired.
    pub fn caveman_mode(&self) -> crate::settings::CavemanModeState {
        match self.settings_store.as_ref() {
            Some(store) => store.load_caveman_mode(),
            None => crate::settings::CavemanModeState::Default,
        }
    }

    /// Drain the settings screen's pending caveman toggle and persist it.
    /// Surfaces a status flash on the screen, and reverts the in-screen
    /// widget on write failure.
    pub fn process_pending_caveman_toggle(&mut self) {
        let Some(screen) = self.settings_screen.as_mut() else {
            return;
        };
        let Some(new_value) = screen.take_pending_caveman_toggle() else {
            return;
        };
        let Some(store) = self.settings_store.as_ref() else {
            screen.show_caveman_status("settings store unavailable; toggle ignored.".to_string());
            return;
        };
        match store.save_caveman_mode(new_value) {
            Ok(()) => {
                let new_state = if new_value {
                    crate::settings::CavemanModeState::ExplicitTrue
                } else {
                    crate::settings::CavemanModeState::ExplicitFalse
                };
                screen.set_caveman_state(new_state);
                screen.show_caveman_status(format!(
                    "caveman_mode → {}  (effective on next Claude session)",
                    new_value
                ));
            }
            Err(err) => {
                let prior = store.load_caveman_mode();
                screen.set_caveman_state(prior);
                screen.show_caveman_status(format!("caveman_mode write error: {}", err));
            }
        }
    }
}
