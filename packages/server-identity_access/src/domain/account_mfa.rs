use erpx_server_shared::public::{BusinessError, FailureKind};

pub const MFA_FRESH_MS: i64 = 300_000;
pub const MFA_STEP_MS: i64 = 30_000;

pub fn mfa_error(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
pub fn recent_authentication(authenticated_at: i64, now: i64) -> Result<(), BusinessError> {
    if authenticated_at < 0 || authenticated_at > now || authenticated_at < now - MFA_FRESH_MS {
        return Err(mfa_error(
            "REAUTHENTICATION_REQUIRED",
            "Sign in again before enrolling an authenticator.",
        ));
    }
    Ok(())
}
pub fn valid_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|value| value.is_ascii_digit())
}
pub fn candidate_counters(now: i64) -> [i64; 3] {
    let counter = now / MFA_STEP_MS;
    [counter - 1, counter, counter + 1]
}
pub fn fresh_until(
    enabled: bool,
    verified_at: i64,
    now: i64,
    expires_at: i64,
) -> Result<i64, BusinessError> {
    if !enabled
        || verified_at < 0
        || verified_at > now
        || verified_at <= now - MFA_FRESH_MS
        || expires_at <= now
    {
        return Err(mfa_error(
            "FRESH_MFA_REQUIRED",
            "Verify your authenticator before this sensitive change.",
        ));
    }
    Ok((verified_at + MFA_FRESH_MS).min(expires_at))
}
pub fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut result = String::new();
    let (mut bits, mut value) = (0u32, 0u32);
    for byte in bytes {
        value = (value << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            result.push(ALPHABET[((value >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        result.push(ALPHABET[((value << (5 - bits)) & 31) as usize] as char);
    }
    result
}
pub fn enrollment_uri(user: &str, secret: &str) -> String {
    let mut escaped = String::new();
    for byte in user.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            escaped.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(&mut escaped, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    format!(
        "otpauth://totp/ERPX:{escaped}?secret={secret}&issuer=ERPX&algorithm=SHA1&digits=6&period=30"
    )
}

pub struct MfaEnrollment {
    pub secret: String,
    pub uri: String,
}
pub struct MfaVerified {
    pub verified_until: i64,
}
