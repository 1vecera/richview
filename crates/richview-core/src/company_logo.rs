//! Validate the static raster bytes that hosts supply for a company logo.

use std::fmt;

/// Largest permitted company logo file, in bytes.
pub const COMPANY_LOGO_MAX_BYTES: usize = 512 * 1024;
/// Largest permitted company logo width or height, in pixels.
pub const COMPANY_LOGO_MAX_DIMENSION: u32 = 1024;

/// The media type and dimensions verified from a company logo's bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedCompanyLogo {
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

/// A company logo violates the raster format, animation or resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompanyLogoError {
    UnsupportedFormat,
    Animated,
    TooLarge,
    InvalidDimensions,
    Malformed,
}

impl fmt::Display for CompanyLogoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => {
                formatter.write_str("Company logo must contain PNG, JPEG or WebP image bytes")
            }
            Self::Animated => formatter.write_str("Company logo must be a static image"),
            Self::TooLarge => write!(
                formatter,
                "Company logo must be at most {} KiB",
                COMPANY_LOGO_MAX_BYTES / 1024
            ),
            Self::InvalidDimensions => write!(
                formatter,
                "Company logo dimensions must be between 1 and {COMPANY_LOGO_MAX_DIMENSION} pixels"
            ),
            Self::Malformed => formatter.write_str("Company logo image is malformed or truncated"),
        }
    }
}

impl std::error::Error for CompanyLogoError {}

/// Validate a logo by sniffing its format, traversing its container and reading its dimensions.
///
/// Accept PNG, JPEG and WebP without animation. Bound the file and both image axes before a host
/// embeds the original bytes. Container checks include PNG checksums and terminal chunks, JPEG
/// marker and scan boundaries, and WebP's RIFF lengths; compressed pixels are not decoded here.
pub fn validate_company_logo(bytes: &[u8]) -> Result<ValidatedCompanyLogo, CompanyLogoError> {
    if bytes.len() > COMPANY_LOGO_MAX_BYTES {
        return Err(CompanyLogoError::TooLarge);
    }
    let (mime, (width, height)) = if bytes.starts_with(b"\x89PNG") {
        ("image/png", validate_png(bytes)?)
    } else if bytes.starts_with(b"\xff\xd8") {
        ("image/jpeg", validate_jpeg(bytes)?)
    } else if bytes.starts_with(b"RIFF") {
        ("image/webp", validate_webp(bytes)?)
    } else {
        return Err(CompanyLogoError::UnsupportedFormat);
    };
    if width == 0
        || height == 0
        || width > COMPANY_LOGO_MAX_DIMENSION
        || height > COMPANY_LOGO_MAX_DIMENSION
    {
        return Err(CompanyLogoError::InvalidDimensions);
    }
    Ok(ValidatedCompanyLogo {
        mime,
        width,
        height,
    })
}

fn validate_png(bytes: &[u8]) -> Result<(u32, u32), CompanyLogoError> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(CompanyLogoError::Malformed);
    }
    let mut offset = 8;
    let mut has_header = false;
    let mut dimensions = (0, 0);
    let mut has_palette = false;
    let mut image_started = false;
    let mut has_image = false;
    let mut image_ended = false;
    let mut color_type = 0;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or(CompanyLogoError::Malformed)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let kind = &header[4..];
        let end = offset
            .checked_add(12)
            .and_then(|start| start.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or(CompanyLogoError::Malformed)?;
        let data = &bytes[offset + 8..end - 4];
        if length > i32::MAX as usize
            || !kind.iter().all(u8::is_ascii_alphabetic)
            || !kind[2].is_ascii_uppercase()
            || crc32(&bytes[offset + 4..end - 4])
                != u32::from_be_bytes(bytes[end - 4..end].try_into().unwrap())
        {
            return Err(CompanyLogoError::Malformed);
        }
        if !has_header && kind != b"IHDR" {
            return Err(CompanyLogoError::Malformed);
        }
        match kind {
            b"acTL" | b"fcTL" | b"fdAT" => return Err(CompanyLogoError::Animated),
            b"IHDR" => {
                if has_header || length != 13 {
                    return Err(CompanyLogoError::Malformed);
                }
                color_type = data[9];
                let valid_depth = match color_type {
                    0 => matches!(data[8], 1 | 2 | 4 | 8 | 16),
                    2 | 4 | 6 => matches!(data[8], 8 | 16),
                    3 => matches!(data[8], 1 | 2 | 4 | 8),
                    _ => false,
                };
                if !valid_depth || data[10] != 0 || data[11] != 0 || data[12] > 1 {
                    return Err(CompanyLogoError::Malformed);
                }
                dimensions = (
                    u32::from_be_bytes(data[..4].try_into().unwrap()),
                    u32::from_be_bytes(data[4..8].try_into().unwrap()),
                );
                has_header = true;
            }
            b"PLTE" => {
                if has_palette
                    || image_started
                    || matches!(color_type, 0 | 4)
                    || length == 0
                    || length > 768
                    || !length.is_multiple_of(3)
                {
                    return Err(CompanyLogoError::Malformed);
                }
                has_palette = true;
            }
            b"IDAT" => {
                if image_ended || (color_type == 3 && !has_palette) {
                    return Err(CompanyLogoError::Malformed);
                }
                image_started = true;
                has_image |= !data.is_empty();
            }
            b"IEND" => {
                return if length == 0 && has_image && end == bytes.len() {
                    Ok(dimensions)
                } else {
                    Err(CompanyLogoError::Malformed)
                };
            }
            _ if kind[0].is_ascii_uppercase() => return Err(CompanyLogoError::Malformed),
            _ => {}
        }
        if kind != b"IDAT" && image_started {
            image_ended = true;
        }
        offset = end;
    }
    Err(CompanyLogoError::Malformed)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn validate_jpeg(bytes: &[u8]) -> Result<(u32, u32), CompanyLogoError> {
    if !bytes.starts_with(b"\xff\xd8") {
        return Err(CompanyLogoError::Malformed);
    }
    let mut offset = 2;
    let mut frame = None;
    let mut has_scan = false;
    while offset < bytes.len() {
        if bytes[offset] != 0xff {
            return Err(CompanyLogoError::Malformed);
        }
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes.get(offset).ok_or(CompanyLogoError::Malformed)?;
        offset += 1;
        match marker {
            // DNL can redefine the height after the SOF; the bounded logo format requires the
            // complete dimensions in its single frame header.
            0xdc => return Err(CompanyLogoError::Malformed),
            0xd9 => {
                return if has_scan && offset == bytes.len() {
                    frame.ok_or(CompanyLogoError::Malformed)
                } else {
                    Err(CompanyLogoError::Malformed)
                };
            }
            0x00 | 0xd0..=0xd8 => return Err(CompanyLogoError::Malformed),
            0x01 => continue,
            _ => {}
        }
        let length_bytes = bytes
            .get(offset..offset + 2)
            .ok_or(CompanyLogoError::Malformed)?;
        let length = usize::from(u16::from_be_bytes(length_bytes.try_into().unwrap()));
        let end = offset
            .checked_add(length)
            .filter(|end| length >= 2 && *end <= bytes.len())
            .ok_or(CompanyLogoError::Malformed)?;
        let data = &bytes[offset + 2..end];
        offset = end;
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if frame.is_some()
                || data.len() < 6
                || data[5] == 0
                || data.len() != 6 + usize::from(data[5]) * 3
            {
                return Err(CompanyLogoError::Malformed);
            }
            frame = Some((
                u32::from(u16::from_be_bytes(data[3..5].try_into().unwrap())),
                u32::from(u16::from_be_bytes(data[1..3].try_into().unwrap())),
            ));
        }
        if marker != 0xda {
            continue;
        }
        if frame.is_none()
            || data.is_empty()
            || data[0] == 0
            || data.len() != 4 + usize::from(data[0]) * 2
        {
            return Err(CompanyLogoError::Malformed);
        }
        has_scan = true;
        // Entropy-coded scans can contain byte stuffing, restart markers and marker fill bytes.
        // Other markers return to the outer loop, including later scans of progressive JPEGs.
        while offset < bytes.len() {
            if bytes[offset] != 0xff {
                offset += 1;
                continue;
            }
            let start = offset;
            while bytes.get(offset) == Some(&0xff) {
                offset += 1;
            }
            match bytes.get(offset) {
                Some(0x00 | 0xd0..=0xd7) => offset += 1,
                Some(_) => {
                    offset = start;
                    break;
                }
                None => return Err(CompanyLogoError::Malformed),
            }
        }
    }
    Err(CompanyLogoError::Malformed)
}

fn validate_webp(bytes: &[u8]) -> Result<(u32, u32), CompanyLogoError> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Err(CompanyLogoError::Malformed);
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if length != bytes.len() - 8 || !length.is_multiple_of(2) {
        return Err(CompanyLogoError::Malformed);
    }
    let mut offset = 12;
    let mut canvas = None;
    let mut image = None;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or(CompanyLogoError::Malformed)?;
        let kind = &header[..4];
        let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(8)
            .and_then(|start| start.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or(CompanyLogoError::Malformed)?;
        let padded_end = end + length % 2;
        if padded_end > bytes.len() || (padded_end != end && bytes[end] != 0) {
            return Err(CompanyLogoError::Malformed);
        }
        let data = &bytes[offset + 8..end];
        match kind {
            b"ANIM" | b"ANMF" => return Err(CompanyLogoError::Animated),
            b"VP8X" => {
                if offset != 12 || data.len() != 10 {
                    return Err(CompanyLogoError::Malformed);
                }
                if data[0] & 0x02 != 0 {
                    return Err(CompanyLogoError::Animated);
                }
                if data[0] & 0xc1 != 0 || data[1..4] != [0, 0, 0] {
                    return Err(CompanyLogoError::Malformed);
                }
                canvas = Some((little_u24(&data[4..7]) + 1, little_u24(&data[7..10]) + 1));
            }
            b"VP8 " | b"VP8L" => {
                if image.is_some() {
                    return Err(CompanyLogoError::Malformed);
                }
                let dimensions = if kind == b"VP8 " {
                    if data.len() < 10 || data[0] & 1 != 0 || data[3..6] != [0x9d, 0x01, 0x2a] {
                        return Err(CompanyLogoError::Malformed);
                    }
                    (
                        u32::from(u16::from_le_bytes(data[6..8].try_into().unwrap()) & 0x3fff),
                        u32::from(u16::from_le_bytes(data[8..10].try_into().unwrap()) & 0x3fff),
                    )
                } else {
                    if data.len() < 5 || data[0] != 0x2f || data[4] & 0xe0 != 0 {
                        return Err(CompanyLogoError::Malformed);
                    }
                    let bits = u32::from_le_bytes(data[1..5].try_into().unwrap());
                    ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
                };
                if canvas.is_some_and(|canvas| canvas != dimensions) {
                    return Err(CompanyLogoError::Malformed);
                }
                image = Some(dimensions);
            }
            _ if offset == 12 => return Err(CompanyLogoError::Malformed),
            _ => {}
        }
        offset = padded_end;
    }
    image.ok_or(CompanyLogoError::Malformed)
}

fn little_u24(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::STANDARD};

    use super::*;

    // Real one-pixel PNG, baseline/progressive JPEG and lossy/lossless WebP from ImageMagick.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVQI12P4DwQACfsD/WMmxY8AAAAASUVORK5CYII=";
    const JPEG: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAMCAgICAgMCAgIDAwMDBAYEBAQEBAgGBgUGCQgKCgkICQkKDA8MCgsOCwkJDRENDg8QEBEQCgwSExIQEw8QEBD/wAALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AVN//2Q==";
    const PROGRESSIVE_JPEG: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAMCAgICAgMCAgIDAwMDBAYEBAQEBAgGBgUGCQgKCgkICQkKDA8MCgsOCwkJDRENDg8QEBEQCgwSExIQEw8QEBD/wgALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACP/aAAgBAQAAAAFU/wD/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAEFAn//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAY/An//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAE/IX//2gAIAQEAAAAQ/wD/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAE/EH//2Q==";
    const WEBP: &str = "UklGRiQAAABXRUJQVlA4IBgAAAAwAQCdASoBAAEAAgA0JaQAA3AA/vuUAAA=";
    const LOSSLESS_WEBP: &str = "UklGRh4AAABXRUJQVlA4TBEAAAAvAAAAAAfQ//73v/+BiOh/AAA=";

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
        chunk.extend(kind);
        chunk.extend(data);
        chunk.extend(crc32(&chunk[4..]).to_be_bytes());
        chunk
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = width.to_be_bytes().to_vec();
        header.extend(height.to_be_bytes());
        header.extend([8, 6, 0, 0, 0]);
        bytes.extend(png_chunk(b"IHDR", &header));
        // A complete zlib stream with a stored deflate block: filter byte and one RGBA pixel.
        // The literal acTL is actual pixel data, not an animation chunk.
        let pixels = b"\0acTL";
        let mut compressed = vec![0x78, 0x01, 0x01, 5, 0, 0xfa, 0xff];
        compressed.extend(pixels);
        let (mut low, mut high) = (1u32, 0u32);
        for byte in pixels {
            low = (low + u32::from(*byte)) % 65521;
            high = (high + low) % 65521;
        }
        compressed.extend(((high << 16) | low).to_be_bytes());
        bytes.extend(png_chunk(b"IDAT", &compressed));
        bytes.extend(png_chunk(b"IEND", &[]));
        bytes
    }

    fn webp_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = kind.to_vec();
        chunk.extend((data.len() as u32).to_le_bytes());
        chunk.extend(data);
        if !data.len().is_multiple_of(2) {
            chunk.push(0);
        }
        chunk
    }

    fn webp(chunks: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((chunks.len() as u32 + 4).to_le_bytes());
        bytes.extend(b"WEBP");
        bytes.extend(chunks);
        bytes
    }

    #[test]
    fn accepts_real_static_rasters_by_bytes() {
        for (bytes, mime) in [
            (png(1, 1), "image/png"),
            (STANDARD.decode(PNG).unwrap(), "image/png"),
            (STANDARD.decode(JPEG).unwrap(), "image/jpeg"),
            (STANDARD.decode(PROGRESSIVE_JPEG).unwrap(), "image/jpeg"),
            (STANDARD.decode(WEBP).unwrap(), "image/webp"),
            (STANDARD.decode(LOSSLESS_WEBP).unwrap(), "image/webp"),
        ] {
            assert_eq!(
                validate_company_logo(&bytes),
                Ok(ValidatedCompanyLogo {
                    mime,
                    width: 1,
                    height: 1,
                }),
                "{mime}"
            );
        }
    }

    #[test]
    fn reads_non_square_real_jpeg_and_webp_headers() {
        for (bytes, mime) in [
            (
                include_bytes!("../tests/fixtures/company-logo-64x32.jpg").as_slice(),
                "image/jpeg",
            ),
            (
                include_bytes!("../tests/fixtures/company-logo-64x32.webp").as_slice(),
                "image/webp",
            ),
        ] {
            assert_eq!(
                validate_company_logo(bytes),
                Ok(ValidatedCompanyLogo {
                    mime,
                    width: 64,
                    height: 32,
                })
            );
        }
    }

    #[test]
    fn enforces_each_jpeg_and_webp_header_dimension() {
        for (width, height, allowed) in [
            (1u16, 1u16, true),
            (1024, 2, true),
            (2, 1024, true),
            (1025, 2, false),
            (2, 1025, false),
        ] {
            // Change dimensions in complete static containers to test header limits; the
            // compressed pixels deliberately remain unchanged and are not decoded here.
            let mut jpeg = STANDARD.decode(JPEG).unwrap();
            let frame = jpeg.windows(2).position(|b| b == [0xff, 0xc0]).unwrap();
            jpeg[frame + 5..frame + 7].copy_from_slice(&height.to_be_bytes());
            jpeg[frame + 7..frame + 9].copy_from_slice(&width.to_be_bytes());

            let mut lossy = STANDARD.decode(WEBP).unwrap();
            lossy[26..28].copy_from_slice(&width.to_le_bytes());
            lossy[28..30].copy_from_slice(&height.to_le_bytes());

            let mut lossless = STANDARD.decode(LOSSLESS_WEBP).unwrap();
            let bits = u32::from(width - 1) | (u32::from(height - 1) << 14);
            lossless[21..25].copy_from_slice(&bits.to_le_bytes());

            for bytes in [jpeg, lossy, lossless] {
                let result = validate_company_logo(&bytes);
                if allowed {
                    let logo = result.unwrap();
                    assert_eq!(
                        (logo.width, logo.height),
                        (u32::from(width), u32::from(height))
                    );
                } else {
                    assert_eq!(result, Err(CompanyLogoError::InvalidDimensions));
                }
            }
        }

        for (width, height) in [(0u16, 1u16), (1, 0)] {
            let mut jpeg = STANDARD.decode(JPEG).unwrap();
            let frame = jpeg.windows(2).position(|b| b == [0xff, 0xc0]).unwrap();
            jpeg[frame + 5..frame + 7].copy_from_slice(&height.to_be_bytes());
            jpeg[frame + 7..frame + 9].copy_from_slice(&width.to_be_bytes());
            let mut webp = STANDARD.decode(WEBP).unwrap();
            webp[26..28].copy_from_slice(&width.to_le_bytes());
            webp[28..30].copy_from_slice(&height.to_le_bytes());
            for bytes in [jpeg, webp] {
                assert_eq!(
                    validate_company_logo(&bytes),
                    Err(CompanyLogoError::InvalidDimensions)
                );
            }
        }
    }

    #[test]
    fn refuses_other_formats_even_when_named_like_rasters() {
        for bytes in [
            b"<svg xmlns='http://www.w3.org/2000/svg'/>".as_slice(),
            b"GIF89a\x01\0\x01\0\0\0\0",
            b"\0\0\0\x20ftypavif\0\0\0\0avif",
            b"not a PNG, JPEG or WebP",
        ] {
            assert_eq!(
                validate_company_logo(bytes),
                Err(CompanyLogoError::UnsupportedFormat)
            );
        }
        let mut mismatched = png(1, 1);
        mismatched[4] = 0;
        assert_eq!(
            validate_company_logo(&mismatched),
            Err(CompanyLogoError::Malformed)
        );
    }

    #[test]
    fn rejects_every_truncated_fixture_and_trailing_bytes() {
        for bytes in [
            png(1, 1),
            STANDARD.decode(JPEG).unwrap(),
            STANDARD.decode(PROGRESSIVE_JPEG).unwrap(),
            STANDARD.decode(WEBP).unwrap(),
            STANDARD.decode(LOSSLESS_WEBP).unwrap(),
        ] {
            for length in 0..bytes.len() {
                assert!(
                    validate_company_logo(&bytes[..length]).is_err(),
                    "prefix {length}"
                );
            }
            let mut trailing = bytes;
            trailing.push(0);
            assert_eq!(
                validate_company_logo(&trailing),
                Err(CompanyLogoError::Malformed)
            );
        }
    }

    #[test]
    fn bounds_file_bytes_and_both_axes() {
        assert_eq!(
            validate_company_logo(&vec![0; COMPANY_LOGO_MAX_BYTES + 1]),
            Err(CompanyLogoError::TooLarge)
        );
        for (width, height) in [(0, 1), (1, 0), (1025, 1), (1, 1025), (u32::MAX, 1)] {
            assert_eq!(
                validate_company_logo(&png(width, height)),
                Err(CompanyLogoError::InvalidDimensions)
            );
        }
        assert!(validate_company_logo(&png(1024, 1024)).is_ok());
        let original = png(1, 1);
        let mut at_limit = original[..33].to_vec();
        at_limit.extend(png_chunk(
            b"npAD",
            &vec![0; COMPANY_LOGO_MAX_BYTES - original.len() - 12],
        ));
        at_limit.extend(&original[33..]);
        assert_eq!(at_limit.len(), COMPANY_LOGO_MAX_BYTES);
        assert!(validate_company_logo(&at_limit).is_ok());
    }

    #[test]
    fn rejects_apng_chunks_but_accepts_animation_names_in_pixels() {
        let original = png(1, 1);
        assert!(original.windows(4).any(|window| window == b"acTL"));
        assert!(validate_company_logo(&original).is_ok());
        for kind in [b"acTL", b"fcTL", b"fdAT"] {
            let mut animated = original[..33].to_vec();
            animated.extend(png_chunk(kind, &[0; 8]));
            animated.extend(&original[33..]);
            assert_eq!(
                validate_company_logo(&animated),
                Err(CompanyLogoError::Animated)
            );
        }
    }

    #[test]
    fn rejects_png_bad_checksums_chunk_lengths_and_missing_image() {
        let original = png(1, 1);
        let mut checksum = original.clone();
        checksum[29] ^= 1;
        assert_eq!(
            validate_company_logo(&checksum),
            Err(CompanyLogoError::Malformed)
        );
        let mut oversized_chunk = original.clone();
        oversized_chunk[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            validate_company_logo(&oversized_chunk),
            Err(CompanyLogoError::Malformed)
        );
        let mut no_image = original[..33].to_vec();
        no_image.extend(png_chunk(b"IEND", &[]));
        assert_eq!(
            validate_company_logo(&no_image),
            Err(CompanyLogoError::Malformed)
        );
        let mut interrupted = original[..33].to_vec();
        interrupted.extend(png_chunk(b"IDAT", &[]));
        interrupted.extend(png_chunk(b"tEXt", b"note\0interrupted image"));
        interrupted.extend(&original[33..]);
        assert_eq!(
            validate_company_logo(&interrupted),
            Err(CompanyLogoError::Malformed)
        );
    }

    #[test]
    fn rejects_a_second_jpeg_frame_with_conflicting_dimensions() {
        let bytes = include_bytes!("../tests/fixtures/company-logo-second-sof.jpg");
        assert_eq!(
            validate_company_logo(bytes),
            Err(CompanyLogoError::Malformed)
        );
    }

    #[test]
    fn rejects_a_jpeg_height_redefinition_after_the_frame() {
        let mut bytes = STANDARD.decode(JPEG).unwrap();
        let scan = bytes.windows(2).position(|b| b == [0xff, 0xda]).unwrap();
        bytes.splice(scan..scan, [0xff, 0xdc, 0x00, 0x04, 0x10, 0x00]);
        assert_eq!(
            validate_company_logo(&bytes),
            Err(CompanyLogoError::Malformed)
        );
    }

    #[test]
    fn accepts_jpeg_metadata_and_progressive_scans_without_scanning_metadata() {
        let original = STANDARD.decode(PROGRESSIVE_JPEG).unwrap();
        let metadata = b"acTL ANIM ANMF \xff\xd9 not an image terminator";
        let mut bytes = original[..2].to_vec();
        bytes.extend([0xff, 0xe1]);
        bytes.extend((metadata.len() as u16 + 2).to_be_bytes());
        bytes.extend(metadata);
        bytes.extend(&original[2..]);
        assert!(validate_company_logo(&bytes).is_ok());
        bytes[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
        assert_eq!(
            validate_company_logo(&bytes),
            Err(CompanyLogoError::Malformed)
        );
    }

    #[test]
    fn rejects_webp_animation_chunks_and_the_extended_animation_flag() {
        let original = STANDARD.decode(WEBP).unwrap();
        for kind in [b"ANIM", b"ANMF"] {
            let mut chunks = original[12..].to_vec();
            chunks.extend(webp_chunk(kind, &[0; 6]));
            assert_eq!(
                validate_company_logo(&webp(&chunks)),
                Err(CompanyLogoError::Animated)
            );
        }
        let mut chunks = webp_chunk(b"VP8X", &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        chunks.extend(&original[12..]);
        assert_eq!(
            validate_company_logo(&webp(&chunks)),
            Err(CompanyLogoError::Animated)
        );
    }

    #[test]
    fn validates_extended_webp_canvas_and_ignores_names_in_chunk_payloads() {
        let original = STANDARD.decode(WEBP).unwrap();
        let mut chunks = webp_chunk(b"VP8X", &[8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        chunks.extend(&original[12..]);
        chunks.extend(webp_chunk(b"EXIF", b"ANIM ANMF acTL"));
        assert!(validate_company_logo(&webp(&chunks)).is_ok());
        chunks[12] = 1; // Canvas becomes two pixels wide; image remains one pixel wide.
        assert_eq!(
            validate_company_logo(&webp(&chunks)),
            Err(CompanyLogoError::Malformed)
        );
    }

    #[test]
    fn rejects_webp_bad_riff_size_chunk_size_padding_and_missing_image() {
        let original = STANDARD.decode(LOSSLESS_WEBP).unwrap();
        let mut bad_riff = original.clone();
        bad_riff[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            validate_company_logo(&bad_riff),
            Err(CompanyLogoError::Malformed)
        );
        let mut bad_chunk = original.clone();
        bad_chunk[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            validate_company_logo(&bad_chunk),
            Err(CompanyLogoError::Malformed)
        );
        let mut bad_padding = original;
        *bad_padding.last_mut().unwrap() = 1;
        assert_eq!(
            validate_company_logo(&bad_padding),
            Err(CompanyLogoError::Malformed)
        );
        assert_eq!(
            validate_company_logo(&webp(&webp_chunk(b"VP8X", &[0; 10]))),
            Err(CompanyLogoError::Malformed)
        );
    }
}
