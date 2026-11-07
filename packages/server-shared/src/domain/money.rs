use super::error::BusinessError;

/// Exact internal decimal units. Each HTTP owner preserves its existing wire format.
/// This type deliberately has no JSON serialization or implicit floating conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecimalAmount {
    units: i128,
    scale: u8,
}

impl DecimalAmount {
    pub fn from_units(units: i128, scale: u8) -> Result<Self, BusinessError> {
        if scale > 12 {
            return Err(BusinessError::invalid(
                "INVALID_MONEY_SCALE",
                "Currency scale must be between 0 and 12.",
            ));
        }
        Ok(Self { units, scale })
    }

    pub fn parse(value: &str, scale: u8) -> Result<Self, BusinessError> {
        Self::from_units(0, scale)?;
        let value = value.trim();
        let (negative, magnitude) = value
            .strip_prefix('-')
            .map_or((false, value), |value| (true, value));
        let mut parts = magnitude.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();
        if whole.is_empty()
            || !whole.bytes().all(|value| value.is_ascii_digit())
            || parts.next().is_some()
            || fraction.is_some_and(|part| {
                part.is_empty() || !part.bytes().all(|value| value.is_ascii_digit())
            })
        {
            return Err(BusinessError::invalid(
                "INVALID_MONEY",
                "Money must be a decimal string.",
            ));
        }
        let fraction = fraction.unwrap_or_default();
        if fraction
            .bytes()
            .skip(usize::from(scale))
            .any(|digit| digit != b'0')
        {
            return Err(BusinessError::invalid(
                "MONEY_PRECISION_EXCEEDED",
                "Money has more precision than the selected scale.",
            ));
        }
        // Parse the signed scaled string together so i128::MIN is representable.
        let kept = &fraction[..fraction.len().min(usize::from(scale))];
        let padding = "0".repeat(usize::from(scale).saturating_sub(kept.len()));
        let signed = format!("{}{whole}{kept}{padding}", if negative { "-" } else { "" });
        let units = signed.parse::<i128>().map_err(|_| {
            BusinessError::invalid(
                "MONEY_OUT_OF_RANGE",
                "Money exceeds supported exact integer precision.",
            )
        })?;
        Self::from_units(units, scale)
    }

    pub const fn units(self) -> i128 {
        self.units
    }
    pub const fn scale(self) -> u8 {
        self.scale
    }

    pub fn checked_add(self, other: Self) -> Result<Self, BusinessError> {
        if self.scale != other.scale {
            return Err(BusinessError::invalid(
                "MONEY_SCALE_MISMATCH",
                "Use the same currency scale for exact addition.",
            ));
        }
        let units = self.units.checked_add(other.units).ok_or_else(|| {
            BusinessError::invalid(
                "MONEY_OUT_OF_RANGE",
                "Money exceeds supported exact integer precision.",
            )
        })?;
        Self::from_units(units, self.scale)
    }

    pub fn decimal_string(self) -> String {
        let negative = if self.units < 0 { "-" } else { "" };
        let absolute = self.units.unsigned_abs();
        if self.scale == 0 {
            return format!("{negative}{absolute}");
        }
        let base = 10_u128.pow(u32::from(self.scale));
        format!(
            "{negative}{}.{:0width$}",
            absolute / base,
            absolute % base,
            width = usize::from(self.scale)
        )
    }
}
