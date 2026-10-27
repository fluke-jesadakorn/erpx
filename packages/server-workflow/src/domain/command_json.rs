//! Bounded JSON value codec for existing ECMAScript receipt identities.
//! JavaScript strings retain escaped lone UTF-16 units; UTF-8 domain strings do
//! not acquire them. Object enumeration and numeric printing follow JSON.stringify.
use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use std::collections::HashMap;
const LIMIT: usize = 2 * 1024 * 1024;
const DEPTH: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum JsonString {
    Unicode(String),
    Utf16(Vec<u16>),
}
impl JsonString {
    pub(crate) fn from_units(units: Vec<u16>) -> Self {
        match String::from_utf16(&units) {
            Ok(s) => Self::Unicode(s),
            Err(_) => Self::Utf16(units),
        }
    }
    pub(crate) fn units(&self) -> Vec<u16> {
        match self {
            Self::Unicode(s) => s.encode_utf16().collect(),
            Self::Utf16(units) => units.clone(),
        }
    }
    pub(crate) fn as_str(&self) -> Option<&str> {
        if let Self::Unicode(s) = self {
            Some(s)
        } else {
            None
        }
    }
    fn write(&self, out: &mut String) -> Result<(), BusinessError> {
        match self {
            Self::Unicode(s) => out.push_str(&serde_json::to_string(s).map_err(|_| invalid())?),
            Self::Utf16(units) => {
                use std::fmt::Write;
                out.push('"');
                for unit in char::decode_utf16(units.iter().copied()) {
                    match unit {
                        Ok(c) => {
                            let encoded =
                                serde_json::to_string(&c.to_string()).map_err(|_| invalid())?;
                            out.push_str(&encoded[1..encoded.len() - 1]);
                        }
                        Err(e) => write!(out, "\\u{:04x}", e.unpaired_surrogate())
                            .map_err(|_| invalid())?,
                    }
                }
                out.push('"');
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OrderedJson {
    Null,
    Bool(bool),
    Number(f64),
    String(JsonString),
    Array(Vec<Self>),
    Object(Vec<(JsonString, Self)>),
}
fn array_index(key: &JsonString) -> Option<u32> {
    let key = key.as_str()?;
    let n = key.parse::<u32>().ok()?;
    (n < u32::MAX && n.to_string() == key).then_some(n)
}
impl OrderedJson {
    pub(crate) fn parse(value: &str) -> Result<Self, BusinessError> {
        if value.len() > LIMIT {
            return Err(invalid());
        }
        let mut parser = Parser {
            source: value,
            at: 0,
        };
        let result = parser.value(0)?;
        parser.whitespace();
        if parser.at != value.len() {
            return Err(invalid());
        }
        Ok(result)
    }
    pub(crate) fn encode(&self) -> Result<String, BusinessError> {
        let mut out = String::new();
        self.write(&mut out)?;
        Ok(out)
    }
    fn write(&self, out: &mut String) -> Result<(), BusinessError> {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
            Self::Number(v) => {
                if !v.is_finite() {
                    out.push_str("null");
                } else if *v == 0.0 {
                    out.push('0');
                } else {
                    let mut buffer = ryu_js::Buffer::new();
                    out.push_str(buffer.format_finite(*v));
                }
            }
            Self::String(v) => v.write(out)?,
            Self::Array(values) => {
                out.push('[');
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out)?;
                }
                out.push(']');
            }
            Self::Object(values) => {
                let mut indexed: Vec<_> = values
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (key, _))| array_index(key).map(|n| (n, i)))
                    .collect();
                indexed.sort_unstable_by_key(|(n, _)| *n);
                let ordered = indexed.into_iter().map(|(_, i)| i).chain(
                    values
                        .iter()
                        .enumerate()
                        .filter(|(_, (key, _))| array_index(key).is_none())
                        .map(|(i, _)| i),
                );
                out.push('{');
                for (position, i) in ordered.enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    values[i].0.write(out)?;
                    out.push(':');
                    values[i].1.write(out)?;
                }
                out.push('}');
            }
        }
        if out.len() > LIMIT {
            return Err(invalid());
        }
        Ok(())
    }
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        if let Self::Object(values) = self {
            values
                .iter()
                .find(|(k, _)| k.as_str() == Some(key))
                .map(|(_, v)| v)
        } else {
            None
        }
    }
    pub(crate) fn string(&self) -> Option<&str> {
        if let Self::String(v) = self {
            v.as_str()
        } else {
            None
        }
    }
    pub(crate) fn text(value: impl Into<String>) -> Self {
        Self::String(JsonString::Unicode(value.into()))
    }
    pub(crate) fn object(values: Vec<(String, Self)>) -> Self {
        Self::Object(
            values
                .into_iter()
                .map(|(k, v)| (JsonString::Unicode(k), v))
                .collect(),
        )
    }
}
struct Parser<'a> {
    source: &'a str,
    at: usize,
}
impl Parser<'_> {
    fn whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.at).copied()
    }
    fn take(&mut self, b: u8) -> Result<(), BusinessError> {
        if self.peek() != Some(b) {
            return Err(invalid());
        }
        self.at += 1;
        Ok(())
    }
    fn value(&mut self, depth: usize) -> Result<OrderedJson, BusinessError> {
        if depth > DEPTH {
            return Err(invalid());
        }
        self.whitespace();
        match self.peek() {
            Some(b'n') => {
                self.literal("null")?;
                Ok(OrderedJson::Null)
            }
            Some(b't') => {
                self.literal("true")?;
                Ok(OrderedJson::Bool(true))
            }
            Some(b'f') => {
                self.literal("false")?;
                Ok(OrderedJson::Bool(false))
            }
            Some(b'"') => Ok(OrderedJson::String(self.string()?)),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'[') => {
                self.at += 1;
                self.whitespace();
                let mut values = Vec::new();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(OrderedJson::Array(values));
                }
                loop {
                    values.push(self.value(depth + 1)?);
                    self.whitespace();
                    if self.peek() == Some(b']') {
                        self.at += 1;
                        break;
                    }
                    self.take(b',')?;
                }
                Ok(OrderedJson::Array(values))
            }
            Some(b'{') => {
                self.at += 1;
                self.whitespace();
                let mut values = Vec::new();
                let mut positions = HashMap::<JsonString, usize>::new();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Ok(OrderedJson::Object(values));
                }
                loop {
                    self.whitespace();
                    let key = self.string()?;
                    self.whitespace();
                    self.take(b':')?;
                    let value = self.value(depth + 1)?;
                    if let Some(index) = positions.get(&key) {
                        values[*index] = (key, value);
                    } else {
                        positions.insert(key.clone(), values.len());
                        values.push((key, value));
                    }
                    self.whitespace();
                    if self.peek() == Some(b'}') {
                        self.at += 1;
                        break;
                    }
                    self.take(b',')?;
                }
                Ok(OrderedJson::Object(values))
            }
            _ => Err(invalid()),
        }
    }
    fn literal(&mut self, value: &str) -> Result<(), BusinessError> {
        if !self.source[self.at..].starts_with(value) {
            return Err(invalid());
        }
        self.at += value.len();
        Ok(())
    }
    fn number(&mut self) -> Result<OrderedJson, BusinessError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.at += 1;
                while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.at += 1;
                }
            }
            _ => return Err(invalid()),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            self.digits()?;
        }
        if self.peek().is_some_and(|b| matches!(b, b'e' | b'E')) {
            self.at += 1;
            if self.peek().is_some_and(|b| matches!(b, b'+' | b'-')) {
                self.at += 1;
            }
            self.digits()?;
        }
        let value = self.source[start..self.at]
            .parse::<f64>()
            .map_err(|_| invalid())?;
        Ok(OrderedJson::Number(value))
    }
    fn digits(&mut self) -> Result<(), BusinessError> {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        if start == self.at {
            return Err(invalid());
        }
        Ok(())
    }
    fn string(&mut self) -> Result<JsonString, BusinessError> {
        self.take(b'"')?;
        let mut units = Vec::new();
        loop {
            match self.peek().ok_or_else(invalid)? {
                b'"' => {
                    self.at += 1;
                    return Ok(JsonString::from_units(units));
                }
                b'\\' => {
                    self.at += 1;
                    let escaped = self.peek().ok_or_else(invalid)?;
                    self.at += 1;
                    match escaped {
                        b'"' | b'\\' | b'/' => units.push(u16::from(escaped)),
                        b'b' => units.push(8),
                        b'f' => units.push(12),
                        b'n' => units.push(10),
                        b'r' => units.push(13),
                        b't' => units.push(9),
                        b'u' => {
                            let end = self.at.checked_add(4).ok_or_else(invalid)?;
                            let text = self.source.get(self.at..end).ok_or_else(invalid)?;
                            if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
                                return Err(invalid());
                            }
                            units.push(u16::from_str_radix(text, 16).map_err(|_| invalid())?);
                            self.at = end;
                        }
                        _ => return Err(invalid()),
                    }
                }
                0..=31 => return Err(invalid()),
                _ => {
                    let c = self.source[self.at..].chars().next().ok_or_else(invalid)?;
                    self.at += c.len_utf8();
                    let mut pair = [0; 2];
                    units.extend_from_slice(c.encode_utf16(&mut pair));
                }
            }
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandJson(String);
impl CommandJson {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        Ok(Self(OrderedJson::parse(value)?.encode()?))
    }
    /// Preserve typed owner DTO property order; an intermediate unordered map
    /// loses the legacy request identity before this codec can retain it.
    pub fn from_serializable<T: Serialize>(value: &T) -> Result<Self, BusinessError> {
        Self::parse(&serde_json::to_string(value).map_err(|_| invalid())?)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// Preserve a property's ordered JSON without an unordered-map conversion.
    pub fn property(&self, name: &str) -> Result<Option<Self>, BusinessError> {
        self.value()?
            .get(name)
            .cloned()
            .map(Self::from_value)
            .transpose()
    }
    pub(crate) fn value(&self) -> Result<OrderedJson, BusinessError> {
        OrderedJson::parse(&self.0)
    }
    pub(crate) fn from_value(value: OrderedJson) -> Result<Self, BusinessError> {
        Ok(Self(value.encode()?))
    }
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_JSON_INVALID",
        "Provide a bounded JSON command value.",
    )
}
