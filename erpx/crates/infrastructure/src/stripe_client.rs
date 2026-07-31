//! Bounded, typed HTTP transport for Stripe's API.
//!
//! This module deliberately stops at the provider boundary. It does not
//! interpret billing events, persist catalog state, or handle webhooks. The
//! webhook path remains the raw-body verification/persistence boundary in
//! [`crate::StripeWebhookPersistenceContext`].

use crate::{
    StripeEntitlementFeatureId, StripeMeterId, StripePriceId, StripeProductId,
    StripeProviderConfig, StripeProviderConfigError, StripeProviderError,
};
use reqwest::{Client, Method};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use zeroize::Zeroize;

pub const DEFAULT_STRIPE_HTTP_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_STRIPE_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
pub const DEFAULT_STRIPE_REQUEST_BODY_BYTES: usize = 256 * 1024;
pub const DEFAULT_STRIPE_RESPONSE_BODY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_STRIPE_REQUEST_BODY_BYTES: usize = 1024 * 1024;
pub const MAX_STRIPE_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;

/// An API credential that is never printed or included in provider errors.
///
/// The value is intentionally only exposed as a borrowed string to the HTTP
/// adapter. Callers should construct it from a secret manager result and
/// should not persist or serialize it.
#[derive(Clone)]
pub struct StripeApiKey(String);

impl StripeApiKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, StripeCredentialError> {
        let value = value.into();
        if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
            return Err(StripeCredentialError::Invalid);
        }
        if !(value.starts_with("sk_") || value.starts_with("rk_")) || value.len() <= 3 {
            return Err(StripeCredentialError::Invalid);
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for StripeApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StripeApiKey([REDACTED])")
    }
}

impl Drop for StripeApiKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum StripeCredentialError {
    #[error("Stripe API credential is not configured")]
    Missing,
    #[error("Stripe API credential is invalid")]
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripeHttpLimits {
    pub timeout: Duration,
    pub max_request_body_bytes: usize,
    pub max_response_body_bytes: usize,
}

impl Default for StripeHttpLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_STRIPE_HTTP_TIMEOUT,
            max_request_body_bytes: DEFAULT_STRIPE_REQUEST_BODY_BYTES,
            max_response_body_bytes: DEFAULT_STRIPE_RESPONSE_BODY_BYTES,
        }
    }
}

impl StripeHttpLimits {
    pub fn validate(self) -> Result<Self, StripeHttpConfigurationError> {
        if self.timeout.is_zero() || self.timeout > MAX_STRIPE_HTTP_TIMEOUT {
            return Err(StripeHttpConfigurationError::InvalidTimeout);
        }
        if self.max_request_body_bytes == 0
            || self.max_request_body_bytes > MAX_STRIPE_REQUEST_BODY_BYTES
        {
            return Err(StripeHttpConfigurationError::InvalidRequestBodyLimit);
        }
        if self.max_response_body_bytes == 0
            || self.max_response_body_bytes > MAX_STRIPE_RESPONSE_BODY_BYTES
        {
            return Err(StripeHttpConfigurationError::InvalidResponseBodyLimit);
        }
        Ok(self)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeHttpConfigurationError {
    #[error(transparent)]
    Provider(#[from] StripeProviderConfigError),
    #[error("Stripe HTTP timeout is outside the permitted range")]
    InvalidTimeout,
    #[error("Stripe HTTP request body limit is outside the permitted range")]
    InvalidRequestBodyLimit,
    #[error("Stripe HTTP response body limit is outside the permitted range")]
    InvalidResponseBodyLimit,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeHttpRequestError {
    #[error("Stripe HTTP idempotency key is required for a mutating request")]
    MissingIdempotencyKey,
    #[error("Stripe HTTP idempotency key is invalid")]
    InvalidIdempotencyKey,
    #[error("Stripe request field is invalid: {field}")]
    InvalidField { field: &'static str },
    #[error("Stripe request body exceeds the configured limit")]
    BodyTooLarge,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeHttpError {
    #[error(transparent)]
    Configuration(#[from] StripeHttpConfigurationError),
    #[error(transparent)]
    Credentials(#[from] StripeCredentialError),
    #[error(transparent)]
    Request(#[from] StripeHttpRequestError),
    #[error("Stripe HTTP request timed out")]
    Timeout,
    #[error("Stripe HTTP transport is unavailable")]
    Transport,
    #[error("Stripe HTTP response body exceeds the configured limit")]
    ResponseBodyTooLarge,
    #[error(transparent)]
    Provider(#[from] StripeProviderError),
    #[error("Stripe response could not be decoded as {resource}")]
    Decode { resource: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeHttpMethod {
    Get,
    Post,
}

impl StripeHttpMethod {
    fn as_reqwest(self) -> Method {
        match self {
            Self::Get => Method::GET,
            Self::Post => Method::POST,
        }
    }

    fn is_mutating(self) -> bool {
        matches!(self, Self::Post)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StripeHttpEndpoint {
    Products,
    Product(StripeProductId),
    Prices,
    Price(StripePriceId),
    CheckoutSessions,
    EntitlementFeatures,
    EntitlementFeature(StripeEntitlementFeatureId),
    BillingMeters,
    BillingMeter(StripeMeterId),
}

impl StripeHttpEndpoint {
    fn path(&self) -> String {
        match self {
            Self::Products => "/v1/products".into(),
            Self::Product(id) => format!("/v1/products/{}", id.as_str()),
            Self::Prices => "/v1/prices".into(),
            Self::Price(id) => format!("/v1/prices/{}", id.as_str()),
            Self::CheckoutSessions => "/v1/checkout/sessions".into(),
            Self::EntitlementFeatures => "/v1/entitlements/features".into(),
            Self::EntitlementFeature(id) => format!("/v1/entitlements/features/{}", id.as_str()),
            Self::BillingMeters => "/v1/billing/meters".into(),
            Self::BillingMeter(id) => format!("/v1/billing/meters/{}", id.as_str()),
        }
    }
}

/// A typed request envelope. No raw URL, header map, JSON value, or
/// credential is representable by this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeRequestEnvelope<T> {
    endpoint: StripeHttpEndpoint,
    method: StripeHttpMethod,
    body: T,
    idempotency_key: Option<String>,
}

impl<T> StripeRequestEnvelope<T> {
    pub fn endpoint(&self) -> &StripeHttpEndpoint {
        &self.endpoint
    }

    pub fn method(&self) -> StripeHttpMethod {
        self.method
    }

    pub fn body(&self) -> &T {
        &self.body
    }

    pub fn idempotency_key(&self) -> Option<&str> {
        self.idempotency_key.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeResponseEnvelope<T> {
    pub status: u16,
    pub request_id: Option<String>,
    pub api_version: String,
    pub body: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeProductCreateRequest {
    pub name: String,
    pub active: Option<bool>,
    pub description: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripeProductUpdateRequest {
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StripePriceInterval {
    Month,
    Year,
}

impl StripePriceInterval {
    fn as_str(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StripePriceUsageType {
    Licensed,
    Metered,
}

impl StripePriceUsageType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Licensed => "licensed",
            Self::Metered => "metered",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripePriceRecurring {
    pub interval: StripePriceInterval,
    pub usage_type: StripePriceUsageType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripePriceCreateRequest {
    pub product: StripeProductId,
    pub currency: String,
    pub unit_amount_minor: i64,
    pub active: Option<bool>,
    pub recurring: Option<StripePriceRecurring>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripeProductListRequest {
    pub active: Option<bool>,
    pub limit: Option<u16>,
    pub starting_after: Option<StripeProductId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripePriceListRequest {
    pub product: Option<StripeProductId>,
    pub active: Option<bool>,
    pub limit: Option<u16>,
    pub starting_after: Option<StripePriceId>,
}

impl StripeFormBody for StripeProductListRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        if self.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
            return Err(StripeHttpRequestError::InvalidField { field: "limit" });
        }
        let mut fields = Vec::new();
        if let Some(active) = self.active {
            fields.push(("active".into(), active.to_string()));
        }
        if let Some(limit) = self.limit {
            fields.push(("limit".into(), limit.to_string()));
        }
        if let Some(starting_after) = &self.starting_after {
            fields.push(("starting_after".into(), starting_after.as_str().into()));
        }
        Ok(fields)
    }
}

impl StripeFormBody for StripePriceListRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        if self.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
            return Err(StripeHttpRequestError::InvalidField { field: "limit" });
        }
        let mut fields = Vec::new();
        if let Some(product) = &self.product {
            fields.push(("product".into(), product.as_str().into()));
        }
        if let Some(active) = self.active {
            fields.push(("active".into(), active.to_string()));
        }
        if let Some(limit) = self.limit {
            fields.push(("limit".into(), limit.to_string()));
        }
        if let Some(starting_after) = &self.starting_after {
            fields.push(("starting_after".into(), starting_after.as_str().into()));
        }
        Ok(fields)
    }
}

// These request DTOs are constructed by the server/catalog integration
// boundary, not by this low-level transport crate's internal call graph.
// Keep them public and typed without forcing a fake in-crate construction
// solely to satisfy dead-code analysis.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeEntitlementFeatureCreateRequest {
    pub lookup_key: String,
    pub name: String,
    pub metadata: BTreeMap<String, String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripeEntitlementFeatureListRequest {
    pub archived: Option<bool>,
    pub lookup_key: Option<String>,
    pub limit: Option<u16>,
    pub starting_after: Option<StripeEntitlementFeatureId>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StripeMeterAggregationFormula {
    Count,
    Sum,
    Last,
}

#[allow(dead_code)]
impl StripeMeterAggregationFormula {
    fn as_str(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Last => "last",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripeMeterAggregation {
    pub formula: StripeMeterAggregationFormula,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StripeMeterStatus {
    Active,
    Inactive,
}

#[allow(dead_code)]
impl StripeMeterStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeMeterCreateRequest {
    pub display_name: String,
    pub event_name: String,
    pub default_aggregation: StripeMeterAggregation,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripeMeterListRequest {
    pub status: Option<StripeMeterStatus>,
    pub limit: Option<u16>,
    pub starting_after: Option<StripeMeterId>,
}

impl StripeFormBody for StripeEntitlementFeatureCreateRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        validate_text("lookup_key", &self.lookup_key, 80)?;
        validate_text("name", &self.name, 80)?;
        let mut fields = vec![
            ("lookup_key".into(), self.lookup_key.clone()),
            ("name".into(), self.name.clone()),
        ];
        append_metadata(&mut fields, &self.metadata)?;
        Ok(fields)
    }
}

impl StripeFormBody for StripeEntitlementFeatureListRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        if self.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
            return Err(StripeHttpRequestError::InvalidField { field: "limit" });
        }
        if let Some(lookup_key) = &self.lookup_key {
            validate_text("lookup_key", lookup_key, 80)?;
        }
        let mut fields = Vec::new();
        if let Some(archived) = self.archived {
            fields.push(("archived".into(), archived.to_string()));
        }
        if let Some(lookup_key) = &self.lookup_key {
            fields.push(("lookup_key".into(), lookup_key.clone()));
        }
        if let Some(limit) = self.limit {
            fields.push(("limit".into(), limit.to_string()));
        }
        if let Some(starting_after) = &self.starting_after {
            fields.push(("starting_after".into(), starting_after.as_str().into()));
        }
        Ok(fields)
    }
}

impl StripeFormBody for StripeMeterCreateRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        validate_text("display_name", &self.display_name, 250)?;
        validate_text("event_name", &self.event_name, 100)?;
        Ok(vec![
            ("display_name".into(), self.display_name.clone()),
            ("event_name".into(), self.event_name.clone()),
            (
                "default_aggregation[formula]".into(),
                self.default_aggregation.formula.as_str().into(),
            ),
        ])
    }
}

impl StripeFormBody for StripeMeterListRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        if self.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
            return Err(StripeHttpRequestError::InvalidField { field: "limit" });
        }
        let mut fields = Vec::new();
        if let Some(status) = self.status {
            fields.push(("status".into(), status.as_str().into()));
        }
        if let Some(limit) = self.limit {
            fields.push(("limit".into(), limit.to_string()));
        }
        if let Some(starting_after) = &self.starting_after {
            fields.push(("starting_after".into(), starting_after.as_str().into()));
        }
        Ok(fields)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeCheckoutLineItem {
    pub price: StripePriceId,
    pub quantity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeCheckoutMode {
    Payment,
    Subscription,
}

impl StripeCheckoutMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Payment => "payment",
            Self::Subscription => "subscription",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeCheckoutSessionCreateRequest {
    pub mode: StripeCheckoutMode,
    pub customer: Option<String>,
    pub success_url: String,
    pub cancel_url: String,
    pub line_items: Vec<StripeCheckoutLineItem>,
}

// Responses are populated by serde from Stripe's remote API and therefore
// have no required in-crate constructor.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StripeEntitlementFeatureResponse {
    pub id: StripeEntitlementFeatureId,
    pub lookup_key: String,
    pub name: String,
    pub active: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct StripeMeterAggregationResponse {
    pub formula: StripeMeterAggregationFormula,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StripeMeterResponse {
    pub id: StripeMeterId,
    pub display_name: String,
    pub event_name: String,
    pub default_aggregation: StripeMeterAggregationResponse,
    pub status: StripeMeterStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StripeProductResponse {
    pub id: StripeProductId,
    pub name: String,
    pub active: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StripePriceResponse {
    pub id: StripePriceId,
    pub product: StripeProductId,
    pub active: bool,
    pub currency: String,
    pub unit_amount: Option<i64>,
    #[serde(default)]
    pub recurring: Option<StripePriceRecurringResponse>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct StripePriceRecurringResponse {
    pub interval: StripePriceInterval,
    pub usage_type: StripePriceUsageType,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StripeCheckoutSessionResponse {
    pub id: String,
    pub status: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StripeListResponse<T> {
    pub data: Vec<T>,
    pub has_more: bool,
}

trait StripeFormBody {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError>;
}

impl StripeFormBody for StripeProductCreateRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        validate_text("name", &self.name, 255)?;
        if let Some(description) = &self.description {
            validate_text("description", description, 4_000)?;
        }
        let mut fields = vec![("name".into(), self.name.clone())];
        if let Some(active) = self.active {
            fields.push(("active".into(), active.to_string()));
        }
        if let Some(description) = &self.description {
            fields.push(("description".into(), description.clone()));
        }
        append_metadata(&mut fields, &self.metadata)?;
        Ok(fields)
    }
}

impl StripeFormBody for StripeProductUpdateRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        let Some(active) = self.active else {
            return Err(StripeHttpRequestError::InvalidField { field: "active" });
        };
        Ok(vec![("active".into(), active.to_string())])
    }
}

impl StripeFormBody for StripePriceCreateRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        validate_currency(&self.currency)?;
        if self.unit_amount_minor <= 0 {
            return Err(StripeHttpRequestError::InvalidField {
                field: "unit_amount_minor",
            });
        }
        let mut fields = vec![
            ("product".into(), self.product.as_str().into()),
            ("currency".into(), self.currency.to_ascii_lowercase()),
            ("unit_amount".into(), self.unit_amount_minor.to_string()),
        ];
        if let Some(active) = self.active {
            fields.push(("active".into(), active.to_string()));
        }
        if let Some(recurring) = self.recurring {
            fields.push((
                "recurring[interval]".into(),
                recurring.interval.as_str().into(),
            ));
            fields.push((
                "recurring[usage_type]".into(),
                recurring.usage_type.as_str().into(),
            ));
        }
        append_metadata(&mut fields, &self.metadata)?;
        Ok(fields)
    }
}

impl StripeFormBody for StripeCheckoutSessionCreateRequest {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        validate_url("success_url", &self.success_url)?;
        validate_url("cancel_url", &self.cancel_url)?;
        if self.line_items.is_empty() || self.line_items.len() > 100 {
            return Err(StripeHttpRequestError::InvalidField {
                field: "line_items",
            });
        }
        if let Some(customer) = &self.customer {
            validate_customer_id(customer)?;
        }
        let mut fields = vec![
            ("mode".into(), self.mode.as_str().into()),
            ("success_url".into(), self.success_url.clone()),
            ("cancel_url".into(), self.cancel_url.clone()),
        ];
        if let Some(customer) = &self.customer {
            fields.push(("customer".into(), customer.clone()));
        }
        for (index, item) in self.line_items.iter().enumerate() {
            if item.quantity == 0 {
                return Err(StripeHttpRequestError::InvalidField {
                    field: "line_items.quantity",
                });
            }
            fields.push((
                format!("line_items[{index}][price]"),
                item.price.as_str().into(),
            ));
            fields.push((
                format!("line_items[{index}][quantity]"),
                item.quantity.to_string(),
            ));
        }
        Ok(fields)
    }
}

impl StripeFormBody for () {
    fn encode_form(&self) -> Result<Vec<(String, String)>, StripeHttpRequestError> {
        Ok(Vec::new())
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), StripeHttpRequestError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(StripeHttpRequestError::InvalidField { field });
    }
    Ok(())
}

fn append_metadata(
    fields: &mut Vec<(String, String)>,
    metadata: &BTreeMap<String, String>,
) -> Result<(), StripeHttpRequestError> {
    if metadata.len() > 50 {
        return Err(StripeHttpRequestError::InvalidField { field: "metadata" });
    }
    for (key, value) in metadata {
        if key.trim().is_empty()
            || key.len() > 40
            || value.len() > 500
            || key.chars().any(char::is_control)
            || value.chars().any(char::is_control)
        {
            return Err(StripeHttpRequestError::InvalidField { field: "metadata" });
        }
        fields.push((format!("metadata[{key}]"), value.clone()));
    }
    Ok(())
}

fn validate_currency(value: &str) -> Result<(), StripeHttpRequestError> {
    if value.len() != 3
        || !value
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err(StripeHttpRequestError::InvalidField { field: "currency" });
    }
    Ok(())
}

fn validate_customer_id(value: &str) -> Result<(), StripeHttpRequestError> {
    if !value.starts_with("cus_")
        || value.len() <= 4
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
    {
        return Err(StripeHttpRequestError::InvalidField { field: "customer" });
    }
    Ok(())
}

fn validate_url(field: &'static str, value: &str) -> Result<(), StripeHttpRequestError> {
    if value.len() > 2_048
        || value.chars().any(char::is_control)
        || reqwest::Url::parse(value)
            .map(|url| url.scheme() != "https")
            .unwrap_or(true)
    {
        return Err(StripeHttpRequestError::InvalidField { field });
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<(), StripeHttpRequestError> {
    if value.trim().is_empty() {
        return Err(StripeHttpRequestError::MissingIdempotencyKey);
    }
    if value.len() > 255 || value.chars().any(char::is_control) {
        return Err(StripeHttpRequestError::InvalidIdempotencyKey);
    }
    Ok(())
}

fn encode_form(fields: &[(String, String)]) -> Vec<u8> {
    fields
        .iter()
        .map(|(name, value)| format!("{}={}", percent_encode(name), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn stripe_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::none()
}

fn apply_form_body(
    builder: reqwest::RequestBuilder,
    method: StripeHttpMethod,
    body: Vec<u8>,
    idempotency_key: Option<&str>,
) -> reqwest::RequestBuilder {
    if method.is_mutating() {
        builder
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Idempotency-Key", idempotency_key.unwrap_or_default())
            .body(body)
    } else {
        // Stripe list/retrieve requests use query parameters (the same wire
        // shape as curl -G). Do not also send a GET body: intermediaries and
        // providers are allowed to ignore it, which would silently drop a
        // pagination cursor or filter.
        builder
    }
}

#[derive(Debug)]
pub struct StripeHttpClient {
    config: StripeProviderConfig,
    api_key: Arc<StripeApiKey>,
    client: Client,
    limits: StripeHttpLimits,
}

impl StripeHttpClient {
    pub fn new(
        config: StripeProviderConfig,
        api_key: StripeApiKey,
        limits: StripeHttpLimits,
    ) -> Result<Self, StripeHttpError> {
        config
            .validate()
            .map_err(StripeHttpConfigurationError::from)?;
        let limits = limits.validate()?;
        let client = Client::builder()
            .https_only(true)
            .redirect(stripe_redirect_policy())
            .timeout(limits.timeout)
            .user_agent("erpx-stripe-boundary/1")
            .build()
            .map_err(|_| StripeHttpError::Transport)?;
        Ok(Self {
            config,
            api_key: Arc::new(api_key),
            client,
            limits,
        })
    }

    pub fn with_defaults(
        config: StripeProviderConfig,
        api_key: StripeApiKey,
    ) -> Result<Self, StripeHttpError> {
        Self::new(config, api_key, StripeHttpLimits::default())
    }

    pub fn config(&self) -> &StripeProviderConfig {
        &self.config
    }

    pub fn limits(&self) -> StripeHttpLimits {
        self.limits
    }

    pub async fn create_product(
        &self,
        request: StripeProductCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<StripeProductResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Products,
            method: StripeHttpMethod::Post,
            body: request,
            idempotency_key: Some(idempotency_key.into()),
        };
        self.execute(envelope, "product").await
    }

    /// Archive a just-created Product when a dependent Price operation fails.
    /// The caller supplies a deterministic idempotency key so a retry cannot
    /// accidentally reactivate or duplicate compensation.
    pub async fn archive_product(
        &self,
        product: StripeProductId,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<StripeProductResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Product(product),
            method: StripeHttpMethod::Post,
            body: StripeProductUpdateRequest {
                active: Some(false),
            },
            idempotency_key: Some(idempotency_key.into()),
        };
        self.execute(envelope, "product archive").await
    }

    pub async fn create_price(
        &self,
        request: StripePriceCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<StripePriceResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Prices,
            method: StripeHttpMethod::Post,
            body: request,
            idempotency_key: Some(idempotency_key.into()),
        };
        self.execute(envelope, "price").await
    }

    pub async fn create_checkout_session(
        &self,
        request: StripeCheckoutSessionCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<StripeCheckoutSessionResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::CheckoutSessions,
            method: StripeHttpMethod::Post,
            body: request,
            idempotency_key: Some(idempotency_key.into()),
        };
        self.execute(envelope, "checkout session").await
    }

    pub async fn list_products(
        &self,
        request: StripeProductListRequest,
    ) -> Result<StripeResponseEnvelope<StripeListResponse<StripeProductResponse>>, StripeHttpError>
    {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Products,
            method: StripeHttpMethod::Get,
            body: request,
            idempotency_key: None,
        };
        self.execute(envelope, "product list").await
    }

    pub async fn list_prices(
        &self,
        request: StripePriceListRequest,
    ) -> Result<StripeResponseEnvelope<StripeListResponse<StripePriceResponse>>, StripeHttpError>
    {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Prices,
            method: StripeHttpMethod::Get,
            body: request,
            idempotency_key: None,
        };
        self.execute(envelope, "price list").await
    }

    pub async fn retrieve_product(
        &self,
        product: StripeProductId,
    ) -> Result<StripeResponseEnvelope<StripeProductResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Product(product),
            method: StripeHttpMethod::Get,
            body: (),
            idempotency_key: None,
        };
        self.execute(envelope, "product").await
    }

    pub async fn retrieve_price(
        &self,
        price: StripePriceId,
    ) -> Result<StripeResponseEnvelope<StripePriceResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Price(price),
            method: StripeHttpMethod::Get,
            body: (),
            idempotency_key: None,
        };
        self.execute(envelope, "price").await
    }

    pub async fn create_entitlement_feature(
        &self,
        request: StripeEntitlementFeatureCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<StripeEntitlementFeatureResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::EntitlementFeatures,
            method: StripeHttpMethod::Post,
            body: request,
            idempotency_key: Some(idempotency_key.into()),
        };
        self.execute(envelope, "entitlement feature").await
    }

    pub async fn list_entitlement_features(
        &self,
        request: StripeEntitlementFeatureListRequest,
    ) -> Result<
        StripeResponseEnvelope<StripeListResponse<StripeEntitlementFeatureResponse>>,
        StripeHttpError,
    > {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::EntitlementFeatures,
            method: StripeHttpMethod::Get,
            body: request,
            idempotency_key: None,
        };
        self.execute(envelope, "entitlement feature list").await
    }

    pub async fn retrieve_entitlement_feature(
        &self,
        feature: StripeEntitlementFeatureId,
    ) -> Result<StripeResponseEnvelope<StripeEntitlementFeatureResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::EntitlementFeature(feature),
            method: StripeHttpMethod::Get,
            body: (),
            idempotency_key: None,
        };
        self.execute(envelope, "entitlement feature").await
    }

    pub async fn create_meter(
        &self,
        request: StripeMeterCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<StripeMeterResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::BillingMeters,
            method: StripeHttpMethod::Post,
            body: request,
            idempotency_key: Some(idempotency_key.into()),
        };
        self.execute(envelope, "meter").await
    }

    pub async fn list_meters(
        &self,
        request: StripeMeterListRequest,
    ) -> Result<StripeResponseEnvelope<StripeListResponse<StripeMeterResponse>>, StripeHttpError>
    {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::BillingMeters,
            method: StripeHttpMethod::Get,
            body: request,
            idempotency_key: None,
        };
        self.execute(envelope, "meter list").await
    }

    pub async fn retrieve_meter(
        &self,
        meter: StripeMeterId,
    ) -> Result<StripeResponseEnvelope<StripeMeterResponse>, StripeHttpError> {
        let envelope = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::BillingMeter(meter),
            method: StripeHttpMethod::Get,
            body: (),
            idempotency_key: None,
        };
        self.execute(envelope, "meter").await
    }

    async fn execute<T: StripeFormBody, R: DeserializeOwned>(
        &self,
        request: StripeRequestEnvelope<T>,
        resource: &'static str,
    ) -> Result<StripeResponseEnvelope<R>, StripeHttpError> {
        let fields = request.body.encode_form()?;
        let body = encode_form(&fields);
        if body.len() > self.limits.max_request_body_bytes {
            return Err(StripeHttpRequestError::BodyTooLarge.into());
        }
        if request.method.is_mutating() {
            let key = request
                .idempotency_key
                .as_deref()
                .ok_or(StripeHttpRequestError::MissingIdempotencyKey)?;
            validate_idempotency_key(key)?;
        }

        let base_url = format!("{}{}", self.config.api_base_url(), request.endpoint.path());
        let url = if request.method == StripeHttpMethod::Get && !body.is_empty() {
            format!("{base_url}?{}", String::from_utf8_lossy(&body))
        } else {
            base_url
        };
        let mut builder = self
            .client
            .request(request.method.as_reqwest(), &url)
            .header("Authorization", format!("Bearer {}", self.api_key.as_str()))
            .header("Stripe-Version", self.config.api_version())
            .header("Stripe-Account", self.config.account_ref().as_str())
            .header("Accept", "application/json");
        builder = apply_form_body(
            builder,
            request.method,
            body,
            request.idempotency_key.as_deref(),
        );

        let response = builder.send().await.map_err(map_transport_error)?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("request-id")
            .and_then(|value| value.to_str().ok())
            .and_then(safe_request_id);
        let body = read_bounded_body(response, self.limits.max_response_body_bytes).await?;
        if !status.is_success() {
            let provider_error = StripeProviderError::from_remote_response(
                status.as_u16(),
                None,
                request_id.as_deref(),
                std::str::from_utf8(&body).unwrap_or_default(),
            );
            return Err(provider_error.into());
        }
        let body =
            serde_json::from_slice::<R>(&body).map_err(|_| StripeHttpError::Decode { resource })?;
        Ok(StripeResponseEnvelope {
            status: status.as_u16(),
            request_id,
            api_version: self.config.api_version().into(),
            body,
        })
    }
}

async fn read_bounded_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, StripeHttpError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(StripeHttpError::ResponseBodyTooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(map_transport_error)? {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(StripeHttpError::ResponseBodyTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn map_transport_error(error: reqwest::Error) -> StripeHttpError {
    if error.is_timeout() {
        StripeHttpError::Timeout
    } else {
        StripeHttpError::Transport
    }
}

fn safe_request_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with("req_")
        && value.len() <= 96
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
    {
        Some(value.into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PINNED_STRIPE_API_VERSION, StripeProviderErrorKind};

    fn client() -> StripeHttpClient {
        StripeHttpClient::with_defaults(
            StripeProviderConfig::pinned("acct_test").expect("config"),
            StripeApiKey::parse("sk_test_boundary_key").expect("key"),
        )
        .expect("client")
    }

    #[test]
    fn credential_debug_and_errors_never_contain_secret() {
        let secret = "sk_test_super_secret_do_not_log";
        let key = StripeApiKey::parse(secret).expect("key");
        assert!(!format!("{key:?}").contains(secret));
        let error = StripeCredentialError::Invalid;
        assert!(!error.to_string().contains("super_secret"));
        assert!(!format!("{:?}", client()).contains(secret));
    }

    #[test]
    fn endpoint_and_version_are_fixed_by_provider_configuration() {
        let client = client();
        assert_eq!(client.config().api_base_url(), "https://api.stripe.com");
        assert_eq!(client.config().api_version(), PINNED_STRIPE_API_VERSION);
        assert_eq!(client.limits(), StripeHttpLimits::default());
        assert!(
            StripeProviderConfig::new(
                PINNED_STRIPE_API_VERSION,
                "acct_test",
                "https://evil.example"
            )
            .is_err()
        );
        assert!(
            StripeProviderConfig::new(
                PINNED_STRIPE_API_VERSION,
                "acct_test",
                "http://api.stripe.com"
            )
            .is_err()
        );
    }

    #[test]
    fn stripe_transport_is_tls_only_and_does_not_follow_redirects() {
        assert_eq!(format!("{:?}", stripe_redirect_policy()), "Policy(None)");
        let client = Client::builder()
            .https_only(true)
            .redirect(stripe_redirect_policy())
            .build()
            .expect("client");
        let request = client
            .get("https://api.stripe.com/v1/products")
            .build()
            .expect("request");
        assert_eq!(request.url().scheme(), "https");
    }

    #[test]
    fn stripe_get_forms_are_query_only_while_post_forms_are_body_encoded() {
        let client = Client::new();
        let get_request = apply_form_body(
            client.get("https://api.stripe.com/v1/products?limit=100&starting_after=prod_test"),
            StripeHttpMethod::Get,
            b"limit=100&starting_after=prod_test".to_vec(),
            None,
        )
        .build()
        .expect("GET request");
        assert_eq!(
            get_request.url().query(),
            Some("limit=100&starting_after=prod_test")
        );
        assert!(get_request.body().is_none());

        let post_request = apply_form_body(
            client.post("https://api.stripe.com/v1/products"),
            StripeHttpMethod::Post,
            b"name=Starter%20plan".to_vec(),
            Some("catalog-publish-1"),
        )
        .build()
        .expect("POST request");
        assert_eq!(
            post_request
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            post_request
                .headers()
                .get("idempotency-key")
                .and_then(|value| value.to_str().ok()),
            Some("catalog-publish-1")
        );
        assert_eq!(
            post_request.body().and_then(reqwest::Body::as_bytes),
            Some(&b"name=Starter%20plan"[..])
        );
    }

    #[test]
    fn limits_bound_timeout_and_bodies() {
        assert_eq!(
            StripeHttpLimits {
                timeout: Duration::ZERO,
                ..StripeHttpLimits::default()
            }
            .validate(),
            Err(StripeHttpConfigurationError::InvalidTimeout)
        );
        assert_eq!(
            StripeHttpLimits {
                max_request_body_bytes: MAX_STRIPE_REQUEST_BODY_BYTES + 1,
                ..StripeHttpLimits::default()
            }
            .validate(),
            Err(StripeHttpConfigurationError::InvalidRequestBodyLimit)
        );
        assert_eq!(
            StripeHttpLimits {
                max_response_body_bytes: MAX_STRIPE_RESPONSE_BODY_BYTES + 1,
                ..StripeHttpLimits::default()
            }
            .validate(),
            Err(StripeHttpConfigurationError::InvalidResponseBodyLimit)
        );
    }

    #[test]
    fn typed_mutation_envelopes_require_idempotency_and_encode_bounded_forms() {
        let product = StripeProductCreateRequest {
            name: "Starter plan".into(),
            active: Some(true),
            description: Some("A small plan".into()),
            metadata: BTreeMap::new(),
        };
        let fields = product.encode_form().expect("valid product");
        let encoded = String::from_utf8(encode_form(&fields)).expect("form");
        assert!(encoded.contains("name=Starter%20plan"));
        assert!(encoded.contains("active=true"));
        assert!(validate_idempotency_key("").is_err());
        assert!(validate_idempotency_key("bad\nkey").is_err());

        let request = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::Products,
            method: StripeHttpMethod::Post,
            body: product,
            idempotency_key: Some("publish-1".into()),
        };
        assert_eq!(request.endpoint().path(), "/v1/products");
        assert_eq!(request.method(), StripeHttpMethod::Post);
        assert_eq!(request.idempotency_key(), Some("publish-1"));

        let body = StripeCheckoutSessionCreateRequest {
            mode: StripeCheckoutMode::Subscription,
            customer: Some("cus_test".into()),
            success_url: "https://example.test/success".into(),
            cancel_url: "https://example.test/cancel".into(),
            line_items: vec![StripeCheckoutLineItem {
                price: StripePriceId::parse("price_test").expect("price"),
                quantity: 1,
            }],
        };
        assert!(body.encode_form().is_ok());
    }

    #[test]
    fn typed_responses_decode_without_public_json_values() {
        let product: StripeProductResponse =
            serde_json::from_str(r#"{"id":"prod_test","name":"Starter","active":true}"#)
                .expect("product response");
        assert_eq!(product.id.as_str(), "prod_test");

        let list: StripeListResponse<StripeProductResponse> = serde_json::from_str(
            r#"{"data":[{"id":"prod_test","name":"Starter","active":true}],"has_more":false}"#,
        )
        .expect("list response");
        assert_eq!(list.data.len(), 1);
        assert!(!list.has_more);
    }

    #[test]
    fn entitlement_and_meter_endpoints_are_fixed_and_typed() {
        let feature = StripeEntitlementFeatureId::parse("feat_finance").expect("feature");
        let meter = StripeMeterId::parse("meter_usage").expect("meter");
        assert_eq!(
            StripeHttpEndpoint::EntitlementFeatures.path(),
            "/v1/entitlements/features"
        );
        assert_eq!(
            StripeHttpEndpoint::EntitlementFeature(feature).path(),
            "/v1/entitlements/features/feat_finance"
        );
        assert_eq!(
            StripeHttpEndpoint::BillingMeters.path(),
            "/v1/billing/meters"
        );
        assert_eq!(
            StripeHttpEndpoint::BillingMeter(meter).path(),
            "/v1/billing/meters/meter_usage"
        );
    }

    #[test]
    fn entitlement_and_meter_forms_are_bounded_and_idempotent() {
        let feature = StripeEntitlementFeatureCreateRequest {
            lookup_key: "finance_read".into(),
            name: "Finance read".into(),
            metadata: BTreeMap::from([(String::from("module"), String::from("finance"))]),
        };
        let feature_fields = feature.encode_form().expect("feature form");
        assert!(
            feature_fields
                .iter()
                .any(|(key, value)| { key == "lookup_key" && value == "finance_read" })
        );
        assert!(
            feature_fields
                .iter()
                .any(|(key, value)| key == "metadata[module]" && value == "finance")
        );
        assert!(
            StripeEntitlementFeatureCreateRequest {
                lookup_key: "x".repeat(81),
                name: "Feature".into(),
                metadata: BTreeMap::new(),
            }
            .encode_form()
            .is_err()
        );
        assert!(
            StripeEntitlementFeatureCreateRequest {
                lookup_key: "feature".into(),
                name: "x".repeat(81),
                metadata: BTreeMap::new(),
            }
            .encode_form()
            .is_err()
        );

        let meter = StripeMeterCreateRequest {
            display_name: "API usage".into(),
            event_name: "api_request".into(),
            default_aggregation: StripeMeterAggregation {
                formula: StripeMeterAggregationFormula::Sum,
            },
        };
        let meter_fields = meter.encode_form().expect("meter form");
        assert!(
            meter_fields
                .iter()
                .any(|(key, value)| { key == "default_aggregation[formula]" && value == "sum" })
        );
        assert!(
            StripeMeterCreateRequest {
                display_name: "x".repeat(251),
                event_name: "api_request".into(),
                default_aggregation: StripeMeterAggregation {
                    formula: StripeMeterAggregationFormula::Count,
                },
            }
            .encode_form()
            .is_err()
        );
        assert!(
            StripeMeterListRequest {
                limit: Some(101),
                ..StripeMeterListRequest::default()
            }
            .encode_form()
            .is_err()
        );

        let feature_request = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::EntitlementFeatures,
            method: StripeHttpMethod::Post,
            body: feature,
            idempotency_key: Some("feature-publish-1".into()),
        };
        assert_eq!(feature_request.idempotency_key(), Some("feature-publish-1"));
        let meter_request = StripeRequestEnvelope {
            endpoint: StripeHttpEndpoint::BillingMeters,
            method: StripeHttpMethod::Post,
            body: meter,
            idempotency_key: Some("meter-publish-1".into()),
        };
        assert_eq!(meter_request.idempotency_key(), Some("meter-publish-1"));
        assert!(validate_idempotency_key("").is_err());
    }

    #[test]
    fn entitlement_and_meter_responses_decode_to_typed_values() {
        let feature: StripeEntitlementFeatureResponse = serde_json::from_str(
            r#"{"id":"feat_finance","lookup_key":"finance_read","name":"Finance read","active":true,"metadata":{"module":"finance"}}"#,
        )
        .expect("feature response");
        assert_eq!(feature.id.as_str(), "feat_finance");
        assert!(feature.active);
        assert_eq!(feature.metadata.get("module"), Some(&"finance".to_string()));

        let meter: StripeMeterResponse = serde_json::from_str(
            r#"{"id":"meter_usage","display_name":"API usage","event_name":"api_request","default_aggregation":{"formula":"sum"},"status":"active"}"#,
        )
        .expect("meter response");
        assert_eq!(meter.id.as_str(), "meter_usage");
        assert_eq!(
            meter.default_aggregation.formula,
            StripeMeterAggregationFormula::Sum
        );
        assert_eq!(meter.status, StripeMeterStatus::Active);
    }

    #[test]
    fn response_errors_are_redacted_and_request_ids_are_allow_listed() {
        let error = StripeProviderError::from_remote_response(
            400,
            Some("invalid_request_error"),
            Some("req_test"),
            r#"{"error":{"type":"invalid_request_error","message":"Bearer sk_test_secret"}}"#,
        );
        assert_eq!(error.kind(), StripeProviderErrorKind::InvalidRequest);
        assert!(!error.to_string().contains("sk_test_secret"));
        assert_eq!(safe_request_id("req_test"), Some("req_test".into()));
        assert_eq!(safe_request_id("authorization=secret"), None);
    }

    #[test]
    fn raw_webhook_boundary_is_not_represented_by_http_endpoint() {
        assert_eq!(StripeHttpEndpoint::Products.path(), "/v1/products");
        assert!(!StripeHttpEndpoint::Products.path().contains("webhook"));
    }
}
