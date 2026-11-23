//! What the app remembers between runs by itself, as opposed to the settings the user
//! writes in `config.toml`: `state.toml`, in the same folder. VS Code keeps the two
//! apart the same way (its UI state never goes into `settings.json`).
//!
//! ```toml
//! [panels]          # the dragged panel widths, in logical pixels
//! sidebar = 312.0
//! changes = 280.0
//!
//! [sidebar]
//! archived = ['C:\repos\old-project']   # archived repos, by their folder's full path
//! archived_open = false                 # whether the Archived section is open
//! ```
//!
//! The app owns this file and rewrites it whenever a divider is dropped or a repo is
//! archived. Nobody needs to edit it, so a missing or broken one just means the defaults
//! (the next save replaces it), and unknown keys are ignored rather than rejected.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::PanelWidths;
use crate::config::{self, Config, ConfigError};

const FILE_NAME: &str = "state.toml";

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub panels: PanelWidths,
    pub sidebar: SidebarState,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SidebarState {
    /// Archived repos, by their folder's full path rather than their name, so another
    /// root with a repo of the same name isn't affected. Paths that aren't under the
    /// current root are kept, for when that root is chosen again.
    pub archived: BTreeSet<PathBuf>,
    /// Whether the Archived section is open; closed until opened.
    pub archived_open: bool,
}

impl State {
    fn path() -> Result<PathBuf, ConfigError> {
        Ok(Config::dir()?.join(FILE_NAME))
    }

    /// Reads the saved state. Anything missing or unreadable gives the defaults.
    pub fn load() -> State {
        State::path()
            .ok()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| State::parse(&text).ok())
            .unwrap_or_default()
    }

    fn parse(text: &str) -> Result<State, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        config::write_toml(&State::path()?, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full() {
        let state = State::parse("[panels]\nsidebar = 312.5\nchanges = 300.0\n").unwrap();
        assert_eq!(state.panels.sidebar, 312.5);
        assert_eq!(state.panels.changes, 300.0);
    }

    #[test]
    fn test_parse_fills_in_missing_keys() {
        assert_eq!(State::parse("").unwrap(), State::default());

        let state = State::parse("[panels]\nsidebar = 312.0\n").unwrap();
        assert_eq!(state.panels.sidebar, 312.0);
        assert_eq!(state.panels.changes, PanelWidths::default().changes);
    }

    #[test]
    fn test_parse_ignores_unknown_keys() {
        assert_eq!(
            State::parse("future = 1\n[panels]\nother = 2\n").unwrap(),
            State::default()
        );
    }

    #[test]
    fn test_parse_sidebar() {
        let state =
            State::parse("[sidebar]\narchived = ['C:\\repos\\old']\narchived_open = true\n")
                .unwrap();
        assert!(
            state
                .sidebar
                .archived
                .contains(&PathBuf::from(r"C:\repos\old"))
        );
        assert!(state.sidebar.archived_open);
        // A file from before archiving existed: nothing archived, section closed.
        assert_eq!(
            State::parse("[panels]\nsidebar = 312.0\n").unwrap().sidebar,
            SidebarState::default()
        );
    }

    #[test]
    fn test_round_trip() {
        let mut state = State::default();
        state.panels.sidebar = 333.25;
        state
            .sidebar
            .archived
            .insert(PathBuf::from(r"C:\repos\old"));
        state.sidebar.archived_open = true;

        let text = toml::to_string_pretty(&state).unwrap();
        assert_eq!(State::parse(&text).unwrap(), state);
    }
}
