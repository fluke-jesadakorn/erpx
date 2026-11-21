//! Finance customer input used by Service preparation. No HTTP/database facts
//! participate in normalization; authority and the retained Money sources are
//! checked by the application before a command can be staged.
use super::money::{FinancePolicyError, decimal_to_units};
use num_traits::ToPrimitive;
use serde::{Deserialize, Deserializer, Serialize};

fn terms_default() -> u32 {
    30
}
fn credit_default() -> String {
    "0".into()
}
fn optional_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceCustomerParty {
    #[cfg_attr(feature = "contracts", ts(type = "\"customer\""))]
    pub kind: String,
    pub code: String,
    pub name: String,
    pub currency: String,
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub tax_registration: Option<String>,
    #[serde(
        default = "terms_default",
        deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32"
    )]
    pub payment_terms_days: u32,
    #[serde(default = "credit_default")]
    pub credit_limit: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceCustomerInput {
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_u32")]
    #[cfg_attr(feature = "contracts", ts(type = "0"))]
    pub expected_version: u32,
    pub context_key: String,
    pub input: ServiceCustomerParty,
}
fn wire_money(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction
            .is_none_or(|f| !f.is_empty() && f.len() <= 6 && f.bytes().all(|b| b.is_ascii_digit()))
}
impl ServiceCustomerInput {
    /// Preserve the original string fields/defaults and uppercase transform;
    /// whitespace in names/codes is not silently changed during cutover.
    pub fn parse(raw: &[u8]) -> Result<Self, FinancePolicyError> {
        let mut value: Self = serde_json::from_slice(raw)
            .map_err(|_| FinancePolicyError("Invalid Service customer input.".into()))?;
        let party = &mut value.input;
        if value.expected_version != 0
            || value.context_key.len() != 64
            || !value
                .context_key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || party.currency.encode_utf16().count() != 3
        {
            return Err("Invalid Service customer input.".into());
        }
        party.validate_fields()?;
        party.currency = party.currency.to_uppercase();
        Ok(value)
    }
}
impl ServiceCustomerParty {
    pub(crate) fn validate_fields(&self) -> Result<(), FinancePolicyError> {
        if self.kind != "customer"
            || !(1..=64).contains(&self.code.encode_utf16().count())
            || !(1..=200).contains(&self.name.encode_utf16().count())
            || self
                .tax_registration
                .as_ref()
                .is_some_and(|v| v.encode_utf16().count() > 100)
            || self.payment_terms_days > 365
            || !wire_money(&self.credit_limit)
        {
            return Err("Invalid Service customer input.".into());
        }
        Ok(())
    }

    /// The existing Finance two-decimal minor-unit conversion, including its
    /// safe-number limit; this runs after replay and current currency admission.
    pub fn credit_minor(&self) -> Result<i64, FinancePolicyError> {
        let units = decimal_to_units(&self.credit_limit, 2)?;
        let value = units
            .to_i64()
            .filter(|v| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(v))
            .ok_or("Money amount is too large.")?;
        Ok(value)
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceCustomerResult {
    pub id: String,
    #[serde(flatten)]
    pub party: ServiceCustomerParty,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=-9_007_199_254_740_991_i64,maximum=9_007_199_254_740_991_i64))]
    pub credit_limit_minor: i64,
}
