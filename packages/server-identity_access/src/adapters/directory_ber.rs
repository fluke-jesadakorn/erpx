//! bounded LDAP BER framing, before any protocol decoder.
//! This module is independent of the network and never formats payload bytes.

pub const MAX_BER_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_BER_DEPTH: usize = 32;
pub const MAX_BER_ELEMENTS: usize = 8_192;
pub const MAX_WIRE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_WIRE_FRAMES: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BerFailure {
    Malformed,
    Indefinite,
    TooLarge,
    TooDeep,
    TooManyElements,
    Truncated,
    Budget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BerFrameLength {
    pub header: usize,
    pub total: usize,
}

/// None means at most six header bytes are needed, never an allocation request.
/// LDAP uses a SEQUENCE and definite lengths (RFC4511 section5.1).
pub fn frame_length(prefix: &[u8]) -> Result<Option<BerFrameLength>, BerFailure> {
    if prefix.first().is_some_and(|tag| *tag != 0x30) {
        return Err(BerFailure::Malformed);
    }
    if prefix.len() < 2 {
        return Ok(None);
    }
    let Some((header, body)) = definite_length(prefix, 1)? else {
        return Ok(None);
    };
    let total = header.checked_add(body).ok_or(BerFailure::TooLarge)?;
    if total > MAX_BER_FRAME_BYTES {
        return Err(BerFailure::TooLarge);
    }
    Ok(Some(BerFrameLength { header, total }))
}

fn definite_length(input: &[u8], at: usize) -> Result<Option<(usize, usize)>, BerFailure> {
    let Some(first) = input.get(at).copied() else {
        return Ok(None);
    };
    if first < 0x80 {
        return Ok(Some((at + 1, usize::from(first))));
    }
    let count = usize::from(first & 0x7f);
    if count == 0 {
        return Err(BerFailure::Indefinite);
    }
    // Four octets already describe a value far above the owned frame ceiling.
    // No count may overflow arithmetic or induce an attacker-sized allocation.
    if count > 4 {
        return Err(BerFailure::TooLarge);
    }
    let end = at + 1 + count;
    let Some(bytes) = input.get(at + 1..end) else {
        return Ok(None);
    };
    let mut length = 0usize;
    for byte in bytes {
        length = length
            .checked_mul(256)
            .and_then(|value| value.checked_add(usize::from(*byte)))
            .ok_or(BerFailure::TooLarge)?;
    }
    Ok(Some((end, length)))
}

/// Validate every constructed TLV before decoding. Primitive attribute values
/// remain opaque: a DN/password/octet string is never interpreted as nested BER.
/// Iteration is depth/element bounded and performs no payload-sized allocation.
pub fn validate_complete_frame(input: &[u8]) -> Result<(), BerFailure> {
    let frame = frame_length(input)?.ok_or(BerFailure::Truncated)?;
    if frame.total != input.len() {
        return Err(if frame.total > input.len() {
            BerFailure::Truncated
        } else {
            BerFailure::Malformed
        });
    }
    let mut ends = [0usize; MAX_BER_DEPTH];
    ends[0] = input.len();
    let mut level = 1usize;
    let mut at = frame.header;
    let mut elements = 1usize;
    while level > 0 {
        let end = ends[level - 1];
        if at == end {
            level -= 1;
            continue;
        }
        if at > end {
            return Err(BerFailure::Malformed);
        }
        let tag = *input.get(at).ok_or(BerFailure::Truncated)?;
        if tag == 0 {
            // End-of-contents is only meaningful with forbidden indefinite BER.
            return Err(BerFailure::Malformed);
        }
        let mut length_at = at + 1;
        if tag & 0x1f == 0x1f {
            let mut tag_octets = 0usize;
            loop {
                let byte = *input.get(length_at).ok_or(BerFailure::Truncated)?;
                length_at += 1;
                tag_octets += 1;
                if tag_octets > 5 || length_at > end {
                    return Err(BerFailure::Malformed);
                }
                if byte & 0x80 == 0 {
                    break;
                }
            }
        }
        let (body_at, length) = definite_length(input, length_at)?.ok_or(BerFailure::Truncated)?;
        let body_end = body_at.checked_add(length).ok_or(BerFailure::TooLarge)?;
        if body_at > end || body_end > end {
            return Err(BerFailure::Truncated);
        }
        elements += 1;
        if elements > MAX_BER_ELEMENTS {
            return Err(BerFailure::TooManyElements);
        }
        if tag & 0x20 != 0 {
            if level == MAX_BER_DEPTH {
                return Err(BerFailure::TooDeep);
            }
            ends[level] = body_end;
            level += 1;
            at = body_at;
        } else {
            at = body_end;
        }
    }
    Ok(())
}

#[derive(Default)]
pub struct WireBudget {
    bytes: usize,
    frames: usize,
}
impl WireBudget {
    pub fn new() -> Self {
        Self {
            bytes: 0,
            frames: 0,
        }
    }
    /// Reserve a declared frame BEFORE reading or allocating its body.
    pub fn reserve(&mut self, bytes: usize) -> Result<(), BerFailure> {
        if bytes > MAX_BER_FRAME_BYTES {
            return Err(BerFailure::TooLarge);
        }
        self.bytes = self.bytes.checked_add(bytes).ok_or(BerFailure::Budget)?;
        self.frames = self.frames.checked_add(1).ok_or(BerFailure::Budget)?;
        if self.bytes > MAX_WIRE_BYTES || self.frames > MAX_WIRE_FRAMES {
            return Err(BerFailure::Budget);
        }
        Ok(())
    }
}
