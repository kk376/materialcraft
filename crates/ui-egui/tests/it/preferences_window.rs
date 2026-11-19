//! Edit ▸ Preferences: pages, OK, Cancel and Apply, and the node creation shortcuts they set

use egui::accesskit::Role;
use egui::{Key, Modifiers, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use materialcraft_ui_egui::preferences::PreferencesPage;
use materialcraft_ui_egui::{MaterialCraftApp, UiState};

fn harness() -> Harness<'static, MaterialCraftApp> {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    app.ui.windows.home = false;
    let mut harness = Harness::builder().with_size(vec2(1600.0, 960.0)).with_pixels_per_point(1.0).build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app,
    );
    harness.run_steps(6);
    harness
}

fn open(harness: &mut Harness<'static, MaterialCraftApp>) {
    harness.state_mut().ui.windows.preferences = true;
    harness.run_steps(4);
}

fn click(harness: &mut Harness<'static, MaterialCraftApp>, role: Role, label: &str) {
    harness.get(By::new().role(role).label(label)).click();
    harness.run_steps(4);
}

#[test]
fn cancel_returns_to_the_preferences_of_the_last_apply() {
    let mut harness = harness();
    open(&mut harness);

    click(&mut harness, Role::CheckBox, "Invert zoom in views");
    assert!(harness.state().ui.prefs.invert_zoom, "a change shows at once");
    click(&mut harness, Role::Button, "Apply");

    click(&mut harness, Role::Tab, "Graph");
    assert_eq!(harness.state().ui.preferences.page, PreferencesPage::Graph);
    click(&mut harness, Role::CheckBox, "Tab key shows the node search");
    assert!(!harness.state().ui.prefs.tab_opens_node_search);

    click(&mut harness, Role::Button, "Cancel");
    let prefs = &harness.state().ui.prefs;
    assert!(prefs.invert_zoom, "the applied change stays");
    assert!(prefs.tab_opens_node_search, "the change after Apply is undone");
    assert!(!harness.state().ui.windows.preferences);

    open(&mut harness);
    click(&mut harness, Role::CheckBox, "View outputs in 3D view when opening a graph");
    click(&mut harness, Role::Button, "OK");
    assert!(!harness.state().ui.prefs.view_outputs_on_open && !harness.state().session.view_outputs_on_open, "OK keeps the change and the session follows it");
    assert!(!harness.state().ui.windows.preferences);
}

#[test]
fn draft_limits_preserve_recents_and_undo_until_applied() {
    let mut harness = harness();
    for _ in 0..3 {
        harness.state_mut().run("graph.node.add", serde_json::json!({"def": "uniform"})).unwrap();
    }
    for path in ["first.sbs", "second.sbs", "third.sbs"] {
        harness.state_mut().ui.add_recent(path);
    }
    let history = harness.state().session.history();
    open(&mut harness);
    harness.state_mut().ui.prefs.history.undo_limit = 1;
    harness.state_mut().ui.prefs.recent_limit = 1;
    harness.run_steps(2);
    click(&mut harness, Role::Button, "Cancel");

    assert_eq!(harness.state().session.history(), history);
    assert_eq!(harness.state().ui.recent.len(), 3);

    open(&mut harness);
    harness.state_mut().ui.prefs.history.undo_limit = 1;
    harness.state_mut().ui.prefs.recent_limit = 1;
    click(&mut harness, Role::Button, "Apply");

    assert_eq!(harness.state().session.history_prefs.undo_limit, 1);
    assert_eq!(harness.state().ui.recent, ["third.sbs"]);
}

#[test]
fn draft_preview_size_reaches_the_session_only_on_apply_or_ok() {
    let mut harness = harness();
    let initial = harness.state().session.preview_size;
    open(&mut harness);
    harness.state_mut().ui.prefs.preview_size = 64;
    harness.run_steps(2);

    assert_eq!(harness.state().session.preview_size, initial);
    click(&mut harness, Role::Button, "Cancel");
    assert_eq!(harness.state().session.preview_size, initial);

    open(&mut harness);
    harness.state_mut().ui.prefs.preview_size = 1;
    click(&mut harness, Role::Button, "Apply");
    assert_eq!(harness.state().session.preview_size, Some(64));

    harness.state_mut().ui.prefs.preview_size = 0;
    harness.run_steps(2);
    assert_eq!(harness.state().session.preview_size, Some(64));
    click(&mut harness, Role::Button, "OK");
    assert_eq!(harness.state().session.preview_size, None);
}

#[test]
fn a_shortcut_is_captured_refused_when_taken_and_adds_its_node_over_the_graph() {
    let mut harness = harness();
    open(&mut harness);
    click(&mut harness, Role::Tab, "Shortcuts");

    click(&mut harness, Role::Button, "Shortcut for Blur");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::S);
    harness.run_steps(3);
    assert!(harness.state().ui.prefs.node_shortcuts.is_empty(), "Ctrl+S already saves");

    click(&mut harness, Role::Button, "Shortcut for Blur");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::B);
    harness.run_steps(3);
    assert_eq!(harness.state().ui.prefs.node_shortcuts.get("node:blur").map(String::as_str), Some("Ctrl+Shift+B"));

    // The same keys on another entry move the shortcut there; the search brings it into view
    harness.state_mut().ui.preferences.shortcut_search = "levels".into();
    harness.run_steps(2);
    click(&mut harness, Role::Button, "Shortcut for Levels");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::B);
    harness.run_steps(3);
    assert_eq!(harness.state().ui.prefs.node_shortcuts.len(), 1);
    assert!(harness.state().ui.prefs.node_shortcuts.contains_key("node:levels"));
    click(&mut harness, Role::Button, "OK");

    let nodes = harness.state().session.graph().unwrap().nodes.len();
    let canvas = harness.state().graph.view;
    harness.hover_at(canvas.center());
    harness.run_steps(2);
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::B);
    harness.run_steps(3);
    let graph = harness.state().session.graph().unwrap();
    assert_eq!(graph.nodes.len(), nodes + 1);
    assert_eq!(graph.nodes.last().unwrap().def, "levels");
}
