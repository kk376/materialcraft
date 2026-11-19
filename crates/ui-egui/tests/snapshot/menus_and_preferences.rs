//! Renders of the menu bar's menus, the Preferences pages, the New graph dialog, the home screen
//! and the Plugin Manager

use egui_kittest::kittest::Queryable;
use materialcraft_ui_egui::preferences::PreferencesPage;
use materialcraft_ui_egui::{MaterialCraftApp, UiState};

#[test]
#[ignore = "requires an offscreen GPU; run explicitly with --ignored"]
fn the_menus_and_their_windows() {
    let Some(directory) = std::env::var_os("MC_SNAPSHOT_DIR").map(std::path::PathBuf::from) else { return };

    std::fs::create_dir_all(&directory).unwrap();
    let app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    let mut harness = super::harness(app, 1600.0, 960.0);
    super::settle(&mut harness, 8);

    for menu in ["File", "Edit", "Tools", "Windows", "Help"] {
        harness.get_by_label(menu).click();
        super::settle(&mut harness, 4);
        super::save(&mut harness, &directory, &format!("menu-{}", menu.to_lowercase()));
        harness.key_press(egui::Key::Escape);
        super::settle(&mut harness, 2);
    }

    harness.state_mut().ui.windows.preferences = true;
    for page in PreferencesPage::ALL {
        harness.state_mut().ui.preferences.page = page;
        super::settle(&mut harness, 6);
        super::save(&mut harness, &directory, &format!("preferences-{}", page.label().to_lowercase().replace(' ', "-")));
    }
    harness.state_mut().ui.windows.preferences = false;

    // The template previews render in the background
    harness.state_mut().ui.windows.new_graph = true;
    super::settle(&mut harness, 200);
    super::save(&mut harness, &directory, "new-graph");
    harness.state_mut().ui.windows.new_graph = false;

    harness.state_mut().ui.windows.home = true;
    super::settle(&mut harness, 10);
    super::save(&mut harness, &directory, "home-screen");
    harness.state_mut().ui.windows.home = false;

    harness.state_mut().ui.windows.plugin_manager = true;
    super::settle(&mut harness, 6);
    super::save(&mut harness, &directory, "plugin-manager");
}
