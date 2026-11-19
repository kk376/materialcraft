//! The pointer and the keys over the 3D view: dragging orbits the camera, the middle button (or
//! Alt, or Ctrl with the right button) pans, the wheel and the right button zoom, Shift turns the
//! environment and Ctrl+Shift (or the Light tool) moves the first point light. A Shift+click, or
//! a click with the Select tool, selects the mesh

use egui::{Key, Modifiers, PointerButton, Pos2, Rect, Response, Ui, Vec2};
use materialcraft_engine::View3d;
use materialcraft_engine::geom::view::PointLight;
use materialcraft_render::Settings;
use serde_json::json;

use super::Tool;
use super::sight::Sight;
use crate::MaterialCraftApp;

/// Degrees the camera or a light turns for each point the pointer moves
const ORBIT_SPEED: f32 = 0.4;
/// Degrees the environment turns for each point the pointer moves
const ENVIRONMENT_SPEED: f32 = 0.5;
/// The share of the camera's distance one notch of the wheel takes
const WHEEL_ZOOM: f32 = 0.025;
/// The share of the camera's distance a point of a right-button drag takes
const DRAG_ZOOM: f32 = 0.01;
/// Mesh radii a light moves for each point the pointer moves
const LIGHT_DISTANCE_SPEED: f32 = 0.02;

/// Apply this frame's gestures and keys. `area` is where the camera's picture is on screen.
/// Returns true while the picture is changing under the pointer
pub(super) fn handle(app: &mut MaterialCraftApp, ui: &Ui, response: &Response, area: Rect) -> bool {
    let modifiers = ui.input(|input| input.modifiers);
    let control = modifiers.ctrl || modifiers.command;
    let on_light = app.view3d.tool == Tool::Light || (control && modifiers.shift);
    let mut moving = false;

    if response.dragged() {
        let delta = response.drag_delta();
        let (primary, secondary) = (response.dragged_by(PointerButton::Primary), response.dragged_by(PointerButton::Secondary));

        if on_light && primary {
            turn_light(app, delta);
        } else if on_light && secondary {
            move_light(app, delta);
        } else if response.dragged_by(PointerButton::Middle) || (primary && modifiers.alt) || (secondary && control) {
            pan(app, delta, area);
        } else if modifiers.shift && (primary || secondary) {
            let view = &mut app.session.view3d;
            view.env_rotation = (view.env_rotation + delta.x * ENVIRONMENT_SPEED).rem_euclid(360.0);
        } else if primary {
            let view = &mut app.session.view3d;
            view.yaw = (view.yaw - delta.x * ORBIT_SPEED).rem_euclid(360.0);
            view.pitch = (view.pitch + delta.y * ORBIT_SPEED).clamp(-89.0, 89.0);
        } else if secondary {
            zoom(&mut app.session.view3d, delta.y * DRAG_ZOOM);
        }
        moving = true;
    }

    if response.hovered() {
        let scroll = app.ui.prefs.zoom_direction() * ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            // One notch of a mouse wheel scrolls one line
            let points_per_notch = ui.ctx().options(|options| options.input_options.line_scroll_speed).max(1.0);
            zoom(&mut app.session.view3d, -scroll / points_per_notch * WHEEL_ZOOM);
            moving = true;
        }
    }

    let selecting = app.view3d.tool == Tool::Select || modifiers.shift;
    if selecting && response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            app.view3d.selected = hits_mesh(app, pointer, area);
        }
    } else if response.double_clicked() {
        app.run("view.3d.resetCamera", json!({}));
    }

    keys(app, ui, response);
    moving
}

/// Move the camera nearer (negative) or farther by a share of its distance
fn zoom(view: &mut View3d, amount: f32) {
    view.distance = (view.distance * amount.exp()).clamp(*View3d::DISTANCE.start(), *View3d::DISTANCE.end());
}

/// Slide the point the camera orbits so the mesh follows the pointer
fn pan(app: &mut MaterialCraftApp, delta: Vec2, area: Rect) {
    let Some((_, scene)) = &app.view3d.scene else { return };

    let preferences = app.session.camera;
    let speed = preferences.pan_speed / area.height().max(1.0);
    let target = materialcraft_render::pan(scene, &Settings::for_view(&app.session.view3d), delta.x * speed, delta.y * speed);
    app.session.view3d.target = preferences.clamp_target(target);
}

/// Turn the first point light around the scene, switching it on so the drag shows
fn turn_light(app: &mut MaterialCraftApp, delta: Vec2) {
    let light = &mut app.session.view3d.lights[0];
    *light = PointLight { enabled: true, yaw: light.yaw + delta.x * ORBIT_SPEED, pitch: light.pitch - delta.y * ORBIT_SPEED, ..*light }.sanitized();
}

/// Move the first point light nearer to the scene or farther from it
fn move_light(app: &mut MaterialCraftApp, delta: Vec2) {
    let light = &mut app.session.view3d.lights[0];
    *light = PointLight { enabled: true, distance: light.distance + delta.y * LIGHT_DISTANCE_SPEED, ..*light }.sanitized();
}

/// The ray from the camera through the pointer meets the mesh
fn hits_mesh(app: &mut MaterialCraftApp, pointer: Pos2, area: Rect) -> bool {
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return false;
    }

    let scene = super::frame::shown_scene(app);
    let sight = Sight::of(&scene, super::frame::shown_view(app), area.width() / area.height());
    let ray = sight.ray([(pointer.x - area.left()) / area.width(), (pointer.y - area.top()) / area.height()]);
    scene.bvh.closest(ray.origin, ray.dir, f32::MAX).is_some()
}

/// Put back what the active tool moves: where the first light is with the Light tool, else the
/// camera with its lens and the rest of its properties
pub(super) fn reset_selected(app: &mut MaterialCraftApp) {
    if app.view3d.tool == Tool::Light {
        let home = PointLight::first();
        app.run("view.3d", json!({"light1": {"yaw": home.yaw, "pitch": home.pitch, "distance": home.distance}}));
    } else {
        app.run("view.3d.resetCamera", json!({"properties": true}));
    }
}

/// The keys of the view, while the pointer is over it and nothing else has the keyboard
fn keys(app: &mut MaterialCraftApp, ui: &Ui, response: &Response) {
    if !response.hovered() || ui.ctx().egui_wants_keyboard_input() || crate::shell::modality::Modality::capture(app, ui.ctx()).is_blocked() {
        return;
    }

    let pressed = |modifiers: Modifiers, key: Key| ui.input(|input| input.modifiers.matches_exact(modifiers) && input.key_pressed(key));
    if pressed(Modifiers::NONE, Key::F) {
        super::frame_view(app);
    }
    if pressed(Modifiers::NONE, Key::R) {
        app.run("view.3d.resetCamera", json!({}));
    }
    if pressed(Modifiers::SHIFT, Key::R) {
        reset_selected(app);
    }
    if pressed(Modifiers::ALT, Key::S) {
        super::output::save_render(app);
    }
    if pressed(Modifiers::ALT, Key::C) {
        super::output::copy_render(app, ui.ctx());
    }
    if pressed(Modifiers::NONE, Key::C) {
        app.view3d.tool = Tool::Camera;
    }
    if pressed(Modifiers::NONE, Key::L) {
        app.view3d.tool = Tool::Light;
    }
    if pressed(Modifiers::NONE, Key::Escape) {
        app.view3d.selected = false;
    }
}
