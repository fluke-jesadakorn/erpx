//! Pure OIDC URL wire values; endpoint approval and authentication belong to callers.
use url::Url;

/// Implements URLSearchParams.set in BetterAuth's configured call order.
pub fn set_authorization_parameters(
    mut url: Url,
    client: &str,
    redirect: &str,
    state: &str,
    nonce: &str,
    challenge: &str,
) -> Url {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    for (name, value) in [
        ("response_type", "code"),
        ("client_id", client),
        ("state", state),
        ("scope", "openid profile email"),
        ("redirect_uri", redirect),
        ("nonce", nonce),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge),
    ] {
        if let Some(first) = pairs.iter().position(|(k, _)| k == name) {
            pairs[first].1 = value.to_owned();
            let mut index = first + 1;
            while index < pairs.len() {
                if pairs[index].0 == name {
                    pairs.remove(index);
                } else {
                    index += 1;
                }
            }
        } else {
            pairs.push((name.to_owned(), value.to_owned()));
        }
    }
    url.query_pairs_mut().clear().extend_pairs(pairs);
    url
}

/// JavaScript encodeURIComponent for an already converted, valid string.
pub fn encode_uri_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[(byte >> 4) as usize]));
            out.push(char::from(HEX[(byte & 15) as usize]));
        }
    }
    out
}

/// Unlike URLSearchParams, encodeURIComponent rejects malformed UTF-16.
pub fn encode_uri_component_utf16(value: &[u16]) -> Result<String, std::string::FromUtf16Error> {
    String::from_utf16(value).map(|s| encode_uri_component(&s))
}
