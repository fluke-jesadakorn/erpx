//! Pure types, validation and invariants for the native lane 33 release
//! child-image read family. No SQL and no transport here.
//!
//! The TypeScript authority was the `jsonb_build_object` image in
//! `adapters/company-release-attachment-read-sources.ts`: one acceptance row
//! plus the reviews/events/revision child aggregates of a release.

pub const MAX_SLUG: usize = 128;
pub const MAX_ID: usize = 200;
pub const MAX_COMPANY: usize = 200;
pub const SQL_SHA256: &str = "0ebb5e0ec16152ee113377a86f89d1b62045c5a9a985f8e8fc66bbb7de561f81";

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SLUG
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_company(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_COMPANY
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// The image is the exact `::text` rendering of the retired `jsonb_build_object`
/// statement. It is transported as an opaque bounded string: the owner builds
/// it, TypeScript never re-projects or patches it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseChildImage {
    pub image: String,
}

impl ReleaseChildImage {
    pub fn new(image: String) -> Result<Self, ()> {
        if image.is_empty() || image.len() > 1_048_576 || !image.starts_with('{') {
            return Err(());
        }
        Ok(Self { image })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_bounded() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("a-b_c"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("ACME"));
        assert!(valid_id("release:001"));
        assert!(!valid_id(""));
        assert!(!valid_id("x'. DROP"));
        assert!(valid_company("company-001"));
        assert!(!valid_company("A'. DROP"));
    }

    #[test]
    fn image_is_a_bounded_json_object() {
        assert!(ReleaseChildImage::new("{\"acceptance\":{}}".into()).is_ok());
        assert!(ReleaseChildImage::new("".into()).is_err());
        assert!(ReleaseChildImage::new("[]".into()).is_err());
        assert!(ReleaseChildImage::new("x".repeat(1_048_577)).is_err());
    }
}
