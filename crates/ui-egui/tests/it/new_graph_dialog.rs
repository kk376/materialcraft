//! File ▸ New ▸ Graph: template cards, the size of the graph and the package it goes in

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use materialcraft_ui_egui::{MaterialCraftApp, UiState, menus};

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

fn open(harness: &mut Harness<'static, MaterialCraftApp>, id: &str) {
    let context = harness.ctx.clone();
    menus::invoke(harness.state_mut(), &context, id);
    harness.run_steps(4);
}

#[test]
fn a_card_and_an_unlinked_size_make_the_graph_in_the_active_package() {
    let mut harness = harness();
    let packages = harness.state().session.open_packages().len();
    open(&mut harness, "ui.newGraph");

    harness.get(By::new().role(Role::ListItem).label("Atomic sample")).click();
    harness.run_steps(2);
    harness.get(By::new().role(Role::Button).label("Keep the width and height the same")).click();
    harness.run_steps(2);
    harness.state_mut().ui.new_graph_size = 512;
    harness.state_mut().ui.new_graph_height = 2048;
    harness.get(By::new().role(Role::Button).label("Create")).click();
    harness.run_steps(6);

    let session = &harness.state().session;
    assert!(!harness.state().ui.windows.new_graph);
    assert_eq!(session.open_packages().len(), packages);
    assert_eq!(session.graph().unwrap().size, [9, 11]);
    let sample = materialcraft_engine::samples::template("atomics", "Fixture", 9).unwrap();
    assert_eq!(session.graph().unwrap().nodes, sample.nodes);
}

#[test]
fn a_function_graph_can_go_in_a_new_package() {
    let mut harness = harness();
    let packages = harness.state().session.open_packages().len();
    open(&mut harness, "ui.newFunctionGraph");
    assert!(harness.query_by_label("Size").is_none(), "a function graph has no size");

    harness.state_mut().ui.new_graph_package = None;
    harness.get(By::new().role(Role::Button).label("Create")).click();
    harness.run_steps(6);

    let session = &harness.state().session;
    assert_eq!(session.open_packages().len(), packages + 1);
    assert!(session.graph().unwrap().function.is_some());
}
