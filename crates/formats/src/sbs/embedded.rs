use super::embedded_error::EmbeddedError;
use crate::{FormatError, Result};
use base64::Engine as _;
use std::{
    cell::Cell,
    io::{self, Read, Write},
};

pub(super) fn base64_engine() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

pub(super) fn decode_embedded(strdata: &str) -> std::result::Result<Vec<u8>, String> {
    decode_embedded_bounded(strdata, crate::MAX_RESOURCE, &Cell::new(crate::MAX_RESOURCE + 1)).map_err(|error| error.to_string())
}

pub(super) fn decode_embedded_bounded(strdata: &str, remaining_bytes: u64, inflation_work: &Cell<u64>) -> std::result::Result<Vec<u8>, EmbeddedError> {
    let work_before = inflation_work.get();
    if work_before == 0 {
        return Err(EmbeddedError::WorkBudget);
    }

    let raw = match strdata.split_once(char::is_whitespace) {
        Some((prefix, rest)) => base64_engine().decode(std::iter::once(prefix).chain(rest.split_whitespace()).collect::<String>()),
        None => base64_engine().decode(strdata),
    }
    .map_err(|error| format!("bad base64: {error}"))?;
    let (length, compressed) = raw.split_at_checked(4).ok_or("too short")?;
    let declared_bytes = u64::from(u32::from_be_bytes([length[0], length[1], length[2], length[3]]));
    if declared_bytes > crate::MAX_RESOURCE {
        return Err("embedded resource exceeds the 256 MiB limit".into());
    }

    // Read one probe byte beyond the tighter bound to detect overruns
    let probe_limit = declared_bytes.min(remaining_bytes) + 1;
    let mut decoder = flate2::read::ZlibDecoder::new(compressed);
    let mut limited = decoder.by_ref().take(probe_limit.min(work_before));
    let mut output = Vec::new();
    let result = if declared_bytes > remaining_bytes || declared_bytes >= work_before {
        // Discard probe output that cannot become a retained asset
        io::copy(&mut limited, &mut io::sink())
    } else {
        limited.read_to_end(&mut output).map(|bytes| bytes as u64)
    };

    // Charge every emitted byte, including output produced before a checksum error
    inflation_work.set(work_before.saturating_sub(decoder.total_out()));
    let actual_bytes = result.map_err(|error| format!("bad compressed data: {error}"))?;
    if actual_bytes > declared_bytes {
        return Err(format!("{actual_bytes} bytes where {declared_bytes} were declared").into());
    }

    if work_before < probe_limit && actual_bytes == work_before {
        return Err(EmbeddedError::WorkBudget);
    }

    if actual_bytes > remaining_bytes {
        return Err(EmbeddedError::Budget);
    }

    if actual_bytes != declared_bytes {
        return Err(format!("{actual_bytes} bytes where {declared_bytes} were declared").into());
    }

    // ResourceBytes retains the vector and its capacity
    output.shrink_to_fit();
    Ok(output)
}

pub(super) fn encode_embedded(bytes: &[u8]) -> Result<String> {
    let length = u32::try_from(bytes.len()).map_err(|_| FormatError::Io("resource too large".into()))?;
    let mut encoder = flate2::write::ZlibEncoder::new(length.to_be_bytes().to_vec(), flate2::Compression::default());
    encoder.write_all(bytes).map_err(|e| FormatError::Io(e.to_string()))?;
    let raw = encoder.finish().map_err(|e| FormatError::Io(e.to_string()))?;

    Ok(craft_mime::transfer::base64(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_encoder_preserves_embedded_wire_bytes() {
        for len in 0..=513 {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let mut encoder = flate2::write::ZlibEncoder::new((len as u32).to_be_bytes().to_vec(), flate2::Compression::default());
            encoder.write_all(&bytes).unwrap();
            let raw = encoder.finish().unwrap();

            assert_eq!(encode_embedded(&bytes).unwrap(), base64_engine().encode(&raw));
            assert_eq!(decode_embedded(&encode_embedded(&bytes).unwrap()).unwrap(), bytes);
        }
    }

    #[test]
    fn strict_decoder_policy_is_not_replaced_by_mime_tolerance() {
        assert!(decode_embedded("!!!!").unwrap_err().starts_with("bad base64:"));
        assert!(decode_embedded("AA").unwrap_err().starts_with("bad base64:"));

        let encoded = encode_embedded(b"payload").unwrap();
        let spaced = encoded.chars().map(|c| format!("{c} \n\u{2003}")).collect::<String>();

        assert_eq!(decode_embedded(&spaced).unwrap(), b"payload");
    }

    #[test]
    fn aggregate_budget_bounds_inflation_without_masking_early_corruption() {
        for length in [3_u32, 4] {
            let mut raw = length.to_be_bytes().to_vec();
            raw.push(255);
            let encoded = base64_engine().encode(raw);
            assert!(matches!(decode_embedded_bounded(&encoded, 3, &Cell::new(16)), Err(EmbeddedError::Damaged(_))));
        }
        let encoded = encode_embedded(b"four").unwrap();

        assert!(matches!(decode_embedded_bounded(&encoded, 3, &Cell::new(16)), Err(EmbeddedError::Budget)));
        assert_eq!(decode_embedded_bounded(&encoded, 4, &Cell::new(16)).unwrap(), b"four");
    }

    #[test]
    fn inflation_work_counts_discarded_prefixes_and_stops_before_next_decode() {
        let work = Cell::new(8);
        let encoded = encode_embedded(b"four").unwrap();
        for expected in [4, 0] {
            assert!(matches!(decode_embedded_bounded(&encoded, 3, &work), Err(EmbeddedError::Budget)));
            assert_eq!(work.get(), expected);
        }

        // Exhaustion stops the next resource before Base64 decoding
        assert!(matches!(decode_embedded_bounded("!!!!", 3, &work), Err(EmbeddedError::WorkBudget)));

        let work = Cell::new(2);
        assert!(matches!(decode_embedded_bounded(&encoded, 4, &work), Err(EmbeddedError::WorkBudget)));
        assert_eq!(work.get(), 0);
    }

    #[test]
    fn checksum_errors_charge_output_without_spending_retained_budget() {
        let mut raw = base64_engine().decode(encode_embedded(b"four").unwrap()).unwrap();
        *raw.last_mut().unwrap() ^= 1;
        let work = Cell::new(8);

        assert!(matches!(decode_embedded_bounded(&base64_engine().encode(raw), 4, &work), Err(EmbeddedError::Damaged(_))));
        assert_eq!(work.get(), 4);
    }

    #[test]
    fn proven_length_mismatch_precedes_retained_budget() {
        let mut raw = base64_engine().decode(encode_embedded(b"four").unwrap()).unwrap();
        raw[..4].copy_from_slice(&3_u32.to_be_bytes());
        let work = Cell::new(4);

        assert!(matches!(decode_embedded_bounded(&base64_engine().encode(raw), 3, &work), Err(EmbeddedError::Damaged(_))));
        assert_eq!(work.get(), 0);
    }

    #[test]
    fn retained_assets_release_spare_decompression_capacity() {
        let bytes = vec![17; 2 << 20];
        let encoded = encode_embedded(&bytes).unwrap();

        let decoded = decode_embedded(&encoded).unwrap();

        assert_eq!(decoded, bytes);
        assert!(decoded.capacity() < 2 * decoded.len(), "retained {} bytes for a {}-byte asset", decoded.capacity(), decoded.len());
    }

    #[test]
    fn decompression_requires_the_declared_length() {
        let encoded = encode_embedded(&[1, 2, 3, 4]).unwrap();

        assert_eq!(decode_embedded(&encoded).unwrap(), [1, 2, 3, 4]);

        let mut raw = base64_engine().decode(&encoded).unwrap();
        for length in [1_u32, 5] {
            raw[..4].copy_from_slice(&length.to_be_bytes());

            assert!(decode_embedded(&base64_engine().encode(&raw)).unwrap_err().contains("declared"));
        }

        raw[..4].copy_from_slice(&u32::MAX.to_be_bytes());

        assert!(decode_embedded(&base64_engine().encode(&raw)).unwrap_err().contains("256 MiB limit"));
    }
}
