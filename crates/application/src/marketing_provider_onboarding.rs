//! Transport-neutral Marketing provider-account onboarding commands.
//!
//! This boundary carries only opaque references to deployment-managed
//! credentials. It deliberately does not model provider tokens, API keys, or
//! vendor payloads.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_domain::contracts::{CommandContext, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MARKETING_PROVIDER_CHANNELS: [&str; 5] = ["email", "sms", "social", "push", "webhook"];
pub const MAX_MARKETING_PROVIDER_CHANNELS: usize = MARKETING_PROVIDER_CHANNELS.len();
pub const MAX_MARKETING_PROVIDER_REFERENCE_BYTES: usize = 255;
pub const MAX_MARKETING_PROVIDER_ROUTE_BYTES: usize = 512;
pub const DEFAULT_MARKETING_PROVIDER_PRIORITY: u32 = 100;
pub const MAX_MARKETING_PROVIDER_PRIORITY: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProviderChannelRoute {
    pub channel: String,
    pub endpoint: String,
    pub reconciliation_endpoint: Option<String>,
}

impl MarketingProviderChannelRoute {
    pub fn new(
        channel: impl Into<String>,
        endpoint: impl Into<String>,
        reconciliation_endpoint: Option<String>,
    ) -> Result<Self, MarketingProviderOnboardingError> {
        let channel = channel.into();
        let endpoint = bounded_route(endpoint.into())?;
        let reconciliation_endpoint = reconciliation_endpoint.map(bounded_route).transpose()?;
        if !MARKETING_PROVIDER_CHANNELS.contains(&channel.as_str()) {
            return Err(MarketingProviderOnboardingError::UnsupportedChannel);
        }
        Ok(Self {
            channel,
            endpoint,
            reconciliation_endpoint,
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingProviderOnboardingError {
    #[error("{0} is invalid")]
    Invalid(&'static str),
    #[error("provider channel is unsupported")]
    UnsupportedChannel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigureMarketingProviderAccountCommand {
    pub provider_account_id: Uuid,
    pub provider_account_ref: String,
    pub tenant_id: String,
    pub holding_ref: String,
    pub holding_id: Uuid,
    pub provider_key: String,
    pub provider_priority: u32,
    pub verifier_key_ref: String,
    pub outbound_credential_ref: String,
    pub channels: Vec<String>,
    pub channel_routes: Vec<MarketingProviderChannelRoute>,
    pub context: CommandContext,
}

impl ConfigureMarketingProviderAccountCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_account_id: Uuid,
        provider_account_ref: impl Into<String>,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        holding_id: Uuid,
        provider_key: impl Into<String>,
        provider_priority: u32,
        verifier_key_ref: impl Into<String>,
        outbound_credential_ref: impl Into<String>,
        channels: impl IntoIterator<Item = String>,
        channel_routes: impl IntoIterator<Item = MarketingProviderChannelRoute>,
        context: CommandContext,
    ) -> Result<Self, MarketingProviderOnboardingError> {
        if provider_account_id.is_nil() || holding_id.is_nil() {
            return Err(MarketingProviderOnboardingError::Invalid(
                "account identity",
            ));
        }
        let provider_account_ref = bounded_reference(provider_account_ref.into())?;
        let tenant_id = bounded_reference(tenant_id.into())?;
        let holding_ref = bounded_reference(holding_ref.into())?;
        let provider_key = bounded_reference(provider_key.into())?;
        if !(1..=MAX_MARKETING_PROVIDER_PRIORITY).contains(&provider_priority) {
            return Err(MarketingProviderOnboardingError::Invalid(
                "provider priority",
            ));
        }
        let verifier_key_ref = bounded_secret_reference(verifier_key_ref.into())?;
        let outbound_credential_ref = bounded_credential_reference(outbound_credential_ref.into())?;
        let channels = normalize_channels(channels)?;
        let channel_routes = normalize_channel_routes(&channels, channel_routes)?;
        context
            .validate()
            .map_err(|_| MarketingProviderOnboardingError::Invalid("command context"))?;
        validate_holding_context(&context, Some(holding_id))?;
        Ok(Self {
            provider_account_id,
            provider_account_ref,
            tenant_id,
            holding_ref,
            holding_id,
            provider_key,
            provider_priority,
            verifier_key_ref,
            outbound_credential_ref,
            channels,
            channel_routes,
            context,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcknowledgeMarketingProviderVerificationCommand {
    pub provider_account_id: Uuid,
    pub tenant_id: String,
    pub holding_ref: String,
    pub verification_reference: String,
    pub activate: bool,
    pub context: CommandContext,
}

impl AcknowledgeMarketingProviderVerificationCommand {
    pub fn new(
        provider_account_id: Uuid,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        verification_reference: impl Into<String>,
        activate: bool,
        context: CommandContext,
    ) -> Result<Self, MarketingProviderOnboardingError> {
        if provider_account_id.is_nil() {
            return Err(MarketingProviderOnboardingError::Invalid(
                "account identity",
            ));
        }
        let tenant_id = bounded_reference(tenant_id.into())?;
        let holding_ref = bounded_reference(holding_ref.into())?;
        let verification_reference = bounded_reference(verification_reference.into())?;
        context
            .validate()
            .map_err(|_| MarketingProviderOnboardingError::Invalid("command context"))?;
        validate_holding_context(&context, None)?;
        Ok(Self {
            provider_account_id,
            tenant_id,
            holding_ref,
            verification_reference,
            activate,
            context,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMarketingProviderAccountStateCommand {
    pub provider_account_id: Uuid,
    pub tenant_id: String,
    pub holding_ref: String,
    pub active: bool,
    pub context: CommandContext,
}

impl SetMarketingProviderAccountStateCommand {
    pub fn new(
        provider_account_id: Uuid,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        active: bool,
        context: CommandContext,
    ) -> Result<Self, MarketingProviderOnboardingError> {
        if provider_account_id.is_nil() {
            return Err(MarketingProviderOnboardingError::Invalid(
                "account identity",
            ));
        }
        let tenant_id = bounded_reference(tenant_id.into())?;
        let holding_ref = bounded_reference(holding_ref.into())?;
        context
            .validate()
            .map_err(|_| MarketingProviderOnboardingError::Invalid("command context"))?;
        validate_holding_context(&context, None)?;
        Ok(Self {
            provider_account_id,
            tenant_id,
            holding_ref,
            active,
            context,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingProviderAccountWriteReceipt {
    pub provider_account_id: Uuid,
    pub provider_account_ref: String,
    pub tenant_id: String,
    pub holding_ref: String,
    pub provider_key: String,
    pub provider_priority: u32,
    pub channels: Vec<String>,
    pub state: String,
    pub onboarding_state: String,
    pub version: u64,
    pub changed_at: DateTime<Utc>,
    pub replayed: bool,
}

fn bounded_reference(value: String) -> Result<String, MarketingProviderOnboardingError> {
    if value.trim().is_empty()
        || value.len() > MAX_MARKETING_PROVIDER_REFERENCE_BYTES
        || !value.is_ascii()
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(MarketingProviderOnboardingError::Invalid(
            "opaque reference",
        ));
    }
    Ok(value)
}

fn validate_holding_context(
    context: &CommandContext,
    expected_holding_id: Option<Uuid>,
) -> Result<(), MarketingProviderOnboardingError> {
    match &context.tenant_scope {
        TenantScope::Holding { holding_id }
            if expected_holding_id.is_none_or(|expected| expected == *holding_id) =>
        {
            Ok(())
        }
        _ => Err(MarketingProviderOnboardingError::Invalid(
            "holding command scope",
        )),
    }
}

fn bounded_credential_reference(value: String) -> Result<String, MarketingProviderOnboardingError> {
    bounded_secret_reference(value)
}

fn bounded_secret_reference(value: String) -> Result<String, MarketingProviderOnboardingError> {
    erpx_domain::marketing_callback_secret::MarketingCallbackSecretReference::new(&value)
        .map_err(|_| MarketingProviderOnboardingError::Invalid("secret reference"))?;
    Ok(value)
}

fn normalize_channels(
    channels: impl IntoIterator<Item = String>,
) -> Result<Vec<String>, MarketingProviderOnboardingError> {
    let mut normalized = BTreeSet::new();
    for channel in channels {
        if !MARKETING_PROVIDER_CHANNELS.contains(&channel.as_str()) {
            return Err(MarketingProviderOnboardingError::UnsupportedChannel);
        }
        normalized.insert(channel);
    }
    if normalized.is_empty() || normalized.len() > MAX_MARKETING_PROVIDER_CHANNELS {
        return Err(MarketingProviderOnboardingError::Invalid(
            "provider channels",
        ));
    }
    Ok(normalized.into_iter().collect())
}

fn bounded_route(value: String) -> Result<String, MarketingProviderOnboardingError> {
    if value.trim().is_empty()
        || value.len() > MAX_MARKETING_PROVIDER_ROUTE_BYTES
        || !value.is_ascii()
        || value.chars().any(|character| character.is_control())
    {
        return Err(MarketingProviderOnboardingError::Invalid("provider route"));
    }
    let Some(authority_and_path) = value.strip_prefix("https://") else {
        return Err(MarketingProviderOnboardingError::Invalid("provider route"));
    };
    let authority_end = authority_and_path
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_path.len());
    let authority = &authority_and_path[..authority_end];
    if authority.is_empty()
        || authority
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '@' | '\\'))
    {
        return Err(MarketingProviderOnboardingError::Invalid("provider route"));
    }
    Ok(value)
}

fn normalize_channel_routes(
    channels: &[String],
    routes: impl IntoIterator<Item = MarketingProviderChannelRoute>,
) -> Result<Vec<MarketingProviderChannelRoute>, MarketingProviderOnboardingError> {
    let mut routes: Vec<_> = routes.into_iter().collect();
    routes.sort_by(|left, right| left.channel.cmp(&right.channel));
    if routes.len() != channels.len()
        || routes
            .windows(2)
            .any(|window| window[0].channel == window[1].channel)
        || routes
            .iter()
            .any(|route| !channels.iter().any(|channel| channel == &route.channel))
    {
        return Err(MarketingProviderOnboardingError::Invalid(
            "provider channel routes",
        ));
    }
    Ok(routes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, TenantScope};

    fn context(holding_id: Uuid) -> CommandContext {
        CommandContext::try_new(
            "provider-config-1",
            ActorAttribution {
                principal_ref: "operator-1".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            TenantScope::Holding { holding_id },
            "provider onboarding",
        )
        .unwrap()
    }

    #[test]
    fn configuration_normalizes_channels_and_rejects_unsafe_references() {
        let holding_id = Uuid::new_v4();
        let command = ConfigureMarketingProviderAccountCommand::new(
            Uuid::new_v4(),
            "account-1",
            "tenant-1",
            "holding-1",
            holding_id,
            "provider-1",
            DEFAULT_MARKETING_PROVIDER_PRIORITY,
            "env:VERIFY_PROVIDER_1",
            "env:SEND_PROVIDER_1",
            ["sms".into(), "email".into(), "sms".into()],
            [
                MarketingProviderChannelRoute::new(
                    "email",
                    "https://email.provider.test/send",
                    None,
                )
                .unwrap(),
                MarketingProviderChannelRoute::new(
                    "sms",
                    "https://sms.provider.test/send",
                    Some("https://sms.provider.test/reconcile".into()),
                )
                .unwrap(),
            ],
            context(holding_id),
        )
        .unwrap();
        assert_eq!(command.channels, vec!["email", "sms"]);
        assert!(
            ConfigureMarketingProviderAccountCommand::new(
                Uuid::new_v4(),
                "unsafe/ref",
                "tenant-1",
                "holding-1",
                holding_id,
                "provider-1",
                DEFAULT_MARKETING_PROVIDER_PRIORITY,
                "verify",
                "send",
                ["email".into()],
                [MarketingProviderChannelRoute::new(
                    "email",
                    "https://email.provider.test/send",
                    None,
                )
                .unwrap()],
                context(holding_id),
            )
            .is_err()
        );
    }

    #[test]
    fn provider_and_reconciliation_routes_require_https_with_a_host() {
        assert!(matches!(
            MarketingProviderChannelRoute::new("email", "http://email.provider.test/send", None,),
            Err(MarketingProviderOnboardingError::Invalid("provider route"))
        ));
        assert!(matches!(
            MarketingProviderChannelRoute::new(
                "email",
                "https://email.provider.test/send",
                Some("http://email.provider.test/reconcile".into()),
            ),
            Err(MarketingProviderOnboardingError::Invalid("provider route"))
        ));
        assert!(matches!(
            MarketingProviderChannelRoute::new("email", "https://", None),
            Err(MarketingProviderOnboardingError::Invalid("provider route"))
        ));
        assert!(
            MarketingProviderChannelRoute::new(
                "email",
                "https://email.provider.test/send",
                Some("https://email.provider.test/reconcile?cursor=1".into()),
            )
            .is_ok()
        );
    }

    #[test]
    fn provider_commands_require_an_explicit_holding_scope() {
        let holding_id = Uuid::new_v4();
        let mut context = context(holding_id);
        context.tenant_scope = TenantScope::Platform;
        assert!(matches!(
            ConfigureMarketingProviderAccountCommand::new(
                Uuid::new_v4(),
                "account-1",
                "tenant-1",
                "holding-1",
                holding_id,
                "provider-1",
                DEFAULT_MARKETING_PROVIDER_PRIORITY,
                "env:VERIFY_PROVIDER_1",
                "env:SEND_PROVIDER_1",
                ["email".into()],
                [MarketingProviderChannelRoute::new(
                    "email",
                    "https://email.provider.test/send",
                    None,
                )
                .unwrap()],
                context,
            ),
            Err(MarketingProviderOnboardingError::Invalid(
                "holding command scope"
            ))
        ));
    }
}
