//! SQLx control-plane adapter for Marketing provider-account onboarding.
//!
//! Account configuration, outbox publication, audit, and idempotency commit
//! in one control-plane transaction. Provider secret values never enter this
//! adapter; only validated opaque secret-manager references are persisted.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use erpx_application::{
    AcknowledgeMarketingProviderVerificationCommand, AuditEvent,
    ConfigureMarketingProviderAccountCommand, MarketingProviderAccountWriteReceipt,
    SetMarketingProviderAccountStateCommand,
};
use erpx_domain::cqrs::OutboxEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{ControlPlaneWriteTransaction, EventMetadata, PersistenceError, SqlxControlPlane};

const AGGREGATE_TYPE: &str = "marketing_provider_account";
const EVENT_VERSION: u32 = 1;
const MAX_VERIFICATION_REFERENCE_BYTES: usize = 255;

#[derive(Debug, Clone)]
pub struct SqlxMarketingProviderOnboardingStore {
    control: Arc<SqlxControlPlane>,
}

impl SqlxMarketingProviderOnboardingStore {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    pub async fn configure(
        &self,
        command: ConfigureMarketingProviderAccountCommand,
    ) -> Result<MarketingProviderAccountWriteReceipt, PersistenceError> {
        validate_configure_command(&command)?;
        let request_hash = request_hash("configure", &command)?;
        let mut transaction = self.control.begin_write().await?;
        let reservation = transaction
            .reserve_idempotency(
                &command.context.idempotency_key,
                &request_hash,
                Some(command.context.command_id),
            )
            .await?;
        if let Some(receipt) =
            replayed_receipt(&mut transaction, reservation, command.provider_account_id).await?
        {
            transaction.commit().await?;
            return Ok(receipt);
        }

        ensure_active_holding(&mut transaction, &command.tenant_id, &command.holding_ref).await?;
        let existing = sqlx::query(
            "SELECT provider_account_id, provider_account_ref, tenant_id, holding_ref,
                    holding_id, provider_key, channels, channel_routes, state,
                    onboarding_state, version
             FROM erpx_control.marketing_provider_accounts
             WHERE provider_account_id = $1 OR provider_account_ref = $2
             FOR UPDATE",
        )
        .bind(command.provider_account_id)
        .bind(&command.provider_account_ref)
        .fetch_optional(&mut *transaction.tx)
        .await?;

        let (version, receipt) = if let Some(row) = existing {
            let existing_id: Uuid = row.try_get("provider_account_id")?;
            let existing_ref: String = row.try_get("provider_account_ref")?;
            let existing_holding_id: Uuid = row.try_get("holding_id")?;
            let current_version = positive_version(row.try_get("version")?)?;
            if existing_id != command.provider_account_id
                || existing_ref != command.provider_account_ref
                || row.try_get::<String, _>("tenant_id")? != command.tenant_id
                || row.try_get::<String, _>("holding_ref")? != command.holding_ref
                || existing_holding_id != command.holding_id
            {
                return Err(PersistenceError::InvalidData(
                    "provider account reference is already bound".into(),
                ));
            }
            require_expected_version(
                &command.context,
                command.provider_account_id,
                current_version,
            )?;
            let next_version = current_version.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("provider account version overflow".into())
            })?;
            let row = sqlx::query(
                "UPDATE erpx_control.marketing_provider_accounts
                 SET provider_key = $2, provider_priority = $3, verifier_key_ref = $4,
                     outbound_credential_ref = $5, channels = $6, channel_routes = $7,
                     state = 'suspended', onboarding_state = 'pending',
                     last_verified_at = NULL, verification_reference_hash = NULL,
                     version = $8, updated_at = now()
                 WHERE provider_account_id = $1 AND version = $9
                 RETURNING provider_account_ref, tenant_id, holding_ref, provider_key,
                           provider_priority, channels, state, onboarding_state, version, updated_at",
            )
            .bind(command.provider_account_id)
            .bind(&command.provider_key)
            .bind(i32::try_from(command.provider_priority).map_err(|_| {
                PersistenceError::InvalidData("provider priority exceeds database range".into())
            })?)
            .bind(&command.verifier_key_ref)
            .bind(&command.outbound_credential_ref)
            .bind(&command.channels)
            .bind(serde_json::to_value(&command.channel_routes)?)
            .bind(as_i64(next_version)?)
            .bind(as_i64(current_version)?)
            .fetch_one(&mut *transaction.tx)
            .await?;
            (
                next_version,
                receipt_from_row(command.provider_account_id, row).await?,
            )
        } else {
            if command.context.expected_aggregate_version.is_some() {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: AGGREGATE_TYPE.into(),
                    aggregate_id: command.provider_account_id,
                    expected: command.context.expected_aggregate_version,
                    actual: None,
                });
            }
            let row = sqlx::query(
                "INSERT INTO erpx_control.marketing_provider_accounts
                    (provider_account_id, provider_account_ref, tenant_id, holding_ref,
                     holding_id, verifier_key_ref, state, version, provider_key,
                     provider_priority, outbound_credential_ref, onboarding_state,
                     channels, channel_routes)
                 VALUES ($1, $2, $3, $4, $5, $6, 'suspended', 1, $7, $8, $9, 'pending', $10, $11)
                 RETURNING provider_account_ref, tenant_id, holding_ref, provider_key,
                           provider_priority, channels, state, onboarding_state, version, updated_at",
            )
            .bind(command.provider_account_id)
            .bind(&command.provider_account_ref)
            .bind(&command.tenant_id)
            .bind(&command.holding_ref)
            .bind(command.holding_id)
            .bind(&command.verifier_key_ref)
            .bind(&command.provider_key)
            .bind(i32::try_from(command.provider_priority).map_err(|_| {
                PersistenceError::InvalidData("provider priority exceeds database range".into())
            })?)
            .bind(&command.outbound_credential_ref)
            .bind(&command.channels)
            .bind(serde_json::to_value(&command.channel_routes)?)
            .fetch_one(&mut *transaction.tx)
            .await?;
            (1, receipt_from_row(command.provider_account_id, row).await?)
        };

        let receipt = persist_change(
            &mut transaction,
            &command.context,
            command.provider_account_id,
            version,
            "marketing.provider_account.configured",
            serde_json::json!({
                "provider_account_id": command.provider_account_id,
                "provider_account_ref": receipt.provider_account_ref,
                "tenant_id": command.tenant_id,
                "holding_ref": command.holding_ref,
                "provider_key": receipt.provider_key,
                "provider_priority": receipt.provider_priority,
                "channels": receipt.channels,
                "channel_routes": command.channel_routes,
                "state": receipt.state,
                "onboarding_state": receipt.onboarding_state,
                "version": version,
            }),
            "marketing.provider.account.configured",
            &receipt,
        )
        .await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    pub async fn acknowledge_verification(
        &self,
        command: AcknowledgeMarketingProviderVerificationCommand,
    ) -> Result<MarketingProviderAccountWriteReceipt, PersistenceError> {
        validate_verification_command(&command)?;
        let request_hash = request_hash("verify", &command)?;
        let mut transaction = self.control.begin_write().await?;
        let reservation = transaction
            .reserve_idempotency(
                &command.context.idempotency_key,
                &request_hash,
                Some(command.context.command_id),
            )
            .await?;
        if let Some(receipt) =
            replayed_receipt(&mut transaction, reservation, command.provider_account_id).await?
        {
            transaction.commit().await?;
            return Ok(receipt);
        }
        let row = account_for_update(
            &mut transaction,
            command.provider_account_id,
            &command.tenant_id,
            &command.holding_ref,
        )
        .await?;
        validate_account_holding_context(&command.context, row.try_get("holding_id")?)?;
        let current_version = positive_version(row.try_get("version")?)?;
        require_expected_version(
            &command.context,
            command.provider_account_id,
            current_version,
        )?;
        let next_version = current_version.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("provider account version overflow".into())
        })?;
        let verification_hash =
            hex::encode(Sha256::digest(command.verification_reference.as_bytes()));
        let row = sqlx::query(
            "UPDATE erpx_control.marketing_provider_accounts
             SET onboarding_state = 'verified', state = $2, last_verified_at = now(),
                 verification_reference_hash = $3, version = $4, updated_at = now()
             WHERE provider_account_id = $1 AND version = $5
             RETURNING provider_account_ref, tenant_id, holding_ref, provider_key,
                       provider_priority, channels, state, onboarding_state, version, updated_at",
        )
        .bind(command.provider_account_id)
        .bind(if command.activate {
            "active"
        } else {
            "suspended"
        })
        .bind(&verification_hash)
        .bind(as_i64(next_version)?)
        .bind(as_i64(current_version)?)
        .fetch_one(&mut *transaction.tx)
        .await?;
        let receipt = receipt_from_row(command.provider_account_id, row).await?;
        let receipt = persist_change(
            &mut transaction,
            &command.context,
            command.provider_account_id,
            next_version,
            "marketing.provider_account.verified",
            serde_json::json!({
                "provider_account_id": command.provider_account_id,
                "provider_account_ref": receipt.provider_account_ref,
                "tenant_id": command.tenant_id,
                "holding_ref": command.holding_ref,
                "verification_reference_hash": verification_hash,
                "channels": receipt.channels,
                "state": receipt.state,
                "onboarding_state": receipt.onboarding_state,
                "version": next_version,
            }),
            "marketing.provider.account.verified",
            &receipt,
        )
        .await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    pub async fn set_state(
        &self,
        command: SetMarketingProviderAccountStateCommand,
    ) -> Result<MarketingProviderAccountWriteReceipt, PersistenceError> {
        validate_state_command(&command)?;
        let request_hash = request_hash("state", &command)?;
        let mut transaction = self.control.begin_write().await?;
        let reservation = transaction
            .reserve_idempotency(
                &command.context.idempotency_key,
                &request_hash,
                Some(command.context.command_id),
            )
            .await?;
        if let Some(receipt) =
            replayed_receipt(&mut transaction, reservation, command.provider_account_id).await?
        {
            transaction.commit().await?;
            return Ok(receipt);
        }
        let row = account_for_update(
            &mut transaction,
            command.provider_account_id,
            &command.tenant_id,
            &command.holding_ref,
        )
        .await?;
        validate_account_holding_context(&command.context, row.try_get("holding_id")?)?;
        let current_version = positive_version(row.try_get("version")?)?;
        let onboarding_state: String = row.try_get("onboarding_state")?;
        require_expected_version(
            &command.context,
            command.provider_account_id,
            current_version,
        )?;
        if command.active && onboarding_state != "verified" {
            return Err(PersistenceError::InvalidData(
                "provider account must be verified before activation".into(),
            ));
        }
        let next_version = current_version.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("provider account version overflow".into())
        })?;
        let row = sqlx::query(
            "UPDATE erpx_control.marketing_provider_accounts
             SET state = $2, version = $3, updated_at = now()
             WHERE provider_account_id = $1 AND version = $4
             RETURNING provider_account_ref, tenant_id, holding_ref, provider_key,
                       provider_priority, channels, state, onboarding_state, version, updated_at",
        )
        .bind(command.provider_account_id)
        .bind(if command.active {
            "active"
        } else {
            "suspended"
        })
        .bind(as_i64(next_version)?)
        .bind(as_i64(current_version)?)
        .fetch_one(&mut *transaction.tx)
        .await?;
        let receipt = receipt_from_row(command.provider_account_id, row).await?;
        let receipt = persist_change(
            &mut transaction,
            &command.context,
            command.provider_account_id,
            next_version,
            "marketing.provider_account.state_changed",
            serde_json::json!({
                "provider_account_id": command.provider_account_id,
                "provider_account_ref": receipt.provider_account_ref,
                "tenant_id": command.tenant_id,
                "holding_ref": command.holding_ref,
                "state": receipt.state,
                "onboarding_state": receipt.onboarding_state,
                "version": next_version,
            }),
            "marketing.provider.account.state_changed",
            &receipt,
        )
        .await?;
        transaction.commit().await?;
        Ok(receipt)
    }
}

async fn ensure_active_holding(
    transaction: &mut ControlPlaneWriteTransaction<'_>,
    tenant_id: &str,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let present = sqlx::query(
        "SELECT 1 FROM erpx_control.tenant_registry
         WHERE tenant_id = $1 AND holding_ref = $2 AND state = 'active'",
    )
    .bind(tenant_id)
    .bind(holding_ref)
    .fetch_optional(&mut *transaction.tx)
    .await?
    .is_some();
    if present {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "tenant holding is not active".into(),
        ))
    }
}

async fn account_for_update(
    transaction: &mut ControlPlaneWriteTransaction<'_>,
    account_id: Uuid,
    tenant_id: &str,
    holding_ref: &str,
) -> Result<sqlx::postgres::PgRow, PersistenceError> {
    sqlx::query(
        "SELECT provider_account_ref, tenant_id, holding_ref, holding_id, provider_key,
                channels, state, onboarding_state, version
         FROM erpx_control.marketing_provider_accounts
         WHERE provider_account_id = $1 AND tenant_id = $2 AND holding_ref = $3
         FOR UPDATE",
    )
    .bind(account_id)
    .bind(tenant_id)
    .bind(holding_ref)
    .fetch_optional(&mut *transaction.tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "marketing_provider_account",
        id: account_id,
    })
}

fn validate_account_holding_context(
    context: &erpx_domain::contracts::CommandContext,
    account_holding_id: Uuid,
) -> Result<(), PersistenceError> {
    match &context.tenant_scope {
        erpx_domain::contracts::TenantScope::Holding { holding_id }
            if *holding_id == account_holding_id =>
        {
            Ok(())
        }
        _ => Err(PersistenceError::InvalidData(
            "provider account command scope is not bound to its holding".into(),
        )),
    }
}

fn validate_verification_command(
    command: &AcknowledgeMarketingProviderVerificationCommand,
) -> Result<(), PersistenceError> {
    let normalized = AcknowledgeMarketingProviderVerificationCommand::new(
        command.provider_account_id,
        command.tenant_id.clone(),
        command.holding_ref.clone(),
        command.verification_reference.clone(),
        command.activate,
        command.context.clone(),
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if normalized != *command {
        return Err(PersistenceError::InvalidData(
            "provider verification command is not canonically normalized".into(),
        ));
    }
    validate_verification_reference(&command.verification_reference)
}

fn validate_state_command(
    command: &SetMarketingProviderAccountStateCommand,
) -> Result<(), PersistenceError> {
    let normalized = SetMarketingProviderAccountStateCommand::new(
        command.provider_account_id,
        command.tenant_id.clone(),
        command.holding_ref.clone(),
        command.active,
        command.context.clone(),
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if normalized != *command {
        return Err(PersistenceError::InvalidData(
            "provider state command is not canonically normalized".into(),
        ));
    }
    Ok(())
}

fn require_expected_version(
    context: &erpx_domain::contracts::CommandContext,
    aggregate_id: Uuid,
    actual: u64,
) -> Result<(), PersistenceError> {
    if context.expected_aggregate_version != Some(actual) {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id,
            expected: context.expected_aggregate_version,
            actual: Some(actual),
        });
    }
    Ok(())
}

fn positive_version(value: i64) -> Result<u64, PersistenceError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| PersistenceError::InvalidData("provider account version is invalid".into()))
}

fn validate_channel_routes(
    routes: &[erpx_application::MarketingProviderChannelRoute],
    channels: &[String],
) -> Result<(), PersistenceError> {
    if routes.len() != channels.len()
        || routes
            .windows(2)
            .any(|window| window[0].channel == window[1].channel)
        || routes
            .iter()
            .any(|route| !channels.iter().any(|channel| channel == &route.channel))
    {
        return Err(PersistenceError::InvalidData(
            "provider channel routes do not match channels".into(),
        ));
    }
    for route in routes {
        validate_https_route(&route.endpoint)?;
        if let Some(endpoint) = route.reconciliation_endpoint.as_deref() {
            validate_https_route(endpoint)?;
        }
    }
    Ok(())
}

/// Application commands are also serde values at the transport boundary, so
/// callers can bypass the constructor that normally normalizes and validates
/// channel configuration. Reconstructing and comparing the command here keeps
/// the control-plane write authoritative even for deserialized input.
fn validate_configure_command(
    command: &ConfigureMarketingProviderAccountCommand,
) -> Result<(), PersistenceError> {
    let normalized = ConfigureMarketingProviderAccountCommand::new(
        command.provider_account_id,
        command.provider_account_ref.clone(),
        command.tenant_id.clone(),
        command.holding_ref.clone(),
        command.holding_id,
        command.provider_key.clone(),
        command.provider_priority,
        command.verifier_key_ref.clone(),
        command.outbound_credential_ref.clone(),
        command.channels.clone(),
        command.channel_routes.clone(),
        command.context.clone(),
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if normalized != *command {
        return Err(PersistenceError::InvalidData(
            "provider account configuration is not canonically normalized".into(),
        ));
    }
    validate_channel_routes(&command.channel_routes, &command.channels)
}

fn validate_https_route(value: &str) -> Result<(), PersistenceError> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| PersistenceError::InvalidData("provider channel route is invalid".into()))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(PersistenceError::InvalidData(
            "provider channel route must be a credential-free HTTPS URL".into(),
        ));
    }
    Ok(())
}

fn validate_verification_reference(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_VERIFICATION_REFERENCE_BYTES
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(PersistenceError::InvalidData(
            "provider verification reference is invalid".into(),
        ));
    }
    Ok(())
}

fn as_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| {
        PersistenceError::InvalidData("provider account version exceeds database range".into())
    })
}

fn positive_priority(value: i32) -> Result<u32, PersistenceError> {
    u32::try_from(value)
        .ok()
        .filter(|value| (1..=erpx_application::MAX_MARKETING_PROVIDER_PRIORITY).contains(value))
        .ok_or_else(|| PersistenceError::InvalidData("provider priority is invalid".into()))
}

fn default_provider_priority() -> u32 {
    erpx_application::DEFAULT_MARKETING_PROVIDER_PRIORITY
}

fn request_hash<T: Serialize>(operation: &str, command: &T) -> Result<String, PersistenceError> {
    let payload = serde_json::to_vec(&(operation, command))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredReceipt {
    provider_account_id: Uuid,
    provider_account_ref: String,
    tenant_id: String,
    holding_ref: String,
    provider_key: String,
    #[serde(default = "default_provider_priority")]
    provider_priority: u32,
    channels: Vec<String>,
    state: String,
    onboarding_state: String,
    version: u64,
    changed_at: DateTime<Utc>,
}

async fn replayed_receipt(
    transaction: &mut ControlPlaneWriteTransaction<'_>,
    reservation: crate::IdempotencyReservation,
    expected_account_id: Uuid,
) -> Result<Option<MarketingProviderAccountWriteReceipt>, PersistenceError> {
    let state = match reservation.state {
        crate::IdempotencyState::Completed { .. } => true,
        crate::IdempotencyState::Reserved => false,
        crate::IdempotencyState::InProgress { .. } => {
            return Err(PersistenceError::IdempotencyConflict {
                key: reservation.key,
            });
        }
    };
    if !state {
        return Ok(None);
    }
    let command_receipt = transaction
        .read_command_receipt(&reservation.key)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData("provider onboarding receipt is missing".into())
        })?;
    if command_receipt.aggregate_type != AGGREGATE_TYPE
        || command_receipt.aggregate_id != expected_account_id
        || command_receipt.aggregate_version == 0
    {
        return Err(PersistenceError::InvalidData(
            "provider onboarding receipt is bound to another aggregate".into(),
        ));
    }
    let stored: StoredReceipt = serde_json::from_value(command_receipt.response)?;
    if stored.provider_account_id != expected_account_id
        || stored.version != command_receipt.aggregate_version
    {
        return Err(PersistenceError::InvalidData(
            "provider onboarding receipt aggregate identity is invalid".into(),
        ));
    }
    Ok(Some(MarketingProviderAccountWriteReceipt {
        provider_account_id: stored.provider_account_id,
        provider_account_ref: stored.provider_account_ref,
        tenant_id: stored.tenant_id,
        holding_ref: stored.holding_ref,
        provider_key: stored.provider_key,
        provider_priority: stored.provider_priority,
        channels: stored.channels,
        state: stored.state,
        onboarding_state: stored.onboarding_state,
        version: stored.version,
        changed_at: stored.changed_at,
        replayed: true,
    }))
}

async fn receipt_from_row(
    account_id: Uuid,
    row: sqlx::postgres::PgRow,
) -> Result<MarketingProviderAccountWriteReceipt, PersistenceError> {
    Ok(MarketingProviderAccountWriteReceipt {
        provider_account_id: account_id,
        provider_account_ref: row.try_get("provider_account_ref")?,
        tenant_id: row.try_get("tenant_id")?,
        holding_ref: row.try_get("holding_ref")?,
        provider_key: row.try_get("provider_key")?,
        provider_priority: positive_priority(row.try_get("provider_priority")?)?,
        channels: row.try_get("channels")?,
        state: row.try_get("state")?,
        onboarding_state: row.try_get("onboarding_state")?,
        version: positive_version(row.try_get("version")?)?,
        changed_at: row.try_get("updated_at")?,
        replayed: false,
    })
}

#[allow(clippy::too_many_arguments)]
async fn persist_change(
    transaction: &mut ControlPlaneWriteTransaction<'_>,
    context: &erpx_domain::contracts::CommandContext,
    account_id: Uuid,
    version: u64,
    event_type: &str,
    payload: Value,
    audit_action: &str,
    receipt: &MarketingProviderAccountWriteReceipt,
) -> Result<MarketingProviderAccountWriteReceipt, PersistenceError> {
    let event_id = Uuid::new_v4();
    let event = OutboxEvent {
        id: event_id,
        aggregate_type: AGGREGATE_TYPE.into(),
        aggregate_id: account_id,
        aggregate_version: version,
        event_type: event_type.into(),
        payload,
        occurred_at: Utc::now(),
    };
    transaction
        .append_outbox_event(
            &event,
            &EventMetadata {
                tenant_scope: Some(serde_json::to_value(&context.tenant_scope)?),
                actor: Some(serde_json::to_value(&context.actor)?),
                correlation_id: Some(context.correlation_id),
                causation_id: context.causation_id,
                idempotency_key: Some(context.idempotency_key.clone()),
                event_version: EVENT_VERSION,
            },
        )
        .await?;
    transaction
        .append_audit_event(&AuditEvent {
            actor_ref: context.actor.principal_ref.clone(),
            customer_account_id: Uuid::parse_str(&receipt.tenant_id).ok(),
            action: audit_action.into(),
            purpose: context.audit.purpose.clone(),
            correlation_id: context.correlation_id,
        })
        .await?;
    let stored = StoredReceipt {
        provider_account_id: receipt.provider_account_id,
        provider_account_ref: receipt.provider_account_ref.clone(),
        tenant_id: receipt.tenant_id.clone(),
        holding_ref: receipt.holding_ref.clone(),
        provider_key: receipt.provider_key.clone(),
        provider_priority: receipt.provider_priority,
        channels: receipt.channels.clone(),
        state: receipt.state.clone(),
        onboarding_state: receipt.onboarding_state.clone(),
        version: receipt.version,
        changed_at: receipt.changed_at,
    };
    let command_receipt = erpx_application::CommandReceipt {
        command_id: context.command_id,
        aggregate_type: AGGREGATE_TYPE.into(),
        aggregate_id: account_id,
        aggregate_version: version,
        event_id,
        response: serde_json::to_value(stored)?,
        replayed: false,
    };
    transaction
        .complete_command(&context.idempotency_key, &command_receipt)
        .await?;
    Ok(receipt.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, TenantScope};

    fn configure_command() -> ConfigureMarketingProviderAccountCommand {
        let holding_id = Uuid::new_v4();
        let context = erpx_domain::contracts::CommandContext::try_new(
            "provider-config-boundary",
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
        .expect("valid command context");
        ConfigureMarketingProviderAccountCommand::new(
            Uuid::new_v4(),
            "provider-account-boundary",
            "tenant-boundary",
            "holding-boundary",
            holding_id,
            "provider-boundary",
            100,
            "env:VERIFY_PROVIDER_BOUNDARY",
            "env:SEND_PROVIDER_BOUNDARY",
            ["email".into(), "sms".into()],
            [
                erpx_application::MarketingProviderChannelRoute::new(
                    "email",
                    "https://email.provider.test/send",
                    None,
                )
                .expect("email route"),
                erpx_application::MarketingProviderChannelRoute::new(
                    "sms",
                    "https://sms.provider.test/send",
                    None,
                )
                .expect("sms route"),
            ],
            context,
        )
        .expect("valid provider account command")
    }

    #[test]
    fn request_hash_is_stable_and_does_not_print_payload() {
        let hash = request_hash("operation", &serde_json::json!({"key": "value"})).unwrap();
        assert_eq!(hash.len(), 64);
        assert!(!format!("{hash:?}").contains("value"));
    }

    #[test]
    fn configure_boundary_rejects_deserialized_unsupported_or_noncanonical_channels() {
        let valid = configure_command();
        assert!(validate_configure_command(&valid).is_ok());

        let mut unsupported_json = serde_json::to_value(&valid).expect("serialize command");
        unsupported_json["channels"] = serde_json::json!(["fax", "email"]);
        unsupported_json["channel_routes"][0]["channel"] = serde_json::json!("fax");
        let unsupported: ConfigureMarketingProviderAccountCommand =
            serde_json::from_value(unsupported_json).expect("deserialize command");
        assert!(validate_configure_command(&unsupported).is_err());

        let mut noncanonical = valid;
        noncanonical.channels.reverse();
        assert!(validate_configure_command(&noncanonical).is_err());
    }

    #[test]
    fn verification_reference_is_bounded_and_canonical() {
        assert!(validate_verification_reference("provider-verification-1").is_ok());
        assert!(validate_verification_reference("").is_err());
        assert!(validate_verification_reference("provider verification").is_err());
        assert!(validate_verification_reference("provider\nverification").is_err());
        assert!(
            validate_verification_reference(&"x".repeat(MAX_VERIFICATION_REFERENCE_BYTES + 1))
                .is_err()
        );
    }

    #[test]
    fn deserialized_verification_and_state_commands_are_revalidated() {
        let holding_id = Uuid::new_v4();
        let context = erpx_domain::contracts::CommandContext::try_new(
            "provider-command-boundary",
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
        .expect("valid command context");
        let mut verification = AcknowledgeMarketingProviderVerificationCommand::new(
            Uuid::new_v4(),
            "tenant-boundary",
            "holding-boundary",
            "provider-verification-1",
            true,
            context.clone(),
        )
        .expect("valid verification command");
        verification.verification_reference = "provider verification".into();
        assert!(validate_verification_command(&verification).is_err());

        let mut state = SetMarketingProviderAccountStateCommand::new(
            Uuid::new_v4(),
            "tenant-boundary",
            "holding-boundary",
            false,
            context.clone(),
        )
        .expect("valid state command");
        state.holding_ref = "holding/boundary".into();
        assert!(validate_state_command(&state).is_err());
        assert!(validate_account_holding_context(&context, Uuid::new_v4()).is_err());
    }
}
