//! Directory freshness and append-only revocation contracts.
//!
//! A directory feed is an authorization dependency.  When the latest
//! observed source state is more than five minutes old, access is denied even
//! if the last known user row was active.  A disable/delete event is recorded
//! as a durable revocation fact and SLA row; it is never handled by deleting
//! an identity link or rewriting role history.
//!
//! This file is a registration-ready child module.  SQLx persistence uses the
//! existing identity write-side tables, while the coordinator owns module
//! registration, migration numbering, scheduler/projector wiring, API, UI,
//! and shared authorization.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use super::sync::ImmutableIdentityLink;

pub const DIRECTORY_REVOCATION_CONTRACT_VERSION: u16 = 1;
pub const DIRECTORY_REVOCATION_SLA: Duration = Duration::minutes(5);
pub const MAX_DIRECTORY_REVOCATION_REASON_BYTES: usize = 256;
pub const MAX_DIRECTORY_REVOCATION_EXTERNAL_ID_BYTES: usize = 512;
pub const MAX_DIRECTORY_REVOCATION_IDEMPOTENCY_BYTES: usize = 512;
pub const MAX_DIRECTORY_REVOCATION_FACT_CHAIN: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevocationGate {
    Allowed,
    Revoked,
    Stale,
    FutureDated,
}

/// The timestamp is source-observed time, not local worker receipt time.  A
/// future-dated source timestamp is denied as a clock/protocol integrity
/// failure rather than extending the authorization window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryFeedFreshness {
    observed_at: DateTime<Utc>,
}

impl DirectoryFeedFreshness {
    pub const fn new(observed_at: DateTime<Utc>) -> Self {
        Self { observed_at }
    }

    pub const fn observed_at(self) -> DateTime<Utc> {
        self.observed_at
    }

    pub fn age_at(self, now: DateTime<Utc>) -> Option<Duration> {
        (now >= self.observed_at).then(|| now.signed_duration_since(self.observed_at))
    }

    pub fn gate(self, now: DateTime<Utc>) -> RevocationGate {
        if now < self.observed_at {
            return RevocationGate::FutureDated;
        }
        if now.signed_duration_since(self.observed_at) > DIRECTORY_REVOCATION_SLA {
            RevocationGate::Stale
        } else {
            RevocationGate::Allowed
        }
    }

    pub fn is_fresh(self, now: DateTime<Utc>) -> bool {
        self.gate(now) == RevocationGate::Allowed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRevocationState {
    connector_id: Uuid,
    external_id: String,
    identity_link: ImmutableIdentityLink,
    active: bool,
    observed_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    source_revision: u64,
}

impl DirectoryRevocationState {
    pub fn try_new(
        connector_id: Uuid,
        external_id: impl Into<String>,
        identity_link: ImmutableIdentityLink,
        active: bool,
        observed_at: DateTime<Utc>,
        revoked_at: Option<DateTime<Utc>>,
        source_revision: u64,
    ) -> Result<Self, DirectoryRevocationError> {
        if connector_id.is_nil() {
            return Err(DirectoryRevocationError::NilIdentifier {
                field: "connector_id",
            });
        }
        let external_id = bounded_text(
            external_id.into(),
            "external_id",
            MAX_DIRECTORY_REVOCATION_EXTERNAL_ID_BYTES,
        )?;
        if source_revision == 0 {
            return Err(DirectoryRevocationError::InvalidSourceRevision);
        }
        if revoked_at.is_some() && active {
            return Err(DirectoryRevocationError::InvalidState);
        }
        if !active && revoked_at.is_none() {
            return Err(DirectoryRevocationError::InvalidState);
        }
        Ok(Self {
            connector_id,
            external_id,
            identity_link,
            active,
            observed_at,
            revoked_at,
            source_revision,
        })
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    pub fn identity_link(&self) -> &ImmutableIdentityLink {
        &self.identity_link
    }

    pub const fn active(&self) -> bool {
        self.active
    }

    pub const fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }

    pub const fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }

    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    pub fn gate(&self, now: DateTime<Utc>) -> RevocationGate {
        if !self.active || self.revoked_at.is_some() {
            RevocationGate::Revoked
        } else {
            DirectoryFeedFreshness::new(self.observed_at).gate(now)
        }
    }

    pub fn authorization_allowed(&self, now: DateTime<Utc>) -> bool {
        self.gate(now) == RevocationGate::Allowed
    }
}

#[repr(i16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRevocationReason {
    AccountDisabled = 1,
    AccountDeleted = 2,
    FeedStale = 3,
    ConnectorDisabled = 4,
    ManualEmergency = 5,
    Reconciliation = 6,
}

impl DirectoryRevocationReason {
    pub const fn as_code(self) -> i16 {
        self as i16
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccountDisabled => "account_disabled",
            Self::AccountDeleted => "account_deleted",
            Self::FeedStale => "feed_stale",
            Self::ConnectorDisabled => "connector_disabled",
            Self::ManualEmergency => "manual_emergency",
            Self::Reconciliation => "reconciliation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRevocationFactInput {
    pub connector_id: Uuid,
    pub external_id: String,
    pub identity_link: ImmutableIdentityLink,
    pub reason: DirectoryRevocationReason,
    pub source_revision: u64,
    pub idempotency_key: String,
    pub observed_at: DateTime<Utc>,
}

/// Immutable source revocation fact.  The sequence/hash pair is scoped to a
/// connector and immutable identity link, so replay cannot replace a prior
/// disable/delete with a mutable-name update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRevocationFact {
    event_id: Uuid,
    connector_id: Uuid,
    external_id: String,
    identity_link: ImmutableIdentityLink,
    reason: DirectoryRevocationReason,
    source_revision: u64,
    sequence: u64,
    previous_hash: Option<String>,
    fact_hash: String,
    idempotency_key: String,
    observed_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
}

impl DirectoryRevocationFact {
    fn from_input(
        input: DirectoryRevocationFactInput,
        sequence: u64,
        previous_hash: Option<String>,
    ) -> Result<Self, DirectoryRevocationError> {
        if input.connector_id.is_nil() {
            return Err(DirectoryRevocationError::NilIdentifier {
                field: "connector_id",
            });
        }
        if sequence == 0 {
            return Err(DirectoryRevocationError::InvalidSequence);
        }
        if input.source_revision == 0 {
            return Err(DirectoryRevocationError::InvalidSourceRevision);
        }
        let external_id = bounded_text(
            input.external_id,
            "external_id",
            MAX_DIRECTORY_REVOCATION_EXTERNAL_ID_BYTES,
        )?;
        let idempotency_key = bounded_text(
            input.idempotency_key,
            "idempotency_key",
            MAX_DIRECTORY_REVOCATION_IDEMPOTENCY_BYTES,
        )?;
        let mut digest = Sha256::new();
        digest.update(b"erpx-directory-revocation-fact:v1\0");
        digest.update(input.connector_id.as_bytes());
        digest.update(external_id.as_bytes());
        digest.update(input.identity_link.fingerprint());
        digest.update(input.reason.as_code().to_be_bytes());
        digest.update(input.source_revision.to_be_bytes());
        digest.update(sequence.to_be_bytes());
        digest.update(idempotency_key.as_bytes());
        if let Some(previous_hash) = previous_hash.as_deref() {
            digest.update(previous_hash.as_bytes());
        }
        digest.update(
            input
                .observed_at
                .timestamp_nanos_opt()
                .unwrap_or_default()
                .to_be_bytes(),
        );
        let fact_hash = hex_lower(&digest.finalize());
        let event_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, idempotency_key.as_bytes());
        let deadline = input
            .observed_at
            .checked_add_signed(DIRECTORY_REVOCATION_SLA)
            .ok_or(DirectoryRevocationError::InvalidRevocationSchedule)?;
        Ok(Self {
            event_id,
            connector_id: input.connector_id,
            external_id,
            identity_link: input.identity_link,
            reason: input.reason,
            source_revision: input.source_revision,
            sequence,
            previous_hash,
            fact_hash,
            idempotency_key,
            observed_at: input.observed_at,
            deadline,
        })
    }

    pub fn event_id(&self) -> Uuid {
        self.event_id
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    pub fn identity_link(&self) -> &ImmutableIdentityLink {
        &self.identity_link
    }

    pub const fn reason(&self) -> DirectoryRevocationReason {
        self.reason
    }

    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn previous_hash(&self) -> Option<&str> {
        self.previous_hash.as_deref()
    }

    pub fn fact_hash(&self) -> &str {
        &self.fact_hash
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub const fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }

    pub const fn deadline(&self) -> DateTime<Utc> {
        self.deadline
    }

    pub fn is_late_at(&self, now: DateTime<Utc>) -> bool {
        now > self.deadline
    }

    pub fn chain_key(&self) -> String {
        format!(
            "{}/{}",
            self.connector_id,
            self.identity_link.database_key()
        )
    }

    pub fn verify_hash(&self) -> bool {
        let input = DirectoryRevocationFactInput {
            connector_id: self.connector_id,
            external_id: self.external_id.clone(),
            identity_link: self.identity_link.clone(),
            reason: self.reason,
            source_revision: self.source_revision,
            idempotency_key: self.idempotency_key.clone(),
            observed_at: self.observed_at,
        };
        Self::from_input(input, self.sequence, self.previous_hash.clone()).is_ok_and(|candidate| {
            candidate.fact_hash == self.fact_hash && candidate.event_id == self.event_id
        })
    }

    pub fn outbox_event(&self) -> Result<DirectoryRevocationOutboxEvent, DirectoryRevocationError> {
        let aggregate_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, self.chain_key().as_bytes());
        Ok(DirectoryRevocationOutboxEvent {
            event_id: self.event_id,
            event_type: "directory.identity_revoked.v1".to_owned(),
            aggregate_type: "directory_identity".to_owned(),
            aggregate_id,
            aggregate_version: self.sequence,
            connector_id: self.connector_id,
            idempotency_key: self.idempotency_key.clone(),
            payload: serde_json::to_value(self)
                .map_err(|_| DirectoryRevocationError::EventEncodingFailed)?,
            occurred_at: self.observed_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRevocationAppendOutcome {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Default, Clone)]
pub struct DirectoryRevocationLedger {
    facts: Vec<DirectoryRevocationFact>,
    by_idempotency: HashMap<String, usize>,
}

impl DirectoryRevocationLedger {
    pub fn append(
        &mut self,
        input: DirectoryRevocationFactInput,
    ) -> Result<(DirectoryRevocationAppendOutcome, DirectoryRevocationFact), DirectoryRevocationError>
    {
        let idempotency_key = bounded_text(
            input.idempotency_key.clone(),
            "idempotency_key",
            MAX_DIRECTORY_REVOCATION_IDEMPOTENCY_BYTES,
        )?;
        if let Some(index) = self.by_idempotency.get(&idempotency_key) {
            let existing = &self.facts[*index];
            let candidate = DirectoryRevocationFact::from_input(
                input.clone(),
                existing.sequence(),
                existing.previous_hash.clone(),
            )?;
            if candidate.fact_hash() != existing.fact_hash()
                || candidate.event_id() != existing.event_id()
            {
                return Err(DirectoryRevocationError::ImmutableConflict);
            }
            return Ok((
                DirectoryRevocationAppendOutcome::AlreadyApplied,
                existing.clone(),
            ));
        }
        let chain_key = format!(
            "{}/{}",
            input.connector_id,
            input.identity_link.database_key()
        );
        let chain_length = self
            .facts
            .iter()
            .filter(|fact| fact.chain_key() == chain_key)
            .count();
        if chain_length >= MAX_DIRECTORY_REVOCATION_FACT_CHAIN {
            return Err(DirectoryRevocationError::FactChainTooLong);
        }
        let prior = self
            .facts
            .iter()
            .filter(|fact| fact.chain_key() == chain_key)
            .max_by_key(|fact| fact.sequence());
        if prior.is_some_and(|fact| input.source_revision <= fact.source_revision()) {
            return Err(DirectoryRevocationError::SourceRevisionRegression);
        }
        let (sequence, previous_hash) = prior.map_or((1, None), |fact| {
            (
                fact.sequence().saturating_add(1),
                Some(fact.fact_hash().to_owned()),
            )
        });
        if sequence == u64::MAX {
            return Err(DirectoryRevocationError::FactChainTooLong);
        }
        let fact = DirectoryRevocationFact::from_input(input, sequence, previous_hash)?;
        self.by_idempotency
            .insert(fact.idempotency_key.clone(), self.facts.len());
        self.facts.push(fact.clone());
        Ok((DirectoryRevocationAppendOutcome::Applied, fact))
    }

    pub fn facts(&self) -> &[DirectoryRevocationFact] {
        &self.facts
    }

    pub fn verify(&self) -> Result<(), DirectoryRevocationError> {
        let mut chains: HashMap<String, (&DirectoryRevocationFact, u64)> = HashMap::new();
        for fact in &self.facts {
            if !fact.verify_hash() {
                return Err(DirectoryRevocationError::FactHashInvalid);
            }
            let entry = chains.entry(fact.chain_key()).or_insert((fact, 0));
            if entry.1 == 0 && fact.previous_hash().is_some() {
                return Err(DirectoryRevocationError::HashChainBroken);
            }
            if entry.1 + 1 != fact.sequence() {
                return Err(DirectoryRevocationError::SequenceGap);
            }
            if entry.1 > 0 && fact.previous_hash() != Some(entry.0.fact_hash()) {
                return Err(DirectoryRevocationError::HashChainBroken);
            }
            *entry = (fact, fact.sequence());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRevocationOutboxEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub connector_id: Uuid,
    pub idempotency_key: String,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DirectoryRevocationReadModel {
    states: BTreeMap<String, DirectoryRevocationState>,
    applied_events: BTreeMap<Uuid, ()>,
    latest_facts: BTreeMap<String, (u64, String)>,
}

impl DirectoryRevocationReadModel {
    pub fn apply(
        &mut self,
        fact: &DirectoryRevocationFact,
    ) -> Result<(), DirectoryRevocationError> {
        if self.applied_events.contains_key(&fact.event_id()) {
            return Ok(());
        }
        if !fact.verify_hash() {
            return Err(DirectoryRevocationError::FactHashInvalid);
        }
        let chain_key = fact.chain_key();
        if let Some((latest_sequence, latest_hash)) = self.latest_facts.get(&chain_key) {
            if fact.sequence() < *latest_sequence {
                self.applied_events.insert(fact.event_id(), ());
                return Ok(());
            }
            if fact.sequence() == *latest_sequence {
                if fact.fact_hash() != latest_hash {
                    return Err(DirectoryRevocationError::HashChainBroken);
                }
                self.applied_events.insert(fact.event_id(), ());
                return Ok(());
            }
            if fact.sequence() != latest_sequence.saturating_add(1) {
                return Err(DirectoryRevocationError::SequenceGap);
            }
            if fact.previous_hash() != Some(latest_hash.as_str()) {
                return Err(DirectoryRevocationError::HashChainBroken);
            }
        } else if fact.sequence() != 1 || fact.previous_hash().is_some() {
            return Err(DirectoryRevocationError::SequenceGap);
        }
        let key = format!(
            "{}/{}",
            fact.connector_id(),
            fact.identity_link().database_key()
        );
        if self
            .states
            .get(&key)
            .is_some_and(|state| fact.source_revision() <= state.source_revision())
        {
            return Err(DirectoryRevocationError::SourceRevisionRegression);
        }
        let state = DirectoryRevocationState::try_new(
            fact.connector_id(),
            fact.external_id().to_owned(),
            fact.identity_link().clone(),
            false,
            fact.observed_at(),
            Some(fact.observed_at()),
            fact.source_revision(),
        )?;
        self.states.insert(key, state);
        self.latest_facts
            .insert(chain_key, (fact.sequence(), fact.fact_hash().to_owned()));
        self.applied_events.insert(fact.event_id(), ());
        Ok(())
    }

    pub fn state_for(
        &self,
        connector_id: Uuid,
        identity_link: &ImmutableIdentityLink,
    ) -> Option<&DirectoryRevocationState> {
        let key = format!("{}/{}", connector_id, identity_link.database_key());
        self.states.get(&key)
    }

    pub fn applied_event_count(&self) -> usize {
        self.applied_events.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRevocationReconciliationReport {
    pub contract_version: u16,
    pub source_active_count: u64,
    pub source_revoked_count: u64,
    pub projected_revoked_count: u64,
    pub pending_sla_count: u64,
    pub breached_sla_count: u64,
    pub source_stable: bool,
    pub complete: bool,
    pub issues: Vec<DirectoryRevocationReconciliationIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRevocationReconciliationIssue {
    MissingRevocationFact,
    MissingOutboxEvent,
    ProjectionMismatch,
    StaleFeed,
    ImmutableLinkMismatch,
}

impl DirectoryRevocationReconciliationReport {
    pub fn is_reconciled(&self) -> bool {
        self.complete && self.source_stable && self.issues.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRevocationRollbackMode {
    BeforeCommit { import_id: Uuid },
    AfterCommit { connector_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRevocationRollbackPlan {
    mode: DirectoryRevocationRollbackMode,
    reason: String,
    preserve_facts: bool,
    preserve_identity_links: bool,
    disable_new_sessions: bool,
}

impl DirectoryRevocationRollbackPlan {
    pub fn before_commit(
        import_id: Uuid,
        reason: impl Into<String>,
    ) -> Result<Self, DirectoryRevocationError> {
        if import_id.is_nil() {
            return Err(DirectoryRevocationError::NilIdentifier { field: "import_id" });
        }
        Ok(Self {
            mode: DirectoryRevocationRollbackMode::BeforeCommit { import_id },
            reason: bounded_text(reason.into(), "rollback_reason", 1_024)?,
            preserve_facts: true,
            preserve_identity_links: true,
            disable_new_sessions: false,
        })
    }

    pub fn after_commit(
        connector_id: Uuid,
        reason: impl Into<String>,
    ) -> Result<Self, DirectoryRevocationError> {
        if connector_id.is_nil() {
            return Err(DirectoryRevocationError::NilIdentifier {
                field: "connector_id",
            });
        }
        Ok(Self {
            mode: DirectoryRevocationRollbackMode::AfterCommit { connector_id },
            reason: bounded_text(reason.into(), "rollback_reason", 1_024)?,
            preserve_facts: true,
            preserve_identity_links: true,
            disable_new_sessions: true,
        })
    }

    pub fn mode(&self) -> &DirectoryRevocationRollbackMode {
        &self.mode
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub const fn preserves_history(&self) -> bool {
        self.preserve_facts && self.preserve_identity_links
    }

    pub const fn disables_new_sessions(&self) -> bool {
        self.disable_new_sessions
    }
}

#[derive(Debug, Error)]
pub enum DirectoryRevocationError {
    #[error("{field} is empty, too long, or contains a control character")]
    InvalidReference { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("directory revocation state is inconsistent")]
    InvalidState,
    #[error("directory revocation sequence is invalid")]
    InvalidSequence,
    #[error("directory revocation sequence contains a gap")]
    SequenceGap,
    #[error("directory revocation fact hash is invalid")]
    FactHashInvalid,
    #[error("directory revocation hash chain is broken")]
    HashChainBroken,
    #[error("directory revocation fact chain exceeded its safety bound")]
    FactChainTooLong,
    #[error("directory revocation event could not be encoded")]
    EventEncodingFailed,
    #[error("directory revocation persistence failed: {0}")]
    Persistence(#[from] sqlx::Error),
    #[error("directory revocation persistence encountered an immutable conflict")]
    ImmutableConflict,
    #[error("directory identity was not found")]
    IdentityNotFound,
    #[error("directory immutable identity link is invalid")]
    ImmutableLinkInvalid,
    #[error("directory source revision is invalid")]
    InvalidSourceRevision,
    #[error("directory revocation deadline is outside the supported time range")]
    InvalidRevocationSchedule,
    #[error("directory revocation source revision regressed")]
    SourceRevisionRegression,
    #[error("directory revocation was not applied before its five-minute deadline")]
    SlaBreached,
    #[error("directory revocation five-minute deadline has not elapsed")]
    SlaNotDue,
}

#[derive(Debug, Clone)]
pub struct SqlxDirectoryRevocationStore {
    pool: PgPool,
}

impl SqlxDirectoryRevocationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Reads the write-side identity link and its disposable projection.  A
    /// missing projection is treated as a database error by the coordinator;
    /// authorization must not infer a healthy state from an absent row.
    pub async fn read_state(
        &self,
        connector_id: Uuid,
        external_id: &str,
    ) -> Result<DirectoryRevocationState, DirectoryRevocationError> {
        let row = sqlx::query(
            "SELECT l.connector_id, l.external_object_id, l.immutable_authority,
                    l.immutable_value, p.active, p.last_feed_at, p.revoked_at,
                    l.source_revision
             FROM erpx_identity.directory_identity_links l
             JOIN erpx_identity.directory_identity_projection p
               ON p.identity_link = l.identity_link
              AND p.connector_id = l.connector_id
             WHERE l.connector_id=$1 AND l.external_object_id=$2",
        )
        .bind(connector_id)
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(DirectoryRevocationError::IdentityNotFound)?;
        let authority: String = row.try_get("immutable_authority")?;
        let value: String = row.try_get("immutable_value")?;
        DirectoryRevocationState::try_new(
            row.try_get("connector_id")?,
            row.try_get::<String, _>("external_object_id")?,
            ImmutableIdentityLink::new(authority, value)
                .map_err(|_| DirectoryRevocationError::ImmutableLinkInvalid)?,
            row.try_get("active")?,
            row.try_get("last_feed_at")?,
            row.try_get("revoked_at")?,
            u64::try_from(row.try_get::<i64, _>("source_revision")?)
                .map_err(|_| DirectoryRevocationError::InvalidSourceRevision)?,
        )
    }

    pub async fn append_fact(
        &self,
        fact: &DirectoryRevocationFact,
    ) -> Result<DirectoryRevocationAppendOutcome, DirectoryRevocationError> {
        if !fact.verify_hash() {
            return Err(DirectoryRevocationError::FactHashInvalid);
        }
        if fact.sequence() > MAX_DIRECTORY_REVOCATION_FACT_CHAIN as u64 {
            return Err(DirectoryRevocationError::FactChainTooLong);
        }
        let event = fact.outbox_event()?;
        let mut tx = self.pool.begin().await?;
        // Protect the empty-chain case as well as the normal tail row. Two
        // concurrent first revocations must not both commit sequence 1.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(fact.chain_key())
            .execute(&mut *tx)
            .await?;
        let existing = sqlx::query(
            "SELECT connector_id, external_id, immutable_authority,
                    immutable_value, observed_at, deadline, reason
             FROM erpx_identity.directory_revocation_sla
             WHERE event_id=$1 FOR UPDATE",
        )
        .bind(fact.event_id())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = existing {
            let existing_payload = sqlx::query(
                "SELECT payload_json FROM erpx_identity.directory_outbox WHERE event_id=$1",
            )
            .bind(fact.event_id())
            .fetch_optional(&mut *tx)
            .await?
            .and_then(|row| row.try_get::<Value, _>("payload_json").ok())
            .and_then(|payload| serde_json::from_value::<DirectoryRevocationFact>(payload).ok());
            tx.rollback().await?;
            let same = row.try_get::<Uuid, _>("connector_id")? == fact.connector_id()
                && row.try_get::<String, _>("external_id")? == fact.external_id()
                && row.try_get::<String, _>("immutable_authority")?
                    == fact.identity_link().authority()
                && row.try_get::<String, _>("immutable_value")? == fact.identity_link().value()
                && row.try_get::<DateTime<Utc>, _>("observed_at")? == fact.observed_at()
                && row.try_get::<DateTime<Utc>, _>("deadline")? == fact.deadline()
                && row.try_get::<String, _>("reason")? == fact.reason().as_str()
                && existing_payload.as_ref() == Some(fact);
            return if same {
                Ok(DirectoryRevocationAppendOutcome::AlreadyApplied)
            } else {
                Err(DirectoryRevocationError::ImmutableConflict)
            };
        }
        // The authority/value pair is canonical. `identity_link` is a legacy
        // derived hash and may be NULL, but a populated hash must agree rather
        // than allowing either representation to authenticate the revocation.
        let identity_link = sqlx::query(
            "SELECT source_revision
             FROM erpx_identity.directory_identity_links
             WHERE connector_id=$1 AND external_object_id=$2
               AND immutable_authority=$4
               AND immutable_value=$5
               AND (identity_link IS NULL OR identity_link=$3)
             FOR SHARE",
        )
        .bind(fact.connector_id())
        .bind(fact.external_id())
        .bind(fact.identity_link().database_key())
        .bind(fact.identity_link().authority())
        .bind(fact.identity_link().value())
        .fetch_optional(&mut *tx)
        .await?;
        if identity_link.is_none() {
            tx.rollback().await?;
            return Err(DirectoryRevocationError::IdentityNotFound);
        }
        let current_source_revision = identity_link
            .as_ref()
            .map(|row| row.try_get::<i64, _>("source_revision"))
            .transpose()?
            .map(|revision| {
                u64::try_from(revision).map_err(|_| DirectoryRevocationError::InvalidSourceRevision)
            })
            .transpose()?;
        if current_source_revision.is_some_and(|revision| revision > fact.source_revision()) {
            tx.rollback().await?;
            return Err(DirectoryRevocationError::SourceRevisionRegression);
        }
        let prior_rows = sqlx::query(
            "SELECT payload_json
             FROM erpx_identity.directory_outbox
             WHERE aggregate_id=$1 AND aggregate_type='directory_identity'
             FOR UPDATE",
        )
        .bind(event.aggregate_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut tail: Option<DirectoryRevocationFact> = None;
        for row in prior_rows {
            let payload: Value = row.try_get("payload_json")?;
            let prior = serde_json::from_value::<DirectoryRevocationFact>(payload)
                .map_err(|_| DirectoryRevocationError::ImmutableConflict)?;
            if prior.chain_key() != fact.chain_key() || !prior.verify_hash() {
                return Err(DirectoryRevocationError::HashChainBroken);
            }
            if tail
                .as_ref()
                .is_none_or(|candidate| prior.sequence() > candidate.sequence())
            {
                tail = Some(prior);
            }
        }
        if let Some(tail) = tail {
            if fact.source_revision() <= tail.source_revision() {
                return Err(DirectoryRevocationError::SourceRevisionRegression);
            }
            let expected_sequence = tail
                .sequence()
                .checked_add(1)
                .ok_or(DirectoryRevocationError::FactChainTooLong)?;
            if fact.sequence() != expected_sequence {
                return Err(DirectoryRevocationError::SequenceGap);
            }
            if fact.previous_hash() != Some(tail.fact_hash()) {
                return Err(DirectoryRevocationError::HashChainBroken);
            }
        } else if fact.sequence() != 1 || fact.previous_hash().is_some() {
            return Err(DirectoryRevocationError::SequenceGap);
        }
        sqlx::query(
            "INSERT INTO erpx_identity.directory_revocation_sla
                (event_id, connector_id, external_id, immutable_authority,
                 immutable_value, observed_at, deadline, reason, status)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'pending')",
        )
        .bind(fact.event_id())
        .bind(fact.connector_id())
        .bind(fact.external_id())
        .bind(fact.identity_link().authority())
        .bind(fact.identity_link().value())
        .bind(fact.observed_at())
        .bind(fact.deadline())
        .bind(fact.reason().as_str())
        .execute(&mut *tx)
        .await?;
        let outbox_result = sqlx::query(
            "INSERT INTO erpx_identity.directory_outbox
                (event_id, aggregate_id, aggregate_type, event_type,
                 event_version, connector_id, aggregate_version, scope_json,
                 idempotency_key, payload_json, occurred_at)
             VALUES ($1,$2,$3,$4,1,$5,$6,'{}'::jsonb,$7,$8,$9)
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(event.event_id)
        .bind(event.aggregate_id)
        .bind(&event.aggregate_type)
        .bind(&event.event_type)
        .bind(event.connector_id)
        .bind(
            i64::try_from(event.aggregate_version)
                .map_err(|_| DirectoryRevocationError::InvalidSequence)?,
        )
        .bind(&event.idempotency_key)
        .bind(&event.payload)
        .bind(event.occurred_at)
        .execute(&mut *tx)
        .await?;
        if outbox_result.rows_affected() == 0 {
            let existing_payload = sqlx::query(
                "SELECT aggregate_id, aggregate_type, payload_json, occurred_at
                 FROM erpx_identity.directory_outbox WHERE event_id=$1",
            )
            .bind(event.event_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| {
                Ok::<bool, sqlx::Error>(
                    row.try_get::<Uuid, _>("aggregate_id")? == event.aggregate_id
                        && row.try_get::<String, _>("aggregate_type")? == event.aggregate_type
                        && row.try_get::<Value, _>("payload_json")? == event.payload
                        && row.try_get::<DateTime<Utc>, _>("occurred_at")? == event.occurred_at,
                )
            })
            .transpose()?;
            if existing_payload != Some(true) {
                tx.rollback().await?;
                return Err(DirectoryRevocationError::ImmutableConflict);
            }
        }
        tx.commit().await?;
        Ok(DirectoryRevocationAppendOutcome::Applied)
    }

    pub async fn scan_stale(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<DirectoryRevocationState>, DirectoryRevocationError> {
        let cutoff = now - DIRECTORY_REVOCATION_SLA;
        let rows = sqlx::query(
            "SELECT l.connector_id, l.external_object_id, l.immutable_authority,
                    l.immutable_value, p.active, p.last_feed_at, p.revoked_at,
                    l.source_revision
             FROM erpx_identity.directory_identity_links l
             JOIN erpx_identity.directory_identity_projection p
               ON p.identity_link=l.identity_link
              AND p.connector_id=l.connector_id
             WHERE p.active=TRUE AND p.revoked_at IS NULL AND p.last_feed_at < $1
             ORDER BY p.last_feed_at, l.connector_id, l.external_object_id",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let authority: String = row.try_get("immutable_authority")?;
                let value: String = row.try_get("immutable_value")?;
                DirectoryRevocationState::try_new(
                    row.try_get("connector_id")?,
                    row.try_get::<String, _>("external_object_id")?,
                    ImmutableIdentityLink::new(authority, value)
                        .map_err(|_| sqlx::Error::Protocol("invalid immutable link".into()))?,
                    row.try_get("active")?,
                    row.try_get("last_feed_at")?,
                    row.try_get("revoked_at")?,
                    u64::try_from(row.try_get::<i64, _>("source_revision")?)
                        .map_err(|_| sqlx::Error::Protocol("invalid source revision".into()))?,
                )
                .map_err(|error| match error {
                    DirectoryRevocationError::Persistence(error) => error,
                    _ => sqlx::Error::Protocol("invalid revocation state".into()),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(DirectoryRevocationError::Persistence)
    }

    pub async fn mark_breached(&self, event_id: Uuid) -> Result<(), DirectoryRevocationError> {
        self.mark_breached_at(event_id, Utc::now()).await
    }

    /// Marks a pending revocation as breached only after the strict
    /// five-minute boundary. The timestamp is explicit for deterministic
    /// workers and certification tests; callers cannot acknowledge or breach
    /// a fact early by supplying an arbitrary SQL-side state transition.
    pub async fn mark_breached_at(
        &self,
        event_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), DirectoryRevocationError> {
        sqlx::query(
            "UPDATE erpx_identity.directory_revocation_sla
             SET status='breached'
             WHERE event_id=$1 AND status='pending' AND $2 > deadline",
        )
        .bind(event_id)
        .bind(now)
        .execute(&self.pool)
        .await?;
        let state = sqlx::query(
            "SELECT status, deadline
             FROM erpx_identity.directory_revocation_sla
             WHERE event_id=$1",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(state) = state else {
            return Err(DirectoryRevocationError::IdentityNotFound);
        };
        let status: String = state.try_get("status")?;
        match status.as_str() {
            "breached" | "applied" => Ok(()),
            "pending" => {
                let deadline: DateTime<Utc> = state.try_get("deadline")?;
                if now <= deadline {
                    Err(DirectoryRevocationError::SlaNotDue)
                } else {
                    Err(DirectoryRevocationError::Persistence(
                        sqlx::Error::Protocol(
                            "revocation breach update raced with another writer".into(),
                        ),
                    ))
                }
            }
            _ => Err(DirectoryRevocationError::ImmutableConflict),
        }
    }

    /// Record the durable projection-application timestamp without allowing
    /// an acknowledgement after the five-minute deadline. The operation is
    /// idempotent for an already-applied row and never rewrites append-only
    /// revocation facts or their outbox event.
    pub async fn mark_applied(
        &self,
        event_id: Uuid,
        applied_at: DateTime<Utc>,
    ) -> Result<(), DirectoryRevocationError> {
        let result = sqlx::query(
            "UPDATE erpx_identity.directory_revocation_sla
             SET status='applied', applied_at=$2
             WHERE event_id=$1 AND status='pending' AND $2 <= deadline",
        )
        .bind(event_id)
        .bind(applied_at)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 0 {
            return Ok(());
        }
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM erpx_identity.directory_revocation_sla WHERE event_id=$1",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;
        match status.as_deref() {
            Some("applied") => Ok(()),
            Some("pending") => Err(DirectoryRevocationError::SlaBreached),
            Some("breached") | None => Err(DirectoryRevocationError::SlaBreached),
            Some(_) => Err(DirectoryRevocationError::ImmutableConflict),
        }
    }
}

impl DirectoryIdentityLinkDatabaseKey for ImmutableIdentityLink {
    fn database_key(&self) -> String {
        hex_lower(&self.fingerprint())
    }
}

trait DirectoryIdentityLinkDatabaseKey {
    fn database_key(&self) -> String;
}

fn bounded_text(
    value: String,
    field: &'static str,
    max: usize,
) -> Result<String, DirectoryRevocationError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(DirectoryRevocationError::InvalidReference { field });
    }
    Ok(value)
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0)
            .single()
            .expect("fixed timestamp")
    }

    fn link() -> ImmutableIdentityLink {
        ImmutableIdentityLink::new("ad:object_guid", "guid-1").expect("link")
    }

    fn input(key: &str, reason: DirectoryRevocationReason) -> DirectoryRevocationFactInput {
        DirectoryRevocationFactInput {
            connector_id: Uuid::from_u128(1),
            external_id: "external-1".to_owned(),
            identity_link: link(),
            reason,
            source_revision: 7,
            idempotency_key: key.to_owned(),
            observed_at: at(),
        }
    }

    #[test]
    fn exactly_five_minutes_is_the_boundary_and_future_feed_fails_closed() {
        let fresh = DirectoryFeedFreshness::new(at());
        assert_eq!(
            fresh.gate(at() + DIRECTORY_REVOCATION_SLA),
            RevocationGate::Allowed
        );
        assert_eq!(
            fresh.gate(at() + DIRECTORY_REVOCATION_SLA + Duration::nanoseconds(1)),
            RevocationGate::Stale
        );
        assert_eq!(
            fresh.gate(at() - Duration::seconds(1)),
            RevocationGate::FutureDated
        );
    }

    #[test]
    fn disabled_and_stale_states_are_denied_even_with_a_recent_or_old_feed() {
        assert!(matches!(
            DirectoryRevocationState::try_new(
                Uuid::from_u128(1),
                "external-1",
                link(),
                false,
                at(),
                None,
                1,
            ),
            Err(DirectoryRevocationError::InvalidState)
        ));
        let disabled = DirectoryRevocationState::try_new(
            Uuid::from_u128(1),
            "external-1",
            link(),
            false,
            at(),
            Some(at()),
            1,
        )
        .expect("state");
        assert_eq!(disabled.gate(at()), RevocationGate::Revoked);
        let stale = DirectoryRevocationState::try_new(
            Uuid::from_u128(1),
            "external-1",
            link(),
            true,
            at() - DIRECTORY_REVOCATION_SLA - Duration::seconds(1),
            None,
            1,
        )
        .expect("state");
        assert_eq!(stale.gate(at()), RevocationGate::Stale);
        assert!(!stale.authorization_allowed(at()));
    }

    #[test]
    fn revocation_source_revisions_are_append_only_and_cannot_regress() {
        let mut ledger = DirectoryRevocationLedger::default();
        ledger
            .append(input(
                "directory-revoke-revision-7",
                DirectoryRevocationReason::AccountDisabled,
            ))
            .expect("first fact");
        let mut regressed = input(
            "directory-revoke-revision-6",
            DirectoryRevocationReason::AccountDeleted,
        );
        regressed.source_revision = 6;
        assert!(matches!(
            ledger.append(regressed),
            Err(DirectoryRevocationError::SourceRevisionRegression)
        ));
    }

    #[test]
    fn revocation_facts_are_hash_chained_and_replay_safe() {
        let mut ledger = DirectoryRevocationLedger::default();
        let (first_outcome, first) = ledger
            .append(input(
                "directory-revoke/1",
                DirectoryRevocationReason::AccountDisabled,
            ))
            .expect("fact");
        assert_eq!(first_outcome, DirectoryRevocationAppendOutcome::Applied);
        let (replay_outcome, replay) = ledger
            .append(input(
                "directory-revoke/1",
                DirectoryRevocationReason::AccountDisabled,
            ))
            .expect("replay");
        assert_eq!(
            replay_outcome,
            DirectoryRevocationAppendOutcome::AlreadyApplied
        );
        assert_eq!(first.event_id(), replay.event_id());
        let mut second_input = input(
            "directory-revoke/2",
            DirectoryRevocationReason::AccountDeleted,
        );
        second_input.source_revision = 8;
        let (_, second) = ledger.append(second_input).expect("second fact");
        assert_eq!(second.sequence(), 2);
        assert_eq!(second.previous_hash(), Some(first.fact_hash()));
        assert!(ledger.verify().is_ok());
    }

    #[test]
    fn immutable_link_change_is_not_a_rebind_and_projection_is_idempotent() {
        let mut ledger = DirectoryRevocationLedger::default();
        let (_, fact) = ledger
            .append(input(
                "directory-revoke/1",
                DirectoryRevocationReason::AccountDeleted,
            ))
            .expect("fact");
        let mut projection = DirectoryRevocationReadModel::default();
        projection.apply(&fact).expect("projection");
        projection.apply(&fact).expect("replay");
        assert_eq!(projection.applied_event_count(), 1);
        assert_eq!(
            projection
                .state_for(Uuid::from_u128(1), &link())
                .expect("state")
                .identity_link()
                .value(),
            "guid-1"
        );
        let changed_name = ImmutableIdentityLink::new("ad:object_guid", "guid-2").expect("link");
        assert_ne!(link(), changed_name);
    }

    #[test]
    fn rollback_plan_preserves_facts_and_disables_new_sessions_after_commit() {
        let plan = DirectoryRevocationRollbackPlan::after_commit(
            Uuid::from_u128(1),
            "source rollback requires connector disable",
        )
        .expect("plan");
        assert!(plan.preserves_history());
        assert!(plan.disables_new_sessions());
    }

    #[test]
    fn revocation_conflicts_and_projection_gaps_fail_closed() {
        let mut ledger = DirectoryRevocationLedger::default();
        let first_input = input(
            "directory-revoke-conflict",
            DirectoryRevocationReason::AccountDisabled,
        );
        let (_, first) = ledger.append(first_input.clone()).expect("fact");
        let mut conflict = first_input.clone();
        conflict.reason = DirectoryRevocationReason::AccountDeleted;
        assert!(matches!(
            ledger.append(conflict),
            Err(DirectoryRevocationError::ImmutableConflict)
        ));

        let mut second_input = first_input;
        second_input.reason = DirectoryRevocationReason::AccountDeleted;
        second_input.idempotency_key = "directory-revoke-next".to_owned();
        second_input.source_revision = 8;
        let (_, second) = ledger.append(second_input).expect("second fact");
        let mut read = DirectoryRevocationReadModel::default();
        assert!(matches!(
            read.apply(&second),
            Err(DirectoryRevocationError::SequenceGap)
        ));
        read.apply(&first).expect("first projection");
        read.apply(&second).expect("second projection");
        assert_eq!(read.applied_event_count(), 2);
    }

    #[test]
    fn revocation_idempotency_cannot_rebind_an_immutable_identity_link() {
        let mut ledger = DirectoryRevocationLedger::default();
        let first = input(
            "directory-revoke-link-rebind",
            DirectoryRevocationReason::AccountDisabled,
        );
        ledger.append(first.clone()).expect("fact");
        let mut rebound = first;
        rebound.identity_link =
            ImmutableIdentityLink::new("ad:object_guid", "guid-2").expect("link");
        assert!(matches!(
            ledger.append(rebound),
            Err(DirectoryRevocationError::ImmutableConflict)
        ));
    }
}
