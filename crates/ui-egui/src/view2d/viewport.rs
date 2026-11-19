//! The image area of the 2D view: the image over its backdrop, tiling, the overlays, pan, zoom and keys

use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use egui::{
    Color32, ColorImage, Painter, PointerButton, Pos2, Rect, Response, Sense, Stroke, TextureFilter, TextureHandle, TextureId, TextureOptions, TextureWrapMode,
    Ui, pos2, vec2,
};
use egui_phosphor::regular as icon;
use materialcraft_engine::image::Image;
use serde_json::json;

use super::shown::Shown;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::state::ImageCamera;
use crate::theme::Tokens;

/// Width of the strip with the color sampler tool, shown while the Information panel is open
const TOOL_STRIP_WIDTH: f32 = 26.0;
/// Side of one checkerboard square on screen
const CHECKER_SIDE: f32 = 8.0;
const CHECKER_LIGHT: Color32 = Color32::from_gray(0x9a);
const CHECKER_DARK: Color32 = Color32::from_gray(0x66);
/// Radius of the ring that marks the color sampler
const SAMPLER_RADIUS: f32 = 5.0;
const SIZE_LINE_TEXT: f32 = 11.0;
/// Distance of the size line from the corner of the image area
const SIZE_LINE_MARGIN: f32 = 6.0;
/// Opacity of the backing behind the size line
const SIZE_LINE_BACKING: u8 = 200;
/// The largest background image file that is read
#[cfg(not(target_arch = "wasm32"))]
const MAX_BACKGROUND_BYTES: u64 = 256 << 20;
/// How far the physical size may stretch the image, so a degenerate size keeps it visible
const MAX_PHYSICAL_STRETCH: f32 = 100.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, shown: Option<&Shown>, height: f32) {
    let tokens = Tokens::get(ui.ctx());
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());

    let Some(shown) = shown else {
        let response = ui.interact(area, ui.scope_id().with("view2d-empty"), Sense::hover());
        crate::widgets::name(&response, egui::Role::Image, "2D view");
        ui.painter().rect_filled(area, 0.0, tokens.viewport);
        ui.painter().text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            tr("Click a node to show it here"),
            egui::FontId::proportional(12.0),
            tokens.viewport_text,
        );
        app.view2d.texture = None;
        app.view2d.shown = None;
        app.view2d.shown_premultiplied = false;
        app.view2d.histogram = None;
        app.view2d.image_size = None;
        app.view2d.hover = None;
        app.view2d.histogram_drag = None;
        return;
    };

    let mut rect = area;
    if app.session.view2d.information {
        rect.min.x = (rect.min.x + TOOL_STRIP_WIDTH).min(rect.max.x);
        tool_strip(ui, Rect::from_min_max(area.min, pos2(rect.left(), area.bottom())));
    }
    let response = ui.interact(rect, ui.scope_id().with("view2d-image"), Sense::click_and_drag());
    crate::widgets::name(&response, egui::Role::Image, "2D view");
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, tokens.viewport);

    let texture = texture(app, ui.ctx(), shown);
    let tile = place(app, ui, &response, rect, shown);
    keys(app, ui, &response);

    let view = app.session.view2d.clone();

    // The readout keeps the last pixel the pointer was over
    if let Some(fraction) = response.hover_pos().and_then(|pointer| fraction_under(pointer, tile, view.tiled)) {
        app.view2d.hover = Some(pixel(&shown.image, fraction));
    }
    if view.information
        && (response.clicked() || response.dragged_by(PointerButton::Primary))
        && let Some(fraction) = response.interact_pointer_pos().and_then(|pointer| fraction_under(pointer, tile, view.tiled))
    {
        app.run("view.2d", json!({"sampler": fraction}));
    }

    // Tiling covers the whole view; a single image only its own rectangle
    let covered = if view.tiled { rect } else { tile.intersect(rect) };
    if covered.is_positive() {
        if !shown.image.is_gray() && view.channel == "rgba" {
            backdrop(app, ui.ctx(), &painter, tile, covered, view.checkerboard);
        }
        paint_image(&painter, texture, tile, covered);
    }
    if view.tiled {
        painter.rect_stroke(tile, 0.0, Stroke::new(1.0, tokens.viewport_accent), egui::StrokeKind::Outside);
    }
    if view.uv {
        let edges = super::uv_overlay::edges(app);
        super::uv_overlay::paint(&painter, &edges, tile, rect, tokens.viewport_accent);
    }
    if view.information
        && let Some([x, y]) = app.session.view2d.sampler
    {
        sampler_mark(&painter, pos2(tile.left() + x * tile.width(), tile.top() + y * tile.height()));
    }

    size_line(&painter, rect, shown, tokens.viewport_text);
}

/// The size, kind and bit depth of the image, on a dark backing so it reads over any image
fn size_line(painter: &Painter, rect: Rect, shown: &Shown, color: Color32) {
    let kind = if shown.image.is_gray() { tr("Grayscale") } else { "RGBA" };
    let line = format!("{} x {} ({kind}, {}bpc)", shown.image.width(), shown.image.height(), shown.bits);
    let text = painter.layout_no_wrap(line, egui::FontId::proportional(SIZE_LINE_TEXT), color);
    let corner = rect.left_bottom() + vec2(SIZE_LINE_MARGIN, -SIZE_LINE_MARGIN - text.size().y);

    painter.rect_filled(Rect::from_min_size(corner, text.size()).expand2(vec2(4.0, 2.0)), 3.0, Color32::from_black_alpha(SIZE_LINE_BACKING));
    painter.galley(corner, text, color);
}

/// The color sampler is the view's only tool, so it is always the active one
fn tool_strip(ui: &mut Ui, strip: Rect) {
    let tokens = Tokens::get(ui.ctx());
    ui.painter().rect_filled(strip, 0.0, tokens.panel);

    let mut tools = ui.new_child(egui::UiBuilder::new().max_rect(strip.shrink(2.0)));
    crate::widgets::icon_button(&mut tools, icon::EYEDROPPER, tr("Color sampler: click the image to place it"), true, TOOL_STRIP_WIDTH - 4.0);
}

/// Update the camera from this frame's input and return where the image sits on screen
fn place(app: &mut MaterialCraftApp, ui: &Ui, response: &Response, rect: Rect, shown: &Shown) -> Rect {
    let (maintain_zoom, sampling) = (app.session.view2d.maintain_zoom, app.session.view2d.information);
    let size = vec2(shown.image.width() as f32 * physical_stretch(app, &shown.image), shown.image.height() as f32);
    let mut camera = app.ui.image_camera.sanitized();

    // Without the zoom lock a new image size keeps the image as large on screen as the last one
    let current = [shown.image.width(), shown.image.height()];
    if let Some(previous) = app.view2d.image_size.replace(current)
        && previous != current
        && !maintain_zoom
        && !camera.fit
    {
        let longest_side = |size: [u32; 2]| size[0].max(size[1]).max(1) as f32;
        camera = ImageCamera { zoom: camera.zoom * longest_side(previous) / longest_side(current), ..camera }.sanitized();
    }

    if camera.fit {
        let fit = (rect.width() / size.x).min(rect.height() / size.y);
        camera.zoom = if fit.is_finite() && fit > 0.0 { fit } else { ImageCamera::ZOOM.0 };
        camera.offset = [0.0, 0.0];
    }

    if response.hovered() {
        let scroll = app.ui.prefs.zoom_direction() * ui.input(|input| input.smooth_scroll_delta.y);
        if scroll != 0.0 {
            // One notch of a mouse wheel scrolls one line, and zooms one step
            let points_per_notch = ui.ctx().options(|options| options.input_options.line_scroll_speed).max(1.0);
            let anchor = response.hover_pos().map_or([0.0, 0.0], |pointer| [pointer.x - rect.center().x, pointer.y - rect.center().y]);
            camera = camera.zoomed(scroll / points_per_notch, anchor);
        }
    }

    // The primary button places the color sampler while the Information panel is open
    if response.dragged_by(PointerButton::Middle) || (!sampling && response.dragged_by(PointerButton::Primary)) {
        let moved = response.drag_delta();
        camera.fit = false;
        camera.offset = [camera.offset[0] + moved.x, camera.offset[1] + moved.y];
    }

    app.ui.image_camera = camera;
    Rect::from_center_size(rect.center() + vec2(camera.offset[0], camera.offset[1]), size * camera.zoom)
}

/// How much wider the image is drawn so it has the proportions of the graph's physical size
fn physical_stretch(app: &MaterialCraftApp, image: &Image) -> f32 {
    if !app.session.view2d.physical_size {
        return 1.0;
    }

    let Ok(graph) = app.session.graph() else { return 1.0 };
    stretch_for(graph.physical_size, [image.width(), image.height()])
}

/// The width factor that gives a `pixels` sized image the proportions of `physical`; 1 without a physical size
fn stretch_for(physical: [f32; 3], pixels: [u32; 2]) -> f32 {
    let [width, height, _] = physical;
    if !(width > 0.0 && height > 0.0) {
        return 1.0;
    }

    let stretch = (width / height) / (pixels[0].max(1) as f32 / pixels[1].max(1) as f32);
    if stretch.is_finite() { stretch.clamp(1.0 / MAX_PHYSICAL_STRETCH, MAX_PHYSICAL_STRETCH) } else { 1.0 }
}

/// Space toggles tiling, F fits the image and C, R, G, B and A pick the channels, while the pointer is over the image
fn keys(app: &mut MaterialCraftApp, ui: &Ui, response: &Response) {
    if !response.hovered() || ui.ctx().egui_wants_keyboard_input() || crate::shell::modality::Modality::capture(app, ui.ctx()).is_blocked() {
        return;
    }

    let pressed = |key: egui::Key| ui.input(|input| input.modifiers.is_none() && input.key_pressed(key));
    if pressed(egui::Key::Space) {
        let tiled = app.session.view2d.tiled;
        app.run("view.2d", json!({"tiled": !tiled}));
    }
    if pressed(egui::Key::F) {
        app.ui.image_camera = ImageCamera::default();
    }
    for (channel, _, key) in super::bottom_toolbar::CHANNELS {
        if pressed(key) {
            app.run("view.2d", json!({"channel": channel}));
        }
    }
}

/// The texture of the shown image, uploaded again when the image or a display setting changes
fn texture(app: &mut MaterialCraftApp, context: &egui::Context, shown: &Shown) -> TextureId {
    let view = &app.session.view2d;
    let limit = context.input(|input| input.max_texture_side).max(1);
    let mut settings = std::hash::DefaultHasher::new();
    (&view.channel, view.srgb, view.premultiplied, view.tiled, limit).hash(&mut settings);
    let key = shown.key ^ settings.finish();
    if let Some((cached, texture)) = &app.view2d.texture
        && *cached == key
    {
        return texture.id();
    }

    // The source pixels stay whole for the readout and screenshots; the upload respects the renderer's limit
    let image = &shown.image;
    let pixels = super::display::display_pixels(image, view);
    let (size, upload) = if image.width().max(image.height()) as usize > limit {
        let scale = limit as f64 / f64::from(image.width().max(image.height()));
        let side = |pixels: u32| (f64::from(pixels) * scale).floor().max(1.0) as u32;
        let smaller = image.resize(side(image.width()), side(image.height()), false);
        ([smaller.width() as usize, smaller.height() as usize], Cow::Owned(super::display::display_pixels(&smaller, view)))
    } else {
        ([image.width() as usize, image.height() as usize], Cow::Borrowed(&pixels))
    };
    let color_image = if view.premultiplied { ColorImage::from_rgba_premultiplied(size, &upload) } else { ColorImage::from_rgba_unmultiplied(size, &upload) };
    let options = TextureOptions {
        magnification: TextureFilter::Nearest,
        minification: TextureFilter::Linear,
        wrap_mode: if view.tiled { TextureWrapMode::Repeat } else { TextureWrapMode::ClampToEdge },
        mipmap_mode: None,
    };

    let texture = context.load_texture("view2d", color_image, options);
    let id = texture.id();
    app.view2d.texture = Some((key, texture));
    app.view2d.shown = Some((image.width(), image.height(), std::sync::Arc::new(pixels)));
    app.view2d.shown_premultiplied = view.premultiplied;
    id
}

/// What shows through transparent pixels: a checkerboard, or black with the background image over it
fn backdrop(app: &mut MaterialCraftApp, context: &egui::Context, painter: &Painter, tile: Rect, covered: Rect, checkerboard: bool) {
    if checkerboard {
        paint_checkerboard(painter, covered);
        return;
    }

    painter.rect_filled(covered, 0.0, Color32::BLACK);
    if let Some(background) = background_texture(app, context) {
        paint_image(painter, background, tile, tile.intersect(covered));
    }
}

fn paint_checkerboard(painter: &Painter, area: Rect) {
    painter.rect_filled(area, 0.0, CHECKER_LIGHT);

    let mut top = area.top();
    let mut row = 0;
    while top < area.bottom() {
        let mut left = area.left() + if row % 2 == 0 { 0.0 } else { CHECKER_SIDE };
        while left < area.right() {
            painter.rect_filled(Rect::from_min_size(pos2(left, top), vec2(CHECKER_SIDE, CHECKER_SIDE)).intersect(area), 0.0, CHECKER_DARK);
            left += CHECKER_SIDE * 2.0;
        }
        top += CHECKER_SIDE;
        row += 1;
    }
}

/// Draw the part of a texture that `covered` shows, when the whole texture would fill `tile`
fn paint_image(painter: &Painter, texture: TextureId, tile: Rect, covered: Rect) {
    if !covered.is_positive() || !tile.is_positive() {
        return;
    }

    let uv = |position: Pos2| pos2((position.x - tile.left()) / tile.width(), (position.y - tile.top()) / tile.height());
    let mut mesh = egui::Mesh::with_texture(texture);
    mesh.add_rect_with_uv(covered, Rect::from_min_max(uv(covered.min), uv(covered.max)), Color32::WHITE);
    painter.add(mesh);
}

/// A ring with a cross through it, dark under light so it shows on any image
fn sampler_mark(painter: &Painter, center: Pos2) {
    for (width, color) in [(3.0, Color32::from_black_alpha(160)), (1.0, Color32::WHITE)] {
        let stroke = Stroke::new(width, color);
        painter.circle_stroke(center, SAMPLER_RADIUS, stroke);
        for reach in [vec2(SAMPLER_RADIUS + 3.0, 0.0), vec2(0.0, SAMPLER_RADIUS + 3.0)] {
            painter.line_segment([center - reach, center - reach * 0.4], stroke);
            painter.line_segment([center + reach * 0.4, center + reach], stroke);
        }
    }
}

/// The position under `pointer` as fractions of the image; with tiling every repeat counts as the image
fn fraction_under(pointer: Pos2, tile: Rect, tiled: bool) -> Option<[f32; 2]> {
    if !tile.is_positive() {
        return None;
    }

    let fraction = [(pointer.x - tile.left()) / tile.width(), (pointer.y - tile.top()) / tile.height()];
    if tiled {
        Some(fraction.map(|coordinate| coordinate.rem_euclid(1.0)))
    } else {
        fraction.iter().all(|coordinate| (0.0..1.0).contains(coordinate)).then_some(fraction)
    }
}

/// The pixel at a position given as fractions of the image
fn pixel(image: &Image, fraction: [f32; 2]) -> [u32; 2] {
    let along = |fraction: f32, side: u32| ((fraction.clamp(0.0, 1.0) * side as f32) as u32).min(side.saturating_sub(1));
    [along(fraction[0], image.width()), along(fraction[1], image.height())]
}

/// The texture of the background image, read again when another file is chosen
fn background_texture(app: &mut MaterialCraftApp, context: &egui::Context) -> Option<TextureId> {
    let Some(path) = app.session.view2d.background.clone() else {
        app.view2d.background = None;
        return None;
    };

    if app.view2d.background.as_ref().is_none_or(|(loaded, _)| *loaded != path) {
        let texture = match load_background(context, &path) {
            Ok(texture) => Some(texture),
            Err(error) => {
                app.toast(format!("{}: {error}", tr("The background image could not be shown")), true);
                None
            }
        };
        app.view2d.background = Some((path, texture));
    }
    app.view2d.background.as_ref().and_then(|(_, texture)| texture.as_ref().map(TextureHandle::id))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_background(context: &egui::Context, path: &str) -> Result<TextureHandle, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    // A pipe can report zero bytes but block the draw path indefinitely on read
    if !metadata.is_file() {
        return Err(format!("{path}: not a regular file"));
    }
    if metadata.len() > MAX_BACKGROUND_BYTES {
        return Err(format!("{path} is larger than {} MB", MAX_BACKGROUND_BYTES >> 20));
    }

    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let image = materialcraft_engine::formats::decode(&bytes, false).map_err(|error| error.to_string())?;
    let limit = context.input(|input| input.max_texture_side).max(1);
    let image = if image.width().max(image.height()) as usize > limit {
        let scale = limit as f64 / f64::from(image.width().max(image.height()));
        let side = |pixels: u32| (f64::from(pixels) * scale).floor().max(1.0) as u32;
        image.resize(side(image.width()), side(image.height()), false)
    } else {
        image
    };

    let pixels = ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], &image.to_rgba8_display());
    Ok(context.load_texture("view2d-background", pixels, TextureOptions::LINEAR))
}

#[cfg(target_arch = "wasm32")]
fn load_background(_: &egui::Context, _: &str) -> Result<TextureHandle, String> {
    Err("the browser cannot read files by path".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_engine::core::NodeId;

    #[cfg(unix)]
    #[test]
    fn a_device_file_is_rejected_before_the_background_read() {
        let error = load_background(&egui::Context::default(), "/dev/null").err().unwrap();

        assert!(error.contains("not a regular file"), "{error}");
    }

    #[test]
    fn an_empty_view_releases_the_previous_image_and_screenshot() {
        let context = egui::Context::default();
        let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default());
        let mut peak_screenshot_bytes = 0;

        for index in 0..8 {
            let image = Image::filled(2048, 64, materialcraft_engine::image::Kind::Color, [index as f32 / 8.0; 4]);
            let shown = Shown { node: NodeId(0), image, key: index + 1, levels: None, bits: "32" };
            let texture_id = texture(&mut app, &context, &shown);
            peak_screenshot_bytes = peak_screenshot_bytes.max(app.view2d.shown.as_ref().unwrap().2.capacity());

            context.run_ui(egui::RawInput::default(), |ui| show(&mut app, ui, None, 100.0)).textures_delta.clear();

            assert!(app.view2d.shown.is_none(), "an empty view must not return an obsolete screenshot");
            assert!(app.view2d.texture.is_none());
            assert!(context.tex_manager().read().meta(texture_id).is_none(), "deleting the displayed image must release its texture");
        }

        println!(
            "distinct empty-view transitions: 8; peak screenshot capacity before deletion: {peak_screenshot_bytes} bytes; retained screenshot bytes after deletion: 0"
        );
    }

    #[test]
    fn distinct_images_release_the_previous_texture_and_bound_the_screenshot_buffer() {
        let context = egui::Context::default();
        context.begin_pass(egui::RawInput { max_texture_side: Some(1024), ..Default::default() });
        let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default());
        let mut previous_texture = None;
        let mut peak_retained_screenshot_bytes = 0;

        for index in 0..8 {
            let image = Image::filled(2048, 64, materialcraft_engine::image::Kind::Color, [index as f32 / 8.0; 4]);
            let shown = Shown { node: NodeId(0), image, key: index + 1, levels: None, bits: "32" };
            let texture_id = texture(&mut app, &context, &shown);
            assert_eq!(app.view2d.texture.as_ref().unwrap().1.size(), [1024, 32]);
            if let Some(previous) = previous_texture.replace(texture_id) {
                assert!(context.tex_manager().read().meta(previous).is_none(), "an earlier image texture stayed alive");
            }
            let retained = app.view2d.shown.as_ref().unwrap().2.capacity();
            peak_retained_screenshot_bytes = peak_retained_screenshot_bytes.max(retained);
            assert_eq!(app.view2d.shown.as_ref().unwrap().2.len(), 2048 * 64 * 4);
        }

        println!("distinct images: 8; upload resizes: 8; peak retained screenshot capacity: {peak_retained_screenshot_bytes} bytes");
        assert!(peak_retained_screenshot_bytes <= 2048 * 64 * 4);
        let last_texture = previous_texture.unwrap();
        drop(app);
        assert!(context.tex_manager().read().meta(last_texture).is_none(), "the last texture survived view shutdown");
        let mut deltas = context.end_pass().textures_delta;
        deltas.clear();
    }

    #[test]
    fn a_pointer_reads_the_image_under_it_and_every_repeat_when_tiled() {
        let tile = Rect::from_min_size(pos2(100.0, 50.0), vec2(200.0, 100.0));

        assert_eq!(fraction_under(pos2(150.0, 75.0), tile, false), Some([0.25, 0.25]));
        assert_eq!(fraction_under(pos2(99.0, 75.0), tile, false), None, "outside a single image");
        assert_eq!(fraction_under(pos2(300.0, 75.0), tile, false), None, "the far edge belongs to no pixel");
        assert_eq!(fraction_under(pos2(350.0, -25.0), tile, true), Some([0.25, 0.25]), "one repeat right and one up");
        assert_eq!(fraction_under(pos2(150.0, 75.0), Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0)), true), None);
    }

    #[test]
    fn a_fraction_names_a_pixel_inside_the_image() {
        let image = Image::filled(8, 4, materialcraft_engine::image::Kind::Gray, [0.0; 4]);

        assert_eq!(pixel(&image, [0.25, 0.25]), [2, 1]);
        assert_eq!(pixel(&image, [1.0, 7.0]), [7, 3]);
        assert_eq!(pixel(&image, [-1.0, f32::NAN]), [0, 0]);
    }

    #[test]
    fn the_physical_size_sets_the_proportions_and_a_missing_one_changes_nothing() {
        assert_eq!(stretch_for([2.0, 1.0, 0.0], [512, 512]), 2.0);
        assert_eq!(stretch_for([1.0, 1.0, 0.0], [1024, 512]), 0.5);
        assert_eq!(stretch_for([0.0, 0.0, 0.0], [512, 512]), 1.0);
        assert_eq!(stretch_for([f32::NAN, 1.0, 0.0], [512, 512]), 1.0);
        assert_eq!(stretch_for([1.0e9, 1.0, 0.0], [512, 512]), MAX_PHYSICAL_STRETCH);
    }
}
