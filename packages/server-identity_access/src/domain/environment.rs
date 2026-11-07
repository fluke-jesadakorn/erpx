//! Account environment names are configuration labels, never authorization proof.
//! Exact origins and audiences still bind every session to a configured environment.

/// Accept the existing development/fixture labels and a production label without
/// letting arbitrary control characters or unbounded values reach audiences.
pub(crate) fn valid_environment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
