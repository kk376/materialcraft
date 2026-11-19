use super::PreferencesPage;
use crate::state::Prefs;

/// What the open Preferences window keeps between frames
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreferencesState {
    pub page: PreferencesPage,
    /// The preferences as they were when the window opened or Apply was last pressed; Cancel goes back to them
    pub applied: Option<Prefs>,
    /// The Shortcuts page's search texts and filter
    pub shortcut_search: String,
    pub shortcut_key_search: String,
    pub only_assigned: bool,
    /// The library entry whose shortcut field waits for keys
    pub capturing: Option<String>,
}
