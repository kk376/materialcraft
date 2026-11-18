use crate::{FormatError, Result};
use craft_texture::{Image, ImageError, Kind, Limits, MAX_SIDE, dds, srgb_to_linear};

pub(crate) fn decode(bytes: &[u8], linear: bool) -> Result<Image> {
    let format_error = |message: String| FormatError::Image(message);
    let mut limits = Limits::default();
    limits.max_side = MAX_SIDE;

    let decoded = dds::decode_first(bytes, &limits).map_err(|error| match error {
        dds::DecodeError::Image(ImageError::BadSize(_, _)) => format_error(format!("DDS dimensions must be 1..={MAX_SIDE}")),
        error => format_error(error.to_string()),
    })?;

    let mut data = decoded.rgba;
    if !linear && !decoded.source_precision_is_f32 {
        for pixel in data.as_chunks_mut::<4>().0 {
            for value in pixel.iter_mut().take(3) {
                *value = srgb_to_linear(*value);
            }
        }
    }

    Image::from_data(decoded.width, decoded.height, Kind::Color, data, &Limits::default()).map_err(|error| format_error(error.to_string()))
}
