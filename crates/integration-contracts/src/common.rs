use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

pub const MAX_REFERENCE_CODE_BYTES: usize = 128;
pub const MAX_DISPLAY_NAME_BYTES: usize = 256;
pub const MAX_EVENT_TYPE_BYTES: usize = 128;
pub const MAX_AGGREGATE_KIND_BYTES: usize = 128;
pub const MAX_QUANTITY_SCALE: u8 = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractValidationError {
    Empty(&'static str),
    TooLong { field: &'static str, maximum: usize },
    NilIdentifier,
    InvalidCurrency,
    InvalidQuantityScale,
    OutOfRange(&'static str),
    ZeroVersion(&'static str),
    InvalidTimestamp,
    OwnerMismatch,
    EventTypeMismatch,
    EventVersionMismatch,
    InvalidTenantScope,
}

impl fmt::Display for ContractValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty(field) => write!(formatter, "{field} must not be empty"),
            Self::TooLong { field, maximum } => {
                write!(formatter, "{field} exceeds {maximum} bytes")
            }
            Self::NilIdentifier => formatter.write_str("identifier must not be nil"),
            Self::InvalidCurrency => {
                formatter.write_str("currency must be a three-letter uppercase ASCII code")
            }
            Self::InvalidQuantityScale => formatter.write_str("quantity scale exceeds 9"),
            Self::OutOfRange(field) => write!(formatter, "{field} is outside its valid range"),
            Self::ZeroVersion(field) => write!(formatter, "{field} must be nonzero"),
            Self::InvalidTimestamp => {
                formatter.write_str("event timestamp must not predate the Unix epoch")
            }
            Self::OwnerMismatch => {
                formatter.write_str("event owner does not match the typed payload owner")
            }
            Self::EventTypeMismatch => {
                formatter.write_str("event type does not match the typed payload variant")
            }
            Self::EventVersionMismatch => {
                formatter.write_str("event version does not match the typed payload schema")
            }
            Self::InvalidTenantScope => formatter.write_str("tenant scope is invalid"),
        }
    }
}

impl Error for ContractValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundedText<const MAXIMUM: usize>(String);

impl<const MAXIMUM: usize> BoundedText<MAXIMUM> {
    pub fn try_new(value: impl Into<String>) -> Result<Self, ContractValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ContractValidationError::Empty("text"));
        }
        if value.len() > MAXIMUM {
            return Err(ContractValidationError::TooLong {
                field: "text",
                maximum: MAXIMUM,
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl<const MAXIMUM: usize> fmt::Display for BoundedText<MAXIMUM> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<const MAXIMUM: usize> Serialize for BoundedText<MAXIMUM> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de, const MAXIMUM: usize> Deserialize<'de> for BoundedText<MAXIMUM> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

pub type ReferenceCode = BoundedText<MAX_REFERENCE_CODE_BYTES>;
pub type DisplayName = BoundedText<MAX_DISPLAY_NAME_BYTES>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    pub fn try_new(value: &str) -> Result<Self, ContractValidationError> {
        let bytes: [u8; 3] = value
            .as_bytes()
            .try_into()
            .map_err(|_| ContractValidationError::InvalidCurrency)?;
        if !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(ContractValidationError::InvalidCurrency);
        }
        Ok(Self(bytes))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("validated ASCII currency code")
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for CurrencyCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoneyAmount {
    minor_units: i64,
    currency: CurrencyCode,
}

impl MoneyAmount {
    pub const fn new(minor_units: i64, currency: CurrencyCode) -> Self {
        Self {
            minor_units,
            currency,
        }
    }

    pub const fn minor_units(self) -> i64 {
        self.minor_units
    }

    pub const fn currency(self) -> CurrencyCode {
        self.currency
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Quantity {
    units: i64,
    scale: u8,
}

impl Quantity {
    pub fn try_new(units: i64, scale: u8) -> Result<Self, ContractValidationError> {
        if scale > MAX_QUANTITY_SCALE {
            return Err(ContractValidationError::InvalidQuantityScale);
        }
        Ok(Self { units, scale })
    }

    pub const fn units(self) -> i64 {
        self.units
    }

    pub const fn scale(self) -> u8 {
        self.scale
    }
}

impl<'de> Deserialize<'de> for Quantity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireQuantity {
            units: i64,
            scale: u8,
        }

        let value = WireQuantity::deserialize(deserializer)?;
        Self::try_new(value.units, value.scale).map_err(de::Error::custom)
    }
}
