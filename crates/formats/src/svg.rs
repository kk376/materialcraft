//! SVG resources, drawn to a linear-light image. Parsing and drawing go through the shared
//! `craft-svg` (`tree` and `tree-render`): its preflight bounds the bytes (SVGZ included), entity
//! expansion, attributes and nesting before usvg sees the document, its image resolver never
//! reads linked files and bounds embedded images, and its draw check refuses a drawing whose
//! layers, filters or patterns would allocate past the budget instead of aborting.

use crate::{FormatError, Result};
use craft_svg::tree::{self, ImageResolver, Limits, draw, usvg};
use craft_texture::{Image, Kind, MAX_SIDE, srgb_to_linear};

/// Largest SVG file (unpacked when SVGZ).
const MAX_BYTES: usize = 16 << 20;
/// Nesting usvg may build for the document itself, references followed.
const MAX_TREE_DEPTH: usize = 32;
/// Nesting usvg may build in all, the document and the SVG images embedded in it (and in those)
/// together. usvg recurses about 15 kB of stack per level, so 64 levels (about 1 MB) stay inside
/// a 2 MB worker stack whatever the embedding.
const MAX_TOTAL_DEPTH: usize = 64;
/// Bytes the RGBA buffers of one drawing may hold at once: the 8192² target plus its layers.
const DRAW_BUDGET: u64 = 1 << 30;

/// Limits for the document: its own nesting gets [`MAX_TREE_DEPTH`].
fn limits() -> Limits {
    Limits { bytes: MAX_BYTES, depth: MAX_TREE_DEPTH, tree_depth: MAX_TREE_DEPTH, ..Limits::default() }
}

/// The embedded-image resolver. It counts the levels of every embedded SVG on top of the
/// document's, seeded with the most the document may use, so the total stays within
/// [`MAX_TOTAL_DEPTH`] however deep the document or the embedding chain is.
fn image_resolver() -> ImageResolver {
    let resolver = ImageResolver::new(Limits { tree_depth: MAX_TOTAL_DEPTH, depth: MAX_TOTAL_DEPTH, ..limits() });
    resolver.set_document_depth(MAX_TREE_DEPTH);

    resolver
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Image> {
    let limits = limits();
    let resolver = image_resolver();
    let options = usvg::Options { image_href_resolver: resolver.resolver(), ..usvg::Options::default() };

    // Bitmap imports do not use the resolver's optional omission counts
    let tree = tree::parse(bytes, &limits, &options).map_err(svg_error)?;
    let size = tree.size().to_int_size();
    let (width, height) = (size.width(), size.height());

    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(FormatError::Image(format!("SVG exceeds {MAX_SIDE} pixels")));
    }

    let pixels = draw::render(&tree, usvg::Transform::identity(), width, height, 0, DRAW_BUDGET)
        .map_err(|refusal| FormatError::Image(format!("SVG not drawn: {}", refusal.message)))?;

    // Release geometry before allocating the float pixel plane
    drop(tree);

    // Reuse the transfer curve for every possible 8-bit channel value
    const CHANNEL_VALUE_COUNT: usize = u8::MAX as usize + 1;
    let linear_channels: [f32; CHANNEL_VALUE_COUNT] = std::array::from_fn(|value| srgb_to_linear(value as f32 / 255.0));
    let linear_channel = |value: u8| linear_channels.get(usize::from(value)).copied().unwrap_or_default();

    Ok(Image::generate(width, height, Kind::Color, |x, y| {
        let pixel = pixels.pixel(x, y).map(|pixel| pixel.demultiply());
        pixel.map_or([0.0; 4], |pixel| {
            [linear_channel(pixel.red()), linear_channel(pixel.green()), linear_channel(pixel.blue()), f32::from(pixel.alpha()) / 255.0]
        })
    }))
}

fn svg_error(error: tree::Error) -> FormatError {
    FormatError::Image(error.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::decode;
    use crate::FormatError;

    fn svg(body: &str) -> Vec<u8> {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="2">{body}</svg>"#).into_bytes()
    }

    #[test]
    fn draws_a_shape_in_linear_light() {
        let image = decode(&svg(r##"<rect width="2" height="2" fill="#ff0000"/>"##)).unwrap();

        let left = image.get(0, 0, false);

        assert_eq!((image.width(), image.height()), (4, 2));
        assert!((left[0] - 1.0).abs() < 1e-3 && left[1].abs() < 1e-3 && (left[3] - 1.0).abs() < 1e-3, "{left:?}");
        assert_eq!(image.get(3, 1, false)[3], 0.0);
    }

    #[test]
    fn hostile_documents_are_errors_not_crashes() {
        // Entities that expand past the shared bound (a "billion laughs")
        let mut laughs = String::from(r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa">"#);
        for level in 1..10 {
            let prev = char::from(b'a' + level - 1);
            let name = char::from(b'a' + level);
            laughs.push_str(&format!("<!ENTITY {name} \"{}\">", format!("&{prev};").repeat(10)));
        }
        laughs.push_str(r#"]><svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text>&j;</text></svg>"#);

        assert!(decode(laughs.as_bytes()).is_err());

        // Nesting deeper than the tree depth
        let deep = format!("{}{}", "<g>".repeat(200), "</g>".repeat(200));

        assert!(decode(&svg(&deep)).is_err());

        // Larger than the image side limit
        let huge = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="1"/>"#;

        assert!(decode(huge).is_err());
        assert!(decode(b"<svg").is_err());
    }

    #[test]
    fn entity_comment_marker_cannot_hide_later_expansion() {
        // Keep the root inside the public decoder's SVG sniff window
        const ENTITY_VALUE_BYTES: usize = 1024;

        let large_value = "a".repeat(ENTITY_VALUE_BYTES);
        let references = "&large;".repeat(craft_svg::guard::MAX_ENTITY_EXPANSION / ENTITY_VALUE_BYTES + 1);
        let document = format!(
            r#"<!DOCTYPE svg [<!ENTITY bait "<!--"><!ENTITY large "{large_value}">]><svg xmlns="http://www.w3.org/2000/svg" width="1" height="1" id="{references}"/>"#
        );

        let result = crate::decode(document.as_bytes(), false);

        assert!(matches!(result, Err(FormatError::Image(ref message)) if message.contains("entities expand to too much text")), "{result:?}");
    }

    fn nested_groups(levels: usize, inner: &str) -> String {
        format!("{}{inner}{}", "<g>".repeat(levels), "</g>".repeat(levels))
    }

    fn embedded(levels: usize) -> Vec<u8> {
        use base64::Engine as _;

        let inner = svg(&nested_groups(levels, r##"<rect width="4" height="2" fill="#ff0000"/>"##));
        let data = base64::engine::general_purpose::STANDARD.encode(inner);

        svg(&nested_groups(30, &format!(r#"<image href="data:image/svg+xml;base64,{data}" width="4" height="2"/>"#)))
    }

    /// The document and the SVG images it embeds share one nesting budget, so a deep document
    /// cannot embed another deep one and double the stack usvg needs.
    #[test]
    fn embedded_svgs_share_the_nesting_budget() {
        let shallow = decode(&embedded(2)).unwrap();

        assert!(shallow.get(1, 1, false)[0] > 0.9, "{:?}", shallow.get(1, 1, false));

        let deep = decode(&embedded(40)).unwrap();

        assert_eq!(deep.get(1, 1, false)[3], 0.0);

        // The document alone past its own budget is refused
        assert!(decode(&svg(&nested_groups(40, ""))).is_err());
    }

    #[test]
    fn linked_files_are_not_read() {
        let linked = svg(r#"<image href="file:///etc/hostname" width="4" height="2"/>"#);

        let image = decode(&linked).unwrap();

        assert_eq!(image.get(0, 0, false)[3], 0.0);
    }

    #[test]
    fn svgz_is_unpacked() {
        use std::io::Write;

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&svg(r#"<rect width="4" height="2" fill="blue"/>"#)).unwrap();

        let image = decode(&gz.finish().unwrap()).unwrap();

        assert!(image.get(1, 1, false)[2] > 0.9);
    }
}
