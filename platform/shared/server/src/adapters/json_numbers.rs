//! JSON structural integers follow finite JavaScript number semantics without string coercion.
use serde::{
    Deserialize, Deserializer,
    de::{self, Visitor},
};
use std::fmt;

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
struct IntegerVisitor {
    minimum: i64,
    maximum: i64,
}
impl IntegerVisitor {
    fn signed<E: de::Error>(&self, value: i64) -> Result<i64, E> {
        if (self.minimum..=self.maximum).contains(&value) {
            Ok(value)
        } else {
            Err(E::invalid_value(de::Unexpected::Signed(value), self))
        }
    }
}
impl<'de> Visitor<'de> for IntegerVisitor {
    type Value = i64;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a finite integral JSON number from {} to {}",
            self.minimum, self.maximum
        )
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<i64, E> {
        self.signed(value)
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<i64, E> {
        i64::try_from(value)
            .map_err(|_| E::invalid_value(de::Unexpected::Unsigned(value), &self))
            .and_then(|v| self.signed(v))
    }
    fn visit_i128<E: de::Error>(self, value: i128) -> Result<i64, E> {
        i64::try_from(value)
            .map_err(|_| E::custom("integer exceeds safe JSON range"))
            .and_then(|v| self.signed(v))
    }
    fn visit_u128<E: de::Error>(self, value: u128) -> Result<i64, E> {
        i64::try_from(value)
            .map_err(|_| E::custom("integer exceeds safe JSON range"))
            .and_then(|v| self.signed(v))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<i64, E> {
        if value.is_finite()
            && value.fract() == 0.0
            && value >= self.minimum as f64
            && value <= self.maximum as f64
        {
            // Every accepted integer is within the exact IEEE-754 integer range and i64.
            Ok(value as i64)
        } else {
            Err(E::invalid_value(de::Unexpected::Float(value), &self))
        }
    }
}
pub fn deserialize_u32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let value = deserializer.deserialize_any(IntegerVisitor {
        minimum: 0,
        maximum: i64::from(u32::MAX),
    })?;
    u32::try_from(value).map_err(de::Error::custom)
}
pub fn deserialize_safe_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    deserializer.deserialize_any(IntegerVisitor {
        minimum: -MAX_SAFE_INTEGER,
        maximum: MAX_SAFE_INTEGER,
    })
}
pub fn deserialize_safe_u64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = deserializer.deserialize_any(IntegerVisitor {
        minimum: 0,
        maximum: MAX_SAFE_INTEGER,
    })?;
    u64::try_from(value).map_err(de::Error::custom)
}
struct OptionalU32(u32);
impl<'de> Deserialize<'de> for OptionalU32 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_u32(deserializer).map(Self)
    }
}
struct OptionalI64(i64);
impl<'de> Deserialize<'de> for OptionalI64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_safe_i64(deserializer).map(Self)
    }
}
struct OptionalU64(u64);
impl<'de> Deserialize<'de> for OptionalU64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_safe_u64(deserializer).map(Self)
    }
}
/// Use with serde(default,deserialize_with=...) when omission is also allowed.
pub fn deserialize_optional_u32<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    Option::<OptionalU32>::deserialize(deserializer).map(|v| v.map(|v| v.0))
}
pub fn deserialize_optional_safe_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    Option::<OptionalI64>::deserialize(deserializer).map(|v| v.map(|v| v.0))
}
pub fn deserialize_optional_safe_u64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    Option::<OptionalU64>::deserialize(deserializer).map(|v| v.map(|v| v.0))
}
