/// A page of the Preferences window, listed down its left side
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreferencesPage {
    #[default]
    General,
    Projects,
    Graph,
    View3d,
    Library,
    Shortcuts,
}

impl PreferencesPage {
    pub const ALL: [PreferencesPage; 6] = [
        PreferencesPage::General,
        PreferencesPage::Projects,
        PreferencesPage::Graph,
        PreferencesPage::View3d,
        PreferencesPage::Library,
        PreferencesPage::Shortcuts,
    ];

    /// The English name in the page list; the UI translates it
    pub fn label(self) -> &'static str {
        match self {
            PreferencesPage::General => "General",
            PreferencesPage::Projects => "Projects",
            PreferencesPage::Graph => "Graph",
            PreferencesPage::View3d => "3D View",
            PreferencesPage::Library => "Library",
            PreferencesPage::Shortcuts => "Shortcuts",
        }
    }
}
