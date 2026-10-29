//! Private source operands. Containers are decoded only when an operation reads them.
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Text(pub(crate) Vec<u16>);
impl Text {
    pub fn ascii(s: &str) -> Self {
        Self(s.encode_utf16().collect())
    }
    pub(crate) fn utf8(&self) -> Result<String, Fault> {
        String::from_utf16(&self.0).map_err(|_| Fault::gap("P3", "text", "document"))
    }
    pub fn json(&self) -> String {
        let mut out = String::from("\"");
        for unit in char::decode_utf16(self.0.iter().copied()) {
            match unit {
                Ok('"') => out.push_str("\\\""),
                Ok('\\') => out.push_str("\\\\"),
                Ok('\u{8}') => out.push_str("\\b"),
                Ok('\u{c}') => out.push_str("\\f"),
                Ok('\n') => out.push_str("\\n"),
                Ok('\r') => out.push_str("\\r"),
                Ok('\t') => out.push_str("\\t"),
                Ok(c) if c <= '\u{1f}' => out.push_str(&format!("\\u{:04x}", c as u32)),
                Ok(c) => out.push(c),
                Err(e) => out.push_str(&format!("\\u{:04x}", e.unpaired_surrogate())),
            }
        }
        out.push('"');
        out
    }
    pub fn upper_key(&self) -> String {
        let mut out = String::new();
        let mut gap = false;
        for v in char::decode_utf16(self.0.iter().copied()) {
            match v {
                Ok(c) => {
                    for x in c.to_uppercase() {
                        if x.is_ascii_uppercase() || x.is_ascii_digit() {
                            if gap {
                                out.push('_');
                                gap = false;
                            }
                            out.push(x);
                        } else {
                            gap = true;
                        }
                    }
                }
                Err(_) => gap = true,
            }
        }
        if gap {
            out.push('_');
        }
        out
    }
}
impl Serialize for Text {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        RawValue::from_string(self.json())
            .map_err(serde::ser::Error::custom)?
            .serialize(s)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Fault {
    Source {
        name: &'static str,
        status: Option<u16>,
        code: Option<&'static str>,
        message: String,
    },
    Gap {
        gate: &'static str,
        operation: &'static str,
        field_path: String,
    },
}
impl Fault {
    pub fn business(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self::Source {
            name: "Error",
            status: Some(status),
            code: Some(code),
            message: message.into(),
        }
    }
    pub fn native(name: &'static str, message: impl Into<String>) -> Self {
        Self::Source {
            name,
            status: None,
            code: None,
            message: message.into(),
        }
    }
    pub fn gap(gate: &'static str, operation: &'static str, path: impl Into<String>) -> Self {
        Self::Gap {
            gate,
            operation,
            field_path: path.into(),
        }
    }
}
pub(crate) type R<T> = Result<T, Fault>;
#[derive(Clone, Debug)]
pub(crate) enum Operand {
    Missing,
    Raw(Box<RawValue>),
}
impl Operand {
    pub fn raw(s: &str) -> R<Self> {
        serde_json::from_str::<Box<RawValue>>(s)
            .map(Self::Raw)
            .map_err(|_| syntax(s))
    }
    pub fn text(s: &str) -> Self {
        Self::Raw(RawValue::from_string(Text::ascii(s).json()).expect("string"))
    }
    pub fn token(&self) -> Option<&str> {
        match self {
            Self::Missing => None,
            Self::Raw(v) => Some(v.get()),
        }
    }
    pub fn nullish(&self) -> bool {
        self.token().is_none_or(|s| s == "null")
    }
    pub fn string(&self) -> Option<Text> {
        let s = self.token()?;
        if !s.starts_with('"') {
            return None;
        }
        decode_string(s)
    }
    pub fn prop(&self, key: &str) -> R<Self> {
        let Some(s) = self.token() else {
            return Err(Fault::native(
                "TypeError",
                format!("Cannot read properties of undefined (reading '{key}')"),
            ));
        };
        if s == "null" {
            return Err(Fault::native(
                "TypeError",
                format!("Cannot read properties of null (reading '{key}')"),
            ));
        }
        if !s.starts_with('{') {
            return Ok(Self::Missing);
        }
        let obj: Obj = serde_json::from_str(s).map_err(|_| Fault::gap("P1", "object", "object"))?;
        Ok(obj
            .0
            .into_iter()
            .rev()
            .find(|(k, _)| *k == Text::ascii(key))
            .map_or(Self::Missing, |(_, v)| Self::Raw(v)))
    }
    pub fn array(&self) -> R<Option<Vec<Self>>> {
        let Some(s) = self.token() else {
            return Ok(None);
        };
        if !s.starts_with('[') {
            return Ok(None);
        }
        let a: Arr = serde_json::from_str(s).map_err(|_| Fault::gap("P1", "array", "array"))?;
        Ok(Some(a.0.into_iter().map(Self::Raw).collect()))
    }
    pub fn truthy(&self) -> bool {
        match self.token() {
            None | Some("null" | "false") => false,
            Some(s) if s.starts_with('"') => self.string().is_some_and(|x| !x.0.is_empty()),
            Some(s) if s.starts_with('[') || s.starts_with('{') => true,
            Some("true") => true,
            Some(_) => {
                let n = self.number().unwrap_or(f64::NAN);
                n != 0.0 && !n.is_nan()
            }
        }
    }
    pub fn number(&self) -> R<f64> {
        match self.token() {
            None => Ok(f64::NAN),
            Some("null" | "false") => Ok(0.0),
            Some("true") => Ok(1.0),
            Some(s) if s.starts_with('"') => number_string(&self.string().expect("decoded")),
            Some(s) if s.starts_with('[') || s.starts_with('{') => {
                Err(Fault::gap("P1", "number", "operand"))
            }
            Some(s) => Ok(s.parse::<f64>().unwrap_or(f64::NAN)),
        }
    }
    pub fn actual_safe_integer(&self) -> bool {
        self.token().is_some_and(|s| {
            s.as_bytes()
                .first()
                .is_some_and(|c| c.is_ascii_digit() || *c == b'-')
        }) && self
            .number()
            .is_ok_and(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0)
    }
    pub fn strict_text(&self, s: &str) -> bool {
        self.string().is_some_and(|t| t == Text::ascii(s))
    }
}
fn number_string(t: &Text) -> R<f64> {
    let trimmed = t.0.trim_ascii_js();
    if trimmed.is_empty() {
        return Ok(0.0);
    }
    let Ok(s) = String::from_utf16(trimmed) else {
        return Ok(f64::NAN);
    };
    if s == "Infinity" || s == "+Infinity" {
        return Ok(f64::INFINITY);
    }
    if s == "-Infinity" {
        return Ok(f64::NEG_INFINITY);
    }
    if let Some((radix, digits)) = [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ]
    .into_iter()
    .find_map(|(prefix, r)| s.strip_prefix(prefix).map(|d| (r, d)))
    {
        use num_traits::ToPrimitive;
        if digits.is_empty() || !digits.bytes().all(|b| (b as char).is_digit(radix)) {
            return Ok(f64::NAN);
        }
        return Ok(num_bigint::BigInt::parse_bytes(digits.as_bytes(), radix)
            .and_then(|n| n.to_f64())
            .unwrap_or(f64::NAN));
    }
    if !decimal_grammar(&s) {
        return Ok(f64::NAN);
    }
    Ok(s.parse::<f64>().unwrap_or(f64::NAN))
}
fn decimal_grammar(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = usize::from(bytes.first().is_some_and(|b| matches!(*b, b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        digits += 1;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return false;
    }
    if bytes.get(i).is_some_and(|b| matches!(*b, b'e' | b'E')) {
        i += 1;
        if bytes.get(i).is_some_and(|b| matches!(*b, b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == bytes.len()
}
trait TrimJs {
    fn trim_ascii_js(&self) -> &[u16];
}
impl TrimJs for [u16] {
    fn trim_ascii_js(&self) -> &[u16] {
        fn ws(x: u16) -> bool {
            matches!(x,0x9..=0xd|0x20|0xa0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
        }
        let a = self.iter().position(|x| !ws(*x)).unwrap_or(self.len());
        let b = self.iter().rposition(|x| !ws(*x)).map_or(a, |x| x + 1);
        &self[a..b]
    }
}
fn decode_string(s: &str) -> Option<Text> {
    let mut it = s[1..s.len() - 1].chars();
    let mut out = Vec::new();
    while let Some(c) = it.next() {
        if c != '\\' {
            let mut b = [0u16; 2];
            out.extend_from_slice(c.encode_utf16(&mut b));
            continue;
        }
        let c = it.next()?;
        match c {
            '"' | '\\' | '/' => out.push(c as u16),
            'b' => out.push(8),
            'f' => out.push(12),
            'n' => out.push(10),
            'r' => out.push(13),
            't' => out.push(9),
            'u' => {
                let hex: String = it.by_ref().take(4).collect();
                out.push(u16::from_str_radix(&hex, 16).ok()?);
            }
            _ => return None,
        }
    }
    Some(Text(out))
}
struct Obj(Vec<(Text, Box<RawValue>)>);
struct Arr(Vec<Box<RawValue>>);
impl<'de> Deserialize<'de> for Obj {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Obj;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Obj, M::Error> {
                let mut v = Vec::new();
                while let Some((k, x)) = m.next_entry::<Box<RawValue>, Box<RawValue>>()? {
                    let key =
                        decode_string(k.get()).ok_or_else(|| serde::de::Error::custom("key"))?;
                    v.push((key, x));
                }
                Ok(Obj(v))
            }
        }
        d.deserialize_map(V)
    }
}
impl<'de> Deserialize<'de> for Arr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Arr;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("array")
            }
            fn visit_seq<M: SeqAccess<'de>>(self, mut m: M) -> Result<Arr, M::Error> {
                let mut v = Vec::new();
                while let Some(x) = m.next_element::<Box<RawValue>>()? {
                    v.push(x)
                }
                Ok(Arr(v))
            }
        }
        d.deserialize_seq(V)
    }
}
fn syntax(s: &str) -> Fault {
    let is_json_ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    let t = s.trim_start_matches(is_json_ws);
    if t.starts_with('{') && t[1..].trim_matches(is_json_ws).is_empty() {
        let position = s.encode_utf16().count();
        let line = s.bytes().filter(|x| *x == b'\n').count() + 1;
        let column = s.rsplit('\n').next().unwrap_or("").encode_utf16().count() + 1;
        return Fault::native(
            "SyntaxError",
            format!(
                "Expected property name or '}}' in JSON at position {position} (line {line} column {column})"
            ),
        );
    }
    Fault::gap("P1", "JSON.parse", "document")
}
pub(crate) fn js_print(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n == f64::INFINITY {
        return "Infinity".into();
    }
    if n == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    if n == 0.0 {
        return "0".into();
    }
    let mut b = ryu_js::Buffer::new();
    b.format(n).to_owned()
}
pub(crate) fn js_round(n: f64) -> f64 {
    if !n.is_finite() || n == 0.0 {
        return n;
    }
    let floor = n.floor();
    let v = if n - floor < 0.5 { floor } else { floor + 1.0 };
    if v == 0.0 && n.is_sign_negative() {
        -0.0
    } else {
        v
    }
}
pub(crate) fn bigint(n: f64) -> R<num_bigint::BigInt> {
    use num_traits::FromPrimitive;
    if !n.is_finite() || n.fract() != 0.0 {
        return Err(Fault::native(
            "RangeError",
            format!(
                "The number {} cannot be converted to a BigInt because it is not an integer",
                js_print(n)
            ),
        ));
    }
    num_bigint::BigInt::from_f64(n).ok_or_else(|| Fault::gap("P1", "BigInt", "number"))
}
