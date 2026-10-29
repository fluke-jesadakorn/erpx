//! actual B0 LDAP control-value semantics, not a generic JS
//! runtime. These two predicates are checked against the unchanged Node source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidDirectoryValue {
    ImmutableId,
    Integer,
}

pub fn ad_guid(bytes: &[u8]) -> Result<String, InvalidDirectoryValue> {
    if bytes.len() != 16 {
        return Err(InvalidDirectoryValue::ImmutableId);
    }
    let order = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
    let mut output = String::with_capacity(36);
    for (index, at) in order.into_iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            output.push('-');
        }
        use std::fmt::Write;
        // Writing a byte to a String is infallible; no credential/error payload.
        let _ = write!(output, "{:02x}", bytes[at]);
    }
    Ok(output)
}

pub(crate) fn js_space(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'
            | '\u{000a}'
            | '\u{000b}'
            | '\u{000c}'
            | '\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
fn radix_digits(value: &str) -> Option<(&str, u32)> {
    if value.starts_with("0x") || value.starts_with("0X") {
        Some((&value[2..], 16))
    } else if value.starts_with("0o") || value.starts_with("0O") {
        Some((&value[2..], 8))
    } else if value.starts_with("0b") || value.starts_with("0B") {
        Some((&value[2..], 2))
    } else {
        None
    }
}
/// Only used by UAC bit2 and shadowExpire. Unsigned radix overflow (>u64) is
/// above the finite time threshold and rounds to a multiple of at least4096,
/// so Infinity has the same result for BOTH of those actual source predicates.
/// It must never be reused as a general JavaScript Number conversion.
fn control_number(value: &str) -> f64 {
    let value = value.trim_matches(js_space);
    if value.is_empty() {
        return 0.0;
    }
    if let Some((digits, radix)) = radix_digits(value) {
        if digits.is_empty()
            || digits
                .chars()
                .any(|character| character.to_digit(radix).is_none())
        {
            return f64::NAN;
        }
        return u64::from_str_radix(digits, radix).map_or(f64::INFINITY, |number| number as f64);
    }
    value.parse::<f64>().unwrap_or(f64::NAN)
}
/// Parse the B0 BigInt String grammar with saturation, without an unbounded
/// bignum allocation. Values outside i128 cannot approach a safe epoch cutoff;
/// their sign alone determines the actual FILETIME comparison below.
fn filetime_integer(value: &str) -> Result<i128, InvalidDirectoryValue> {
    let value = value.trim_matches(js_space);
    let (digits, radix, negative) = if let Some((digits, radix)) = radix_digits(value) {
        (digits, radix, false)
    } else if let Some(digits) = value.strip_prefix('-') {
        (digits, 10, true)
    } else {
        (value.strip_prefix('+').unwrap_or(value), 10, false)
    };
    if value.is_empty() {
        return Ok(0);
    }
    if digits.is_empty() {
        return Err(InvalidDirectoryValue::Integer);
    }
    let mut integer = 0i128;
    for character in digits.chars() {
        let digit = character
            .to_digit(radix)
            .ok_or(InvalidDirectoryValue::Integer)?;
        // BigInt permits ASCII numeric grammar, not Unicode decimal digits.
        if !character.is_ascii() {
            return Err(InvalidDirectoryValue::Integer);
        }
        integer = integer
            .saturating_mul(i128::from(radix))
            .saturating_add(i128::from(digit));
    }
    Ok(if negative { -integer } else { integer })
}

pub struct LdapAccountControls<'a> {
    pub user_account_control: &'a str,
    pub account_expires: &'a str,
    pub shadow_expire: &'a str,
    pub password_locked_at: &'a str,
    pub samba_account_flags: &'a str,
}
impl LdapAccountControls<'_> {
    pub fn enabled(&self, active_directory: bool, now: i64) -> Result<bool, InvalidDirectoryValue> {
        if active_directory {
            let control = control_number(self.user_account_control);
            let disabled = control.is_finite() && control.trunc().rem_euclid(4.0) >= 2.0;
            if disabled {
                // Actual JS && short-circuits malformed accountExpires here.
                return Ok(false);
            }
            let expiration = self.account_expires;
            if expiration.is_empty() || matches!(expiration, "0" | "9223372036854775807") {
                return Ok(true);
            }
            let expires = filetime_integer(expiration)? / 10_000 - 11_644_473_600_000;
            return Ok(expires > i128::from(now));
        }
        let shadow = control_number(self.shadow_expire);
        Ok(self.password_locked_at.is_empty()
            && !self.samba_account_flags.contains('D')
            && (shadow == 0.0
                || shadow.is_nan()
                || shadow < 0.0
                || shadow * 86_400_000.0 > now as f64))
    }
}

/// Alias keys retain the source profile's exact casing; only the observed
/// provider ID is lowercased using pinned Unicode17 contextual semantics.
pub fn canonical_external_id(
    aliases: &std::collections::BTreeMap<String, String>,
    provider_id: &str,
) -> String {
    aliases
        .get(&provider_id.to_lowercase())
        .cloned()
        .unwrap_or_else(|| provider_id.to_owned())
}
