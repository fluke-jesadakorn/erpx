use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PREVIEW_INPUT_LIMIT: usize = 10 * 1024 * 1024;
pub const PREVIEW_OUTPUT_LIMIT: usize = 32 * 1024 * 1024;
pub const PREVIEW_PIXEL_LIMIT: u64 = 40_000_000;
pub const PREVIEW_EDGE: u32 = 2400;
pub const PREVIEW_DEADLINE_MILLIS: u64 = 40_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewSourceMedia {
    Png,
    Jpeg,
    Webp,
    Heic,
    Heif,
    Pdf,
    Csv,
    Docx,
}
impl PreviewSourceMedia {
    pub fn as_mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Heic => "image/heic",
            Self::Heif => "image/heif",
            Self::Pdf => "application/pdf",
            Self::Csv => "text/csv",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        }
    }
    pub fn needs_preview(self) -> bool {
        !matches!(self, Self::Csv | Self::Docx)
    }
}
impl TryFrom<&str> for PreviewSourceMedia {
    type Error = PreviewFailure;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        [
            Self::Png,
            Self::Jpeg,
            Self::Webp,
            Self::Heic,
            Self::Heif,
            Self::Pdf,
            Self::Csv,
            Self::Docx,
        ]
        .into_iter()
        .find(|v| v.as_mime() == value)
        .ok_or(PreviewFailure::SourceMedia)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewFailure {
    SourceSize,
    SourceChecksum,
    SourceMedia,
    PixelLimit,
    DecoderRejected,
    DecodedSize,
    OutputSize,
    OutputFormat,
    OutputMetadata,
    ToolConfiguration,
    WorkerUnavailable,
    WorkerCancelled,
    Deadline,
    WorkerMemory,
    WorkerOutput,
    WorkerCleanup,
    WorkerProtocol,
    ResourceLimit,
}
/// Admitted computation data only: neither a scan verdict nor owner authority.
pub struct PreviewInput {
    bytes: Vec<u8>,
    media: PreviewSourceMedia,
    checksum: String,
}
impl PreviewInput {
    pub fn new(
        bytes: Vec<u8>,
        media: PreviewSourceMedia,
        checksum: &str,
    ) -> Result<Self, PreviewFailure> {
        if bytes.is_empty() || bytes.len() > PREVIEW_INPUT_LIMIT {
            return Err(PreviewFailure::SourceSize);
        }
        if checksum.len() != 64
            || !checksum
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            || hex::encode(Sha256::digest(&bytes)) != checksum
        {
            return Err(PreviewFailure::SourceChecksum);
        }
        if crate::domain::media_signature::detected_attachment_media_type(&bytes, media.as_mime())
            != Some(media.as_mime())
        {
            return Err(PreviewFailure::SourceMedia);
        }
        Ok(Self {
            bytes,
            media,
            checksum: checksum.to_owned(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn media(&self) -> PreviewSourceMedia {
        self.media
    }
    pub fn checksum_sha256(&self) -> &str {
        &self.checksum
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewComputeBudget(u64);
impl PreviewComputeBudget {
    pub fn new(millis: u64) -> Result<Self, PreviewFailure> {
        if millis == 0 || millis > PREVIEW_DEADLINE_MILLIS {
            return Err(PreviewFailure::Deadline);
        }
        Ok(Self(millis))
    }
    pub fn millis(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewMedia {
    Webp,
    Jpeg,
}
impl PreviewMedia {
    pub fn as_mime(self) -> &'static str {
        match self {
            Self::Webp => "image/webp",
            Self::Jpeg => "image/jpeg",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Webp => "webp",
            Self::Jpeg => "jpg",
        }
    }
}
pub struct PreviewArtifact {
    bytes: Vec<u8>,
    media: PreviewMedia,
    width: u32,
    height: u32,
    checksum: String,
}
impl PreviewArtifact {
    /// Validate the fixed output container independently of decoder metadata.
    pub fn from_rendered(bytes: Vec<u8>, media: PreviewMedia) -> Result<Self, PreviewFailure> {
        if bytes.is_empty() || bytes.len() > PREVIEW_OUTPUT_LIMIT {
            return Err(PreviewFailure::OutputSize);
        }
        let (width, height) = match media {
            PreviewMedia::Webp => webp_dimensions(&bytes)?,
            PreviewMedia::Jpeg => jpeg_dimensions(&bytes)?,
        };
        if width == 0 || height == 0 || width > PREVIEW_EDGE || height > PREVIEW_EDGE {
            return Err(PreviewFailure::OutputFormat);
        }
        let checksum = hex::encode(Sha256::digest(&bytes));
        Ok(Self {
            bytes,
            media,
            width,
            height,
            checksum,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn media(&self) -> PreviewMedia {
        self.media
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn checksum_sha256(&self) -> &str {
        &self.checksum
    }
}
pub enum PreviewResult {
    NoPreview,
    Rendered(PreviewArtifact),
}
impl PreviewResult {
    pub fn require_source_kind(&self, media: PreviewSourceMedia) -> Result<(), PreviewFailure> {
        match (self, media) {
            (Self::NoPreview, PreviewSourceMedia::Csv | PreviewSourceMedia::Docx) => Ok(()),
            (Self::Rendered(v), PreviewSourceMedia::Pdf) if v.media == PreviewMedia::Jpeg => Ok(()),
            (
                Self::Rendered(v),
                PreviewSourceMedia::Png
                | PreviewSourceMedia::Jpeg
                | PreviewSourceMedia::Webp
                | PreviewSourceMedia::Heic
                | PreviewSourceMedia::Heif,
            ) if v.media == PreviewMedia::Webp => Ok(()),
            _ => Err(PreviewFailure::WorkerProtocol),
        }
    }
}
fn webp_dimensions(b: &[u8]) -> Result<(u32, u32), PreviewFailure> {
    let bad = PreviewFailure::OutputFormat;
    if b.len() < 20
        || &b[..4] != b"RIFF"
        || &b[8..12] != b"WEBP"
        || le32(b, 4)?.checked_add(8) != u32::try_from(b.len()).ok()
    {
        return Err(bad);
    }
    let mut offset = 12;
    let mut canvas = None;
    let mut image = None;
    while offset < b.len() {
        let header = b.get(offset..offset + 8).ok_or(bad)?;
        let len = le32(header, 4)? as usize;
        let body = b.get(offset + 8..offset + 8 + len).ok_or(bad)?;
        match &header[..4] {
            b"EXIF" | b"XMP " | b"ICCP" => return Err(PreviewFailure::OutputMetadata),
            b"VP8X" => {
                if canvas.is_some() || body.len() != 10 || body[0] & !0x10 != 0 {
                    return Err(bad);
                }
                canvas = Some((le24(body, 4)? + 1, le24(body, 7)? + 1));
            }
            b"VP8 " => {
                if image.is_some()
                    || body.len() < 10
                    || body[0] & 1 != 0
                    || &body[3..6] != b"\x9d\x01\x2a"
                {
                    return Err(bad);
                }
                image = Some((
                    u16::from_le_bytes([body[6], body[7]]) as u32 & 0x3fff,
                    u16::from_le_bytes([body[8], body[9]]) as u32 & 0x3fff,
                ));
            }
            b"VP8L" => {
                if image.is_some() || body.len() < 5 || body[0] != 0x2f {
                    return Err(bad);
                }
                let bits = le32(body, 1)?;
                if bits >> 29 != 0 {
                    return Err(bad);
                }
                image = Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1));
            }
            b"ALPH" => {}
            _ => return Err(bad),
        }
        offset = offset
            .checked_add(8)
            .and_then(|v| v.checked_add(len))
            .and_then(|v| v.checked_add(len % 2))
            .ok_or(bad)?;
    }
    let image = image.ok_or(bad)?;
    if offset != b.len() || canvas.is_some_and(|v| v != image) {
        return Err(bad);
    }
    Ok(image)
}
fn le32(b: &[u8], o: usize) -> Result<u32, PreviewFailure> {
    Ok(u32::from_le_bytes(
        b.get(o..o + 4)
            .ok_or(PreviewFailure::OutputFormat)?
            .try_into()
            .map_err(|_| PreviewFailure::OutputFormat)?,
    ))
}
fn le24(b: &[u8], o: usize) -> Result<u32, PreviewFailure> {
    let v = b.get(o..o + 3).ok_or(PreviewFailure::OutputFormat)?;
    Ok(v[0] as u32 | ((v[1] as u32) << 8) | ((v[2] as u32) << 16))
}
fn jpeg_dimensions(b: &[u8]) -> Result<(u32, u32), PreviewFailure> {
    let bad = PreviewFailure::OutputFormat;
    if !b.starts_with(&[0xff, 0xd8]) || !b.ends_with(&[0xff, 0xd9]) {
        return Err(bad);
    }
    let mut offset = 2;
    let mut dimensions = None;
    let mut entropy = false;
    while offset < b.len() {
        if entropy {
            while b.get(offset).is_some_and(|v| *v != 0xff) {
                offset += 1;
            }
            if b.get(offset + 1) == Some(&0) {
                offset += 2;
                continue;
            }
            if b.get(offset + 1).is_some_and(|v| (0xd0..=0xd7).contains(v)) {
                offset += 2;
                continue;
            }
        }
        if b.get(offset) != Some(&0xff) {
            return Err(bad);
        }
        while b.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *b.get(offset).ok_or(bad)?;
        offset += 1;
        if marker == 0xd9 {
            return if offset == b.len() {
                dimensions.ok_or(bad)
            } else {
                Err(bad)
            };
        }
        if matches!(marker, 0xe1 | 0xe2 | 0xed | 0xfe) {
            return Err(PreviewFailure::OutputMetadata);
        }
        if marker == 0xd8 || marker == 0 || marker == 1 || (0xd0..=0xd7).contains(&marker) {
            return Err(bad);
        }
        let length = b.get(offset..offset + 2).ok_or(bad)?;
        let len = u16::from_be_bytes([length[0], length[1]]) as usize;
        if len < 2 {
            return Err(bad);
        }
        let segment = b.get(offset + 2..offset + len).ok_or(bad)?;
        if (0xe0..=0xef).contains(&marker) {
            let standard_jfif = marker == 0xe0
                && segment.len() == 14
                && segment.starts_with(b"JFIF\0")
                && segment[12..] == [0, 0];
            let standard_adobe = marker == 0xee
                && segment.len() == 12
                && segment.starts_with(b"Adobe")
                && segment[11] <= 2;
            if !standard_jfif && !standard_adobe {
                return Err(PreviewFailure::OutputMetadata);
            }
        }
        if matches!(marker,0xc0..=0xc3|0xc5..=0xc7|0xc9..=0xcb|0xcd..=0xcf) {
            if dimensions.is_some()
                || segment.len() < 6
                || segment[0] != 8
                || !(1..=4).contains(&segment[5])
                || segment.len() != 6 + segment[5] as usize * 3
            {
                return Err(bad);
            }
            dimensions = Some((
                u16::from_be_bytes([segment[3], segment[4]]) as u32,
                u16::from_be_bytes([segment[1], segment[2]]) as u32,
            ));
        }
        if marker == 0xda {
            if dimensions.is_none()
                || segment.len() < 6
                || !(1..=4).contains(&segment[0])
                || segment.len() != 4 + segment[0] as usize * 2
            {
                return Err(bad);
            }
            entropy = true;
        } else {
            entropy = false;
        }
        offset = offset.checked_add(len).ok_or(bad)?;
    }
    Err(bad)
}
