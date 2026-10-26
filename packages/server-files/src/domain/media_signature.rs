use std::collections::BTreeSet;

const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const HEIC: [[u8; 4]; 6] = [*b"heic", *b"heix", *b"hevc", *b"hevx", *b"heim", *b"heis"];
const HEIF: [[u8; 4]; 2] = [*b"mif1", *b"msf1"];

/// Compatibility admission hint only. This does not establish clean scanning,
/// safe decoding, a valid PDF signature or an authorized publication.
pub fn matches_upload_signature(media_type: &str, bytes: &[u8]) -> bool {
    match media_type {
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "image/png" => bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47]),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8]),
        "image/webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        "image/heic" | "image/heif" => {
            bytes.len() >= 12
                && bytes.get(4..8) == Some(b"ftyp")
                && bytes.get(8..12).is_some_and(|brand| {
                    if media_type == "image/heic" {
                        HEIC.iter().any(|v| v == brand)
                    } else {
                        HEIF.iter().any(|v| v == brand)
                    }
                })
        }
        "text/csv" => true,
        _ => false,
    }
}

/// Exact owner byte-signature policy, independent of HTTP and native decoders.
/// A scanner/preview adapter still must inspect and sanitize accepted bytes.
pub fn detected_attachment_media_type(bytes: &[u8], declared: &str) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        let start = bytes.len().saturating_sub(4096);
        let eof = bytes[start..]
            .windows(5)
            .rposition(|value| value == b"%%EOF")?
            + start
            + 5;
        return padding_only(&bytes[eof..]).then_some("application/pdf");
    }
    if bytes.len() >= 20 && bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return bytes
            .ends_with(&[0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82])
            .then_some("image/png");
    }
    if bytes.len() >= 5 && bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        let end = bytes
            .windows(2)
            .enumerate()
            .rev()
            .find(|(index, value)| *index >= 2 && *value == [0xff, 0xd9])
            .map(|(index, _)| index + 2)?;
        return padding_only(&bytes[end..]).then_some("image/jpeg");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return (u32_le(bytes, 4)? as u64 + 8 == bytes.len() as u64).then_some("image/webp");
    }
    if let Some(media) = detected_heif(bytes) {
        return Some(media);
    }
    if declared == DOCX && bytes.len() >= 22 && bytes.starts_with(b"PK") {
        let minimum = bytes.len().saturating_sub(65557);
        let eocd = (minimum..=bytes.len() - 22)
            .rev()
            .find(|offset| u32_le(bytes, *offset) == Some(0x06054b50))?;
        let central_size = u32_le(bytes, eocd + 12)? as usize;
        let mut offset = u32_le(bytes, eocd + 16)? as usize;
        let end = offset.checked_add(central_size)?;
        let mut names = BTreeSet::new();
        while offset.checked_add(46)? <= end
            && end <= bytes.len()
            && u32_le(bytes, offset) == Some(0x02014b50)
        {
            let name_len = u16_le(bytes, offset + 28)? as usize;
            let extra_len = u16_le(bytes, offset + 30)? as usize;
            let comment_len = u16_le(bytes, offset + 32)? as usize;
            let name_end = offset.checked_add(46)?.checked_add(name_len)?;
            if name_end > end {
                break;
            }
            names.insert(&bytes[offset + 46..name_end]);
            offset = name_end.checked_add(extra_len)?.checked_add(comment_len)?;
        }
        return (names.contains(b"[Content_Types].xml".as_slice())
            && names.contains(b"word/document.xml".as_slice()))
        .then_some(DOCX);
    }
    if declared == "text/csv" && !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok() {
        return Some("text/csv");
    }
    None
}

fn detected_heif(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 16 {
        return None;
    }
    let mut offset = 0usize;
    let mut brands = BTreeSet::new();
    let mut meta = false;
    while offset < bytes.len() {
        let size32 = u32_be(bytes, offset)?;
        let kind = bytes.get(offset.checked_add(4)?..offset.checked_add(8)?)?;
        let (header, size) = if size32 == 1 {
            let extended = u64::from_be_bytes(bytes.get(offset + 8..offset + 16)?.try_into().ok()?);
            if extended > 9_007_199_254_740_991 {
                return None;
            }
            (16, usize::try_from(extended).ok()?)
        } else {
            (
                8,
                if size32 == 0 {
                    bytes.len() - offset
                } else {
                    size32 as usize
                },
            )
        };
        let end = offset.checked_add(size)?;
        if size < header || end > bytes.len() {
            return None;
        }
        if offset == 0 {
            if kind != b"ftyp" || size < header + 8 {
                return None;
            }
            brands.insert(<[u8; 4]>::try_from(bytes.get(header..header + 4)?).ok()?);
            let mut at = header + 8;
            while at + 4 <= end {
                brands.insert(<[u8; 4]>::try_from(bytes.get(at..at + 4)?).ok()?);
                at += 4;
            }
        }
        if kind == b"meta" {
            meta = true;
        }
        offset = end;
    }
    if !meta {
        return None;
    }
    if HEIC.iter().any(|brand| brands.contains(brand)) {
        Some("image/heic")
    } else if HEIF.iter().any(|brand| brands.contains(brand)) {
        Some("image/heif")
    } else {
        None
    }
}
fn padding_only(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| [0, 9, 10, 12, 13, 32].contains(b))
}
fn u16_le(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn u32_be(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}
