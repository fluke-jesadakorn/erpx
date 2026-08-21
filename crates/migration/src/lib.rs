//! Deterministic, anonymized source fixtures used by migration and
//! reconciliation tests. The fixture contains no credentials or customer data.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

pub mod g13;
mod schema_manifest;

pub use schema_manifest::{
    DatabaseRole, SCHEMA_MANIFEST_VERSION, SCHEMA_REVISION, SchemaManifest,
    SchemaManifestViolation, migrate_database_role, verify_schema_manifest,
};

/// The control-plane write migration set is applied only to the control write
/// database by the server/worker composition roots.
///
/// Keep this composition root in sync with the embedded migration directory;
/// changing this marker also makes migration additions visible to incremental
/// builds that cache the SQLx embed macro. Authorization migration 0006,
/// provisioning metadata migration 0007, customer scope bindings migration
/// 0008, Stripe webhook audit migration 0009, object access bindings migration
/// 0010, durable worker readiness migration 0011, and support module binding
/// migration 0013 are part of the control write schema. The holding write and
/// read sets include Operations migrations 0012 and 0002 respectively. The
/// holding-write set also includes the Finance banking import/reconciliation
/// migration 0028, the holding-read banking projection migration 0029, and the
/// Marketing Journey write migrations 0044-0045 and Marketing content write
/// migration 0047 are part of the holding-write set. The holding-read set also
/// includes Marketing content projection migrations 0036-0037 and the
/// payload-free, rebuildable G13 target import projection 0040. The G13
/// projection never becomes a command source.
/// Control migration 0047 is the durable anonymous Marketing abuse window;
/// it stores only coarse holding/holding/hostname/resource counters. Control
/// migration 0048 adds the recurring Marketing delivery scheduler lease and
/// dead-letter tables; it coordinates workers without becoming holding source
/// state.
/// Control migration 0049 adds the Marketing provider onboarding lifecycle,
/// channel capabilities, and verification metadata. It stores no secret
/// values. Control migration 0050 adds account-bound channel routes without
/// credential material. Control migration 0051 adds bounded deterministic
/// provider-account priority metadata. Control migration 0052 plus forward
/// migration 0054 add durable, redacted holding backup/restore-test job
/// coordination and attempt/run attribution. Control read migration 0007
/// adds the secret-free rebuildable backup-job history projection. Control
/// migration 0053 adds holding-bound, bounded backup retention actions; forward
/// migration 0055 enforces their composite holding/holding binding; control read
/// migration 0008 adds their secret-free rebuildable report projection. Control
/// migration 0056 adds bounded object-inventory reconciliation evidence and
/// control read migration 0009 adds its rebuildable projection.
/// Holding-write migration 0051 adds the holding-scoped Operations SLA
/// notification inbox and claim/lease state; it never stores provider
/// credentials or message bodies.
/// Holding-write migration 0052 adds append-only Thai statutory tax facts
/// linked to immutable posted journal lines; holding-read migration 0041 adds
/// only their rebuildable, payload-free tax report projection.
/// Control migration 0057 adds holding-bound, two-person owner-seeding metadata;
/// recovery material remains in the identity write schema and is never copied
/// to a control read database. Control migration 0058 adds durable G13 run,
/// transition, idempotency, checkpoint, work-item, reconciliation, and rollback
/// evidence state; control-read migration 0011 adds only rebuildable migration
/// status and reconciliation projections. Control migration 0059 adds bounded
/// read-only Legacy source snapshot evidence and control-read migration 0012
/// projects only source hash/count metadata. Holding-write migration 0050 adds
/// the RLS-scoped G13 import/rollback ledger and its append-only target outbox
/// events. Neither migration performs a command owner switch.
/// Control migration 0061 adds bounded, secret-free worker failure context to
/// control outbox rows without changing business event payloads.
/// Control migration 0062 adds validated, secret-free PITR recovery-point
/// metadata to backup jobs and runs. It stores no WAL bytes, credentials, or
/// physical cluster data; the physical archive/restore executor remains
/// credential-gated and fail-closed.
/// Control read migration 0013 extends the rebuildable backup-job vocabulary
/// with the PITR kind without projecting recovery references.
/// Control migration 0063 adds the durable, credential-free Finance
/// bank-provider import job/claim/evidence lifecycle; control read migration
/// 0014 projects only rebuildable job status and never request or credential
/// material.
/// Control migration 0064 adds composite holding/holding/worker binding
/// constraints to the holding projection scheduler. It is defense-in-depth
/// for recovery and direct-SQL paths; scheduler authorization remains in the
/// repository and worker composition.
/// Control migration 0065 requires content-addressed source fences on G13
/// work items; repository enqueue paths additionally bind each fence to the
/// immutable migration run source contract hash.
/// Holding-read migration 0042 adds the durable Marketing delivery projection
/// cursor; it stores only source-event position metadata and is rebuilt from
/// the holding-write delivery outbox.
/// Control migration 0067 makes physical PITR cluster-scoped, rejects new
/// holding PITR jobs, and stores only redacted chain, WAL continuity, recovery
/// point, rehearsal, retention, and promotion-audit metadata.
/// Control migration 0068 adds payload-free cluster recovery markers used by
/// isolated local timestamp/LSN rehearsals.
/// Control migration 0069 adds exact customer/holding/root-holding bindings,
/// hashed 30-minute platform holding-administration contexts, Stripe binding
/// metadata, bounded grace extensions, holding-visible platform access audit,
/// and rebuildable holding/invoice summaries. It performs no automatic
/// binding, activation, billing, suspension, or deletion.
/// Control migration 0070 adds ERPX-managed/customer-data-plane profiles,
/// hash-only agent enrollment and Root capabilities, signed desired/observed
/// configuration state, customer approval records, and canonical business-unit
/// configuration. It stores secret references only and defaults already-bound
/// holdings to ERPX-managed execution without changing routing.
/// Control migration 0072 adds secret-free database profiles, opt-in
/// holding-plus-branch routing, durable branch provisioning jobs and health,
/// and resumable first-install receipts. It stores no connection URLs or
/// credential values.
/// Control migration 0073 adds durable, categorical Platform integration
/// observations. It stores no endpoints, database keys, provider payloads, or
/// credential material and is refreshed only from server-owned probes.
/// Holding-write migration 0057 introduces the shared atomic CQRS receipt,
/// audit, and outbox schema. Aggregate authority remains in context-owned
/// typed relational tables. Holding-read migration 0048 introduces
/// disposable projection generations, checkpoints, command receipts, retries,
/// and dead letters. Context-owned read tables remain separate migrations and
/// include a generation fence so rebuilds can switch atomically. Holding-read
/// migration 0049 is the first such table: the typed Quality inspection view.
/// Control write migration 0075 and control read migration 0015 install the
/// same receipt/audit/outbox and rebuild primitives for platform-scoped
/// aggregates and keep typed authority outside those shared fact tables.
/// Control write migration 0076 adds atomic Wave 5 guards for Platform
/// Administration, Brand, and Customer Authority; control read migration 0016
/// adds their disposable generation-scoped projections.
/// Holding-write migration 0060 adds Wave 6 context guards plus the typed Assets
/// fact inbox used by Maintenance; holding-read migration 0052 adds disposable,
/// generation-scoped projections for all six paired contexts.
/// Holding-write migration 0061 adds Sales, People & Payroll, and POS command
/// guards; holding-read migration 0053 adds their disposable, generation-scoped
/// projections for the Wave 7 transactional-context cutover.
/// Holding-write migration 0062 adds Finance, Legal, and Support guards;
/// holding-read migration 0054 adds their disposable projections. Capability
/// secrets remain hash-only on the write side and never enter a read model.
/// Holding-write migration 0063 adds Operations exception/source-receipt/policy
/// guards; holding-read migration 0055 adds its disposable projections and
/// source-qualified cursor foundation. Control-read migration 0017 adds the
/// composed Customer 360 projections with an independent cursor per holding
/// source database, so global sequences are never compared across databases.
/// Control migration 0080 stores only credential-free development-persona role
/// and scope metadata beside the encrypted, expiring context token. This lets
/// the BFF resolve the effective navigation scope without replacing the
/// controller's durable role and restores that role when the context ends.
/// Control migration 0082 completes typed authority for current control-plane
/// contexts. Its Platform Completion section uses registry, backup-job and
/// projection-switch scalar rows plus relational reconciliation and connector
/// child tables; it stores no serialized aggregate document.
pub static CONTROL_PLANE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The control read database receives only rebuildable projection tables.
pub static CONTROL_READ_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_read");

/// Per-holding write and read schemas are independently rebuildable and never
/// share the control-plane migration directory.
pub static HOLDING_WRITE_MIGRATOR: sqlx::migrate::Migrator =
    sqlx::migrate!("./migrations_holding_write");
pub static HOLDING_READ_MIGRATOR: sqlx::migrate::Migrator =
    sqlx::migrate!("./migrations_holding_read");

pub async fn migrate_control_plane(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    migrate_database_role(pool, DatabaseRole::ControlWrite).await
}

pub async fn migrate_control_read(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    migrate_database_role(pool, DatabaseRole::ControlRead).await
}

pub async fn migrate_holding_write(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    migrate_database_role(pool, DatabaseRole::HoldingWrite).await
}

pub async fn migrate_holding_read(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    migrate_database_role(pool, DatabaseRole::HoldingRead).await
}

/// Deterministically map a source identifier into an opaque ERPX identifier.
/// The context is part of the digest so equal source IDs in separate source
/// aggregates cannot collide, while reruns produce the same mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicIdMapper {
    namespace: String,
}

impl DeterministicIdMapper {
    pub fn new(namespace: impl Into<String>) -> Result<Self, FixtureError> {
        let namespace = namespace.into();
        if namespace.trim().is_empty() {
            return Err(FixtureError::InvalidMappingNamespace);
        }
        Ok(Self { namespace })
    }

    pub fn map(&self, context: &str, source_id: Uuid) -> Uuid {
        let mut digest = Sha256::new();
        digest.update(self.namespace.as_bytes());
        digest.update([0]);
        digest.update(context.as_bytes());
        digest.update([0]);
        digest.update(source_id.as_bytes());
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest.finalize()[..16]);
        // Mark the result as a UUID v4/ RFC-compatible variant while retaining
        // deterministic entropy for opaque external references.
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconciliationSnapshot {
    pub counts: BTreeMap<String, u64>,
    pub totals_minor: BTreeMap<String, i64>,
}

impl ReconciliationSnapshot {
    pub fn from_fixture(fixture: &MigrationFixture) -> Self {
        let mut snapshot = Self::default();
        snapshot.count("holdings", fixture.holdings.len());
        snapshot.count(
            "companies",
            fixture
                .holdings
                .iter()
                .map(|holding| holding.companies.len())
                .sum(),
        );
        snapshot.count(
            "branches",
            fixture
                .holdings
                .iter()
                .flat_map(|holding| holding.companies.iter())
                .map(|company| company.branches.len())
                .sum(),
        );
        snapshot.count("role_assignments", fixture.role_assignments.len());
        snapshot.count("customers", fixture.customers.len());
        snapshot.count("sales_orders", fixture.sales_orders.len());
        snapshot.count("purchase_orders", fixture.purchase_orders.len());
        snapshot.count("inventory_movements", fixture.inventory_movements.len());
        snapshot.count("journal_entries", fixture.journal_entries.len());
        snapshot.count("support_tickets", fixture.support_tickets.len());
        snapshot.count("operations_exceptions", fixture.operations_exceptions.len());
        snapshot.count("marketing_profiles", fixture.marketing_profiles.len());
        snapshot.count("attribution", fixture.attribution.len());
        snapshot.count("assets", fixture.assets.len());
        snapshot.count("maintenance", fixture.maintenance.len());
        snapshot.count("employees", fixture.employees.len());
        snapshot.count("legal_documents", fixture.legal_documents.len());
        snapshot.total(
            "sales_orders",
            fixture
                .sales_orders
                .iter()
                .map(|row| row.amount_minor)
                .sum(),
        );
        snapshot.total(
            "purchase_orders",
            fixture
                .purchase_orders
                .iter()
                .map(|row| row.amount_minor)
                .sum(),
        );
        snapshot.total(
            "inventory_movements",
            fixture
                .inventory_movements
                .iter()
                .map(|row| row.quantity)
                .sum(),
        );
        snapshot.total(
            "journal_debits",
            fixture
                .journal_entries
                .iter()
                .map(|row| row.debit_minor)
                .sum(),
        );
        snapshot.total(
            "journal_credits",
            fixture
                .journal_entries
                .iter()
                .map(|row| row.credit_minor)
                .sum(),
        );
        snapshot.total(
            "attribution_revenue",
            fixture
                .attribution
                .iter()
                .map(|row| row.revenue_minor)
                .sum(),
        );
        snapshot.total(
            "asset_book_value",
            fixture.assets.iter().map(|row| row.book_value_minor).sum(),
        );
        snapshot.total(
            "payroll",
            fixture.employees.iter().map(|row| row.payroll_minor).sum(),
        );
        snapshot
    }

    pub fn count(&mut self, key: impl Into<String>, value: usize) {
        self.count_u64(key, u64::try_from(value).unwrap_or(u64::MAX));
    }

    fn count_u64(&mut self, key: impl Into<String>, value: u64) {
        self.counts.insert(key.into(), value);
    }

    pub fn total(&mut self, key: impl Into<String>, value: i64) {
        self.totals_minor.insert(key.into(), value);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationDifference {
    pub key: String,
    pub source_count: Option<u64>,
    pub target_count: Option<u64>,
    pub source_total_minor: Option<i64>,
    pub target_total_minor: Option<i64>,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationReport {
    pub differences: Vec<ReconciliationDifference>,
    pub unexplained: Vec<String>,
}

/// Ordered, durable-safe cutover phases for one migration cohort.
///
/// This is deliberately transport-neutral: a persistence adapter may store
/// the serialized run, but callers can only advance one phase at a time. The
/// command owner is derived from the phase so a caller cannot set ERPX as the
/// owner before the final-delta and rollback gates have completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationPhase {
    SourceFrozen,
    IdsMapped,
    WriteBackfilled,
    ReadProjected,
    ShadowReplaying,
    Reconciled,
    ReadOnlyPilot,
    WorkflowAccepted,
    RollbackRehearsed,
    MutationFrozen,
    FinalDeltaImported,
    ErpxCommandOwner,
    LegacyReadOnly,
    RetentionComplete,
    LegacyRetired,
}

impl MigrationPhase {
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::SourceFrozen => Some(Self::IdsMapped),
            Self::IdsMapped => Some(Self::WriteBackfilled),
            Self::WriteBackfilled => Some(Self::ReadProjected),
            Self::ReadProjected => Some(Self::ShadowReplaying),
            Self::ShadowReplaying => Some(Self::Reconciled),
            Self::Reconciled => Some(Self::ReadOnlyPilot),
            Self::ReadOnlyPilot => Some(Self::WorkflowAccepted),
            Self::WorkflowAccepted => Some(Self::RollbackRehearsed),
            Self::RollbackRehearsed => Some(Self::MutationFrozen),
            Self::MutationFrozen => Some(Self::FinalDeltaImported),
            Self::FinalDeltaImported => Some(Self::ErpxCommandOwner),
            Self::ErpxCommandOwner => Some(Self::LegacyReadOnly),
            Self::LegacyReadOnly => Some(Self::RetentionComplete),
            Self::RetentionComplete => Some(Self::LegacyRetired),
            Self::LegacyRetired => None,
        }
    }

    pub const fn command_owner(self) -> CommandOwner {
        match self {
            Self::ErpxCommandOwner
            | Self::LegacyReadOnly
            | Self::RetentionComplete
            | Self::LegacyRetired => CommandOwner::Erpx,
            _ => CommandOwner::Legacy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOwner {
    Legacy,
    Erpx,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationRun {
    pub run_id: Uuid,
    pub cohort: String,
    pub source_contract_hash: String,
    pub id_namespace: String,
    pub phase: MigrationPhase,
    pub retention_until: Option<DateTime<Utc>>,
    pub transitioned_at: DateTime<Utc>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MigrationRunError {
    #[error("migration run ID must be non-nil")]
    NilRunId,
    #[error("migration {0} is empty or exceeds 128 bytes")]
    InvalidReference(&'static str),
    #[error("source contract hash must be exactly 64 hexadecimal characters")]
    InvalidSourceContractHash,
    #[error("migration run is already retired")]
    Retired,
    #[error("retention window must be scheduled after Legacy becomes read-only")]
    RetentionNotSchedulable,
    #[error("retention window must end after the scheduling instant")]
    InvalidRetentionWindow,
    #[error("Legacy retention window has not elapsed")]
    RetentionNotElapsed,
}

/// Aggregate-only, PII-free snapshot supplied to the generic legacy migration
/// machinery. Source-specific extractors live outside ERPX; only deterministic
/// fingerprints, opaque mappings, counts, totals, and reconciliation metadata
/// cross this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacySourceSnapshot {
    pub extractor_version: String,
    pub generated_at: DateTime<Utc>,
    pub source_contract_hash: String,
    pub missing_tables: Vec<String>,
    pub counts: BTreeMap<String, u64>,
    pub statuses: BTreeMap<String, BTreeMap<String, u64>>,
    pub totals_minor: BTreeMap<String, i64>,
    pub object_bytes: BTreeMap<String, u64>,
    pub id_mappings: BTreeMap<String, BTreeMap<String, Uuid>>,
}

/// Compute the stable content fence for a source-agnostic legacy snapshot.
///
/// Capture time and the fence field itself are deliberately excluded so an
/// external extractor can replay identical source content without changing
/// the contract hash.
pub fn legacy_snapshot_contract_hash(
    snapshot: &LegacySourceSnapshot,
) -> Result<String, serde_json::Error> {
    let mut contract = snapshot.clone();
    contract.generated_at = DateTime::<Utc>::UNIX_EPOCH;
    contract.source_contract_hash.clear();
    let bytes = serde_json::to_vec(&contract)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

impl LegacySourceSnapshot {
    pub fn seal(mut self) -> Result<Self, serde_json::Error> {
        self.source_contract_hash = legacy_snapshot_contract_hash(&self)?;
        Ok(self)
    }
}

impl MigrationRun {
    pub fn new(
        run_id: Uuid,
        cohort: impl Into<String>,
        source_contract_hash: impl Into<String>,
        id_namespace: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, MigrationRunError> {
        if run_id.is_nil() {
            return Err(MigrationRunError::NilRunId);
        }
        let cohort = bounded_migration_reference(cohort.into(), "cohort")?;
        let id_namespace = bounded_migration_reference(id_namespace.into(), "id namespace")?;
        let source_contract_hash = source_contract_hash.into();
        if source_contract_hash.len() != 64
            || !source_contract_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(MigrationRunError::InvalidSourceContractHash);
        }
        Ok(Self {
            run_id,
            cohort,
            source_contract_hash,
            id_namespace,
            phase: MigrationPhase::SourceFrozen,
            retention_until: None,
            transitioned_at: now,
        })
    }

    pub const fn command_owner(&self) -> CommandOwner {
        self.phase.command_owner()
    }

    pub fn schedule_retention_window(
        &mut self,
        retention_until: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), MigrationRunError> {
        if self.phase != MigrationPhase::LegacyReadOnly {
            return Err(MigrationRunError::RetentionNotSchedulable);
        }
        if retention_until <= now {
            return Err(MigrationRunError::InvalidRetentionWindow);
        }
        self.retention_until = Some(retention_until);
        self.transitioned_at = now;
        Ok(())
    }

    pub fn advance(&mut self, now: DateTime<Utc>) -> Result<MigrationPhase, MigrationRunError> {
        let next = self.phase.next().ok_or(MigrationRunError::Retired)?;
        if next == MigrationPhase::RetentionComplete
            && self.retention_until.is_none_or(|until| until > now)
        {
            return Err(MigrationRunError::RetentionNotElapsed);
        }
        self.phase = next;
        self.transitioned_at = now;
        Ok(next)
    }
}

fn bounded_migration_reference(
    value: String,
    field: &'static str,
) -> Result<String, MigrationRunError> {
    if value.trim().is_empty()
        || value.len() > 128
        || value
            .chars()
            .any(|character| character.is_control() || character == '/' || character == '\\')
    {
        return Err(MigrationRunError::InvalidReference(field));
    }
    Ok(value)
}

impl ReconciliationReport {
    pub fn is_reconciled(&self) -> bool {
        self.differences.is_empty() && self.unexplained.is_empty()
    }
}

pub fn reconcile_snapshots(
    source: &ReconciliationSnapshot,
    target: &ReconciliationSnapshot,
    explanations: &BTreeMap<String, String>,
) -> ReconciliationReport {
    let keys = source
        .counts
        .keys()
        .chain(target.counts.keys())
        .chain(source.totals_minor.keys())
        .chain(target.totals_minor.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut differences = Vec::new();
    let mut unexplained = Vec::new();
    for key in keys {
        let source_count = source.counts.get(&key).copied();
        let target_count = target.counts.get(&key).copied();
        let source_total_minor = source.totals_minor.get(&key).copied();
        let target_total_minor = target.totals_minor.get(&key).copied();
        if source_count != target_count || source_total_minor != target_total_minor {
            let explanation = explanations.get(&key).cloned();
            if explanation.is_none() {
                unexplained.push(key.clone());
            }
            differences.push(ReconciliationDifference {
                key,
                source_count,
                target_count,
                source_total_minor,
                target_total_minor,
                explanation,
            });
        }
    }
    ReconciliationReport {
        differences,
        unexplained,
    }
}

const FIXTURE_JSON: &str = include_str!("../../../fixtures/migration/deterministic.json");

#[derive(Debug, Clone, Deserialize)]
pub struct MigrationFixture {
    pub fixture_version: String,
    pub seed: u64,
    pub holdings: Vec<HoldingFixture>,
    pub role_assignments: Vec<RoleFixture>,
    pub customers: Vec<CustomerFixture>,
    pub sales_orders: Vec<StatusFixture>,
    pub purchase_orders: Vec<StatusFixture>,
    pub inventory_movements: Vec<InventoryMovementFixture>,
    pub journal_entries: Vec<JournalFixture>,
    pub support_tickets: Vec<StatusFixture>,
    pub operations_exceptions: Vec<StatusFixture>,
    pub marketing_profiles: Vec<MarketingProfileFixture>,
    pub attribution: Vec<AttributionFixture>,
    pub assets: Vec<AssetFixture>,
    pub maintenance: Vec<StatusFixture>,
    pub employees: Vec<EmployeeFixture>,
    pub legal_documents: Vec<LegalDocumentFixture>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HoldingFixture {
    pub id: Uuid,
    pub name: String,
    pub companies: Vec<CompanyFixture>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyFixture {
    pub id: Uuid,
    pub legal_name: String,
    pub branches: Vec<BranchFixture>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BranchFixture {
    pub id: Uuid,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleFixture {
    pub id: Uuid,
    pub principal_ref: String,
    pub holding_id: Uuid,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
    pub parent_assignment_id: Option<Uuid>,
    pub status: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomerFixture {
    pub id: Uuid,
    pub account_code: String,
    pub holding_id: Uuid,
    pub status: String,
    pub billing_state: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusFixture {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub status: String,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InventoryMovementFixture {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub product_code: String,
    pub quantity: i64,
    pub movement_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JournalFixture {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub period: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketingProfileFixture {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub email: String,
    pub consent_purposes: Vec<String>,
    pub suppressed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttributionFixture {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub model: String,
    pub revenue_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetFixture {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub asset_code: String,
    pub status: String,
    pub book_value_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmployeeFixture {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub employment_state: String,
    pub leave_status: String,
    pub payroll_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegalDocumentFixture {
    pub id: Uuid,
    pub holding_id: Uuid,
    pub document_type: String,
    pub version: u32,
    pub retention_class: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FixtureError {
    #[error("fixture JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("fixture must contain at least two holdings")]
    RequiresMultipleHoldings,
    #[error("fixture must contain multiple companies and branches")]
    RequiresOrganizationDepth,
    #[error("fixture must contain active and expired assignments")]
    RequiresRoleWindows,
    #[error("fixture domain is empty: {0}")]
    EmptyDomain(&'static str),
    #[error("journal {0} is unbalanced")]
    UnbalancedJournal(Uuid),
    #[error("fixture contains a cross-holding reference")]
    CrossHoldingReference,
    #[error("migration mapping namespace cannot be empty")]
    InvalidMappingNamespace,
}

pub fn load_deterministic_fixture() -> Result<MigrationFixture, FixtureError> {
    let fixture: MigrationFixture = serde_json::from_str(FIXTURE_JSON)
        .map_err(|error| FixtureError::InvalidJson(error.to_string()))?;
    validate_fixture(&fixture)?;
    Ok(fixture)
}

pub fn validate_fixture(fixture: &MigrationFixture) -> Result<(), FixtureError> {
    if fixture.holdings.len() < 2 {
        return Err(FixtureError::RequiresMultipleHoldings);
    }
    let company_count: usize = fixture
        .holdings
        .iter()
        .map(|holding| holding.companies.len())
        .sum();
    let branch_count: usize = fixture
        .holdings
        .iter()
        .flat_map(|holding| holding.companies.iter())
        .map(|company| company.branches.len())
        .sum();
    if company_count < 3 || branch_count < 4 {
        return Err(FixtureError::RequiresOrganizationDepth);
    }
    let now = Utc::now();
    if !fixture
        .role_assignments
        .iter()
        .any(|role| role.status == "active" && role.valid_from <= now && role.valid_until > now)
        || !fixture
            .role_assignments
            .iter()
            .any(|role| role.status == "expired" && role.valid_until <= now)
    {
        return Err(FixtureError::RequiresRoleWindows);
    }
    let required_domains: [(&'static str, usize); 11] = [
        ("role_assignments", fixture.role_assignments.len()),
        ("customers", fixture.customers.len()),
        ("sales_orders", fixture.sales_orders.len()),
        ("purchase_orders", fixture.purchase_orders.len()),
        ("inventory_movements", fixture.inventory_movements.len()),
        ("journal_entries", fixture.journal_entries.len()),
        ("support_tickets", fixture.support_tickets.len()),
        ("operations_exceptions", fixture.operations_exceptions.len()),
        ("marketing_profiles", fixture.marketing_profiles.len()),
        ("assets", fixture.assets.len()),
        ("employees", fixture.employees.len()),
    ];
    if let Some((name, _)) = required_domains.iter().find(|(_, count)| *count == 0) {
        return Err(FixtureError::EmptyDomain(name));
    }
    if fixture
        .journal_entries
        .iter()
        .any(|journal| journal.debit_minor != journal.credit_minor)
    {
        let journal = fixture
            .journal_entries
            .iter()
            .find(|journal| journal.debit_minor != journal.credit_minor)
            .expect("unbalanced journal exists");
        return Err(FixtureError::UnbalancedJournal(journal.id));
    }
    let holding_ids: std::collections::HashSet<Uuid> =
        fixture.holdings.iter().map(|holding| holding.id).collect();
    if fixture
        .customers
        .iter()
        .any(|customer| !holding_ids.contains(&customer.holding_id))
        || fixture
            .sales_orders
            .iter()
            .any(|order| !holding_ids.contains(&order.holding_id))
    {
        return Err(FixtureError::CrossHoldingReference);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn between_markers<'a>(sql: &'a str, start: &str, end: &str) -> &'a str {
        sql.split_once(start)
            .expect("baseline start marker")
            .1
            .split_once(end)
            .expect("baseline end marker")
            .0
    }

    #[test]
    fn deterministic_fixture_is_valid_and_repeatable() {
        let first = load_deterministic_fixture().expect("fixture");
        let second = load_deterministic_fixture().expect("fixture");
        assert_eq!(first.fixture_version, "g0-004");
        assert_eq!(first.seed, second.seed);
        assert_eq!(first.holdings[0].id, second.holdings[0].id);
        assert!(first.holdings.len() >= 2);
        assert!(
            first
                .holdings
                .iter()
                .any(|holding| holding.companies.len() >= 2)
        );
    }

    #[test]
    fn fixture_covers_balanced_finance_and_marketing_attribution() {
        let fixture = load_deterministic_fixture().expect("fixture");
        assert!(
            fixture
                .journal_entries
                .iter()
                .all(|journal| journal.debit_minor == journal.credit_minor)
        );
        assert!(
            fixture
                .marketing_profiles
                .iter()
                .any(|profile| profile.suppressed)
        );
        assert!(
            fixture
                .attribution
                .iter()
                .any(|attribution| attribution.model == "u_shaped")
        );
    }

    #[test]
    fn deterministic_mapping_is_stable_and_context_bound() {
        let mapper = DeterministicIdMapper::new("legacy-shadow").expect("namespace");
        let source_id = Uuid::new_v4();
        assert_eq!(
            mapper.map("customer", source_id),
            mapper.map("customer", source_id)
        );
        assert_ne!(
            mapper.map("customer", source_id),
            mapper.map("invoice", source_id)
        );
        assert_ne!(mapper.map("customer", source_id), source_id);
        assert!(DeterministicIdMapper::new(" ").is_err());
    }

    #[test]
    fn reconciliation_reports_and_explains_every_difference() {
        let fixture = load_deterministic_fixture().expect("fixture");
        let source = ReconciliationSnapshot::from_fixture(&fixture);
        let mut target = source.clone();
        assert!(reconcile_snapshots(&source, &target, &BTreeMap::new()).is_reconciled());
        target.counts.insert("customers".into(), 99);
        let unexplained = reconcile_snapshots(&source, &target, &BTreeMap::new());
        assert!(!unexplained.is_reconciled());
        assert_eq!(unexplained.unexplained, vec!["customers"]);
        let mut explanations = BTreeMap::new();
        explanations.insert("customers".into(), "redacted test row".into());
        let explained = reconcile_snapshots(&source, &target, &explanations);
        assert!(explained.unexplained.is_empty());
        assert_eq!(
            explained.differences[0].explanation.as_deref(),
            Some("redacted test row")
        );
    }

    #[test]
    fn migration_run_requires_the_exact_ordered_cutover_sequence() {
        let now = Utc::now();
        let mut run = MigrationRun::new(
            Uuid::new_v4(),
            "internal-demo",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "legacy-shadow-v1",
            now,
        )
        .expect("migration run");
        assert_eq!(run.phase, MigrationPhase::SourceFrozen);
        assert_eq!(run.command_owner(), CommandOwner::Legacy);

        for expected in [
            MigrationPhase::IdsMapped,
            MigrationPhase::WriteBackfilled,
            MigrationPhase::ReadProjected,
            MigrationPhase::ShadowReplaying,
            MigrationPhase::Reconciled,
            MigrationPhase::ReadOnlyPilot,
            MigrationPhase::WorkflowAccepted,
            MigrationPhase::RollbackRehearsed,
            MigrationPhase::MutationFrozen,
            MigrationPhase::FinalDeltaImported,
            MigrationPhase::ErpxCommandOwner,
            MigrationPhase::LegacyReadOnly,
        ] {
            assert_eq!(run.advance(now).expect("next phase"), expected);
        }
        assert_eq!(run.command_owner(), CommandOwner::Erpx);
        assert!(matches!(
            run.advance(now),
            Err(MigrationRunError::RetentionNotElapsed)
        ));

        let retention_until = now + chrono::Duration::hours(1);
        run.schedule_retention_window(retention_until, now)
            .expect("retention window");
        assert!(matches!(
            run.advance(now),
            Err(MigrationRunError::RetentionNotElapsed)
        ));
        assert_eq!(
            run.advance(retention_until),
            Ok(MigrationPhase::RetentionComplete)
        );
        assert_eq!(
            run.advance(retention_until),
            Ok(MigrationPhase::LegacyRetired)
        );
        assert!(matches!(
            run.advance(retention_until),
            Err(MigrationRunError::Retired)
        ));
    }

    #[test]
    fn migration_run_rejects_unsafe_contract_and_retention_inputs() {
        let now = Utc::now();
        assert_eq!(
            MigrationRun::new(
                Uuid::new_v4(),
                "demo",
                "not-a-contract-hash",
                "namespace",
                now,
            )
            .expect_err("invalid hash"),
            MigrationRunError::InvalidSourceContractHash
        );
        let mut run = MigrationRun::new(
            Uuid::new_v4(),
            "demo",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "namespace",
            now,
        )
        .expect("migration run");
        assert_eq!(
            run.schedule_retention_window(now + chrono::Duration::hours(1), now),
            Err(MigrationRunError::RetentionNotSchedulable)
        );
    }

    #[test]
    fn source_snapshot_contract_hash_ignores_capture_time_but_binds_content() {
        let base = LegacySourceSnapshot {
            extractor_version: "g13-generic-source-v1".into(),
            generated_at: Utc::now(),
            source_contract_hash: String::new(),
            missing_tables: vec![],
            counts: BTreeMap::from([(String::from("customers"), 2)]),
            statuses: BTreeMap::new(),
            totals_minor: BTreeMap::from([(String::from("sales_orders_minor"), 100)]),
            object_bytes: BTreeMap::new(),
            id_mappings: BTreeMap::new(),
        };
        let mut later = base.clone();
        later.generated_at += chrono::Duration::hours(2);
        assert_eq!(
            legacy_snapshot_contract_hash(&base).expect("hash"),
            legacy_snapshot_contract_hash(&later).expect("hash")
        );
        later.counts.insert("customers".into(), 3);
        assert_ne!(
            legacy_snapshot_contract_hash(&base).expect("hash"),
            legacy_snapshot_contract_hash(&later).expect("hash")
        );
    }

    #[test]
    fn control_plane_migration_is_embedded_and_has_the_expected_schema() {
        assert_eq!(CONTROL_PLANE_MIGRATOR.iter().count(), 2);
        let initial = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let command_receipts = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(initial.sql.contains("erpx_control.outbox_events"));
        assert!(initial.sql.contains("erpx_control.projection_checkpoints"));
        assert!(initial.sql.contains("erpx_control.idempotency_keys"));
        assert!(
            command_receipts
                .sql
                .contains("erpx_control.projection_event_receipts")
        );
        let audits = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(audits.sql.contains("erpx_control.global_read_audits"));
        assert_eq!(CONTROL_READ_MIGRATOR.iter().count(), 1);
        let support_sla = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(support_sla.sql.contains("support_sla_notifications"));
        assert!(support_sla.sql.contains("claim_expires_at"));
        let platform_governance_write = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for table in [
            "governance_records",
            "governance_record_receipts",
            "governance_record_audit",
        ] {
            assert!(platform_governance_write.sql.contains(table));
        }
        assert!(
            !platform_governance_write
                .sql
                .contains("governance_record_snapshots")
        );
        let platform_governance_read = CONTROL_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            platform_governance_read
                .sql
                .contains("governance_record_projection")
        );
        assert!(platform_governance_read.sql.contains("generation UUID"));
        let normalized_control = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for contract in [
            "PLATFORM_COMPLETION_TYPED_AUTHORITY_BEGIN",
            "lifecycle_aggregate_version",
            "holding_backup_jobs",
            "holding_projection_switches",
            "reconciliation_authority",
            "reconciliation_drift_codes",
            "connector_authority",
            "connector_capabilities",
        ] {
            assert!(normalized_control.sql.contains(contract));
        }
        assert!(
            !between_markers(
                normalized_control.sql.as_ref(),
                "PLATFORM_COMPLETION_TYPED_AUTHORITY_BEGIN",
                "PLATFORM_COMPLETION_TYPED_AUTHORITY_END",
            )
            .contains("JSONB")
        );
        let read = CONTROL_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let detail_reads = CONTROL_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(read.sql.contains("erpx_control.customers"));
        assert!(!read.sql.contains("outbox_events"));
        assert!(
            detail_reads
                .sql
                .contains("erpx_control.customer_access_audit_summaries")
        );
        let incident_reads = CONTROL_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(incident_reads.sql.contains("erpx_control.incidents"));
        assert!(detail_reads.sql.contains("erpx_control.customer_notes"));
        let projection_status = CONTROL_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            projection_status
                .sql
                .contains("erpx_control.holding_projection_status")
        );
        let projection_sql = projection_status
            .sql
            .lines()
            .filter(|line| line.contains("holding_projection_status"))
            .collect::<String>()
            .to_ascii_lowercase();
        for forbidden in [
            "payload",
            "claim_id",
            "worker_id",
            "database_url",
            "storage_namespace",
            "credential",
        ] {
            assert!(!projection_sql.contains(forbidden), "{forbidden}");
        }
        assert!(
            projection_status
                .sql
                .to_ascii_lowercase()
                .contains("attempts between 0 and 25")
        );
        assert!(
            projection_status
                .sql
                .to_ascii_lowercase()
                .contains("octet_length(last_error) <= 1000")
        );
        let freshness = CONTROL_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(freshness.sql.contains("published_at"));
        let checkpoint = CONTROL_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            checkpoint
                .sql
                .contains("holding_projection_status_checkpoints")
        );
        assert!(checkpoint.sql.contains("source_updated_at"));
        assert!(checkpoint.sql.contains("source_job_id"));
    }

    #[test]
    fn holding_write_and_read_migrations_are_separate_and_rls_backed() {
        assert_eq!(HOLDING_WRITE_MIGRATOR.iter().count(), 2);
        assert_eq!(HOLDING_READ_MIGRATOR.iter().count(), 1);
        let write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let operations_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let operations_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(write.sql.contains("erpx_holding.outbox_events"));
        assert!(
            operations_write
                .sql
                .contains("erpx_holding.operations_events")
        );
        assert!(operations_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(!read.sql.contains("outbox_events"));
        assert!(read.sql.contains("erpx_holding.customer_summary"));
        assert!(
            operations_read
                .sql
                .contains("erpx_holding.operations_read_rows")
        );
        assert!(operations_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let final_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .find(|migration| {
                migration
                    .sql
                    .contains("DROP TABLE IF EXISTS erpx_holding.operations_aggregate_snapshots")
            })
            .expect("holding write hard-cutover migration");
        assert!(
            final_write
                .sql
                .contains("DROP TABLE IF EXISTS erpx_holding.operations_aggregate_snapshots")
        );
        assert!(
            final_write
                .sql
                .contains("DROP TABLE IF EXISTS erpx_holding.operations_events")
        );
        let final_read = HOLDING_READ_MIGRATOR
            .iter()
            .max_by_key(|migration| migration.version)
            .expect("final holding read migration");
        assert!(
            final_read
                .sql
                .contains("DROP TABLE IF EXISTS erpx_holding.operations_read_rows")
        );
        let finance_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let finance_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(finance_write.sql.contains("finance_posted_journals"));
        assert!(finance_write.sql.contains("finance_command_idempotency"));
        assert!(finance_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(finance_read.sql.contains("finance_posted_journals"));
        assert!(finance_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let finance_matching = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            finance_matching
                .sql
                .contains("finance_bank_match_proposals")
        );
        assert!(
            finance_matching
                .sql
                .contains("finance_bank_match_decisions")
        );
        assert!(finance_matching.sql.contains("FORCE ROW LEVEL SECURITY"));
        let finance_match_facts = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(finance_match_facts.sql.contains("total_minor"));
        assert!(
            finance_match_facts
                .sql
                .contains("finance_posted_journals_immutable")
        );
        let finance_match_backfill = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            finance_match_backfill
                .sql
                .contains("DISABLE TRIGGER finance_posted_journals_immutable")
        );
        assert!(
            finance_match_backfill
                .sql
                .contains("NO FORCE ROW LEVEL SECURITY")
        );
        assert!(finance_match_backfill.sql.contains("jsonb_array_elements"));
        assert!(finance_match_backfill.sql.contains("valid_line_count"));
        assert!(finance_match_backfill.sql.contains("source_currency"));
        assert!(finance_match_backfill.sql.contains("::NUMERIC"));
        let finance_budgets = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(finance_budgets.sql.contains("finance_budgets"));
        assert!(
            finance_budgets
                .sql
                .contains("finance_budget_commitments_immutable")
        );
        assert!(finance_budgets.sql.contains("approval_limit_minor"));
        assert!(finance_budgets.sql.contains("actual_minor"));
        assert!(finance_budgets.sql.contains("decision_remaining_minor"));
        assert!(
            finance_budgets
                .sql
                .contains("FOREIGN KEY (data_profile_key, budget_id)")
        );
        assert!(finance_budgets.sql.contains("FORCE ROW LEVEL SECURITY"));
        let finance_close_reviews = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(finance_close_reviews.sql.contains("finance_close_reviews"));
        assert!(
            finance_close_reviews
                .sql
                .contains("finance_close_reviews_closed_immutable")
        );
        assert!(
            finance_close_reviews
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let finance_close_control_evidence = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            finance_close_control_evidence
                .sql
                .contains("finance_close_control_evidence")
        );
        assert!(
            finance_close_control_evidence
                .sql
                .contains("finance_close_control_evidence_closed_immutable")
        );
        assert!(
            finance_close_control_evidence
                .sql
                .contains("NEW.data_profile_key")
        );
        assert!(
            finance_close_control_evidence
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let finance_close_control_evidence_versions = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            finance_close_control_evidence_versions
                .sql
                .contains("source_version")
        );
        assert!(
            finance_close_control_evidence_versions
                .sql
                .contains("source_version_check")
        );
        let marketing_crm_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for table in [
            "crm_idempotency_bindings",
            "crm_accepted_lead_handoffs",
            "crm_opportunities",
            "crm_opportunity_attribution_inputs",
            "crm_attribution_records",
            "crm_attribution_entries",
        ] {
            assert!(marketing_crm_write.sql.contains(table));
        }
        assert!(marketing_crm_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(
            marketing_crm_write
                .sql
                .contains("crm_opportunities_binding_immutable")
        );
        let marketing_content_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for table in [
            "marketing_content_assets",
            "marketing_content_asset_versions",
            "marketing_content_templates",
            "marketing_content_template_versions",
            "marketing_content_previews",
            "marketing_content_forms",
            "marketing_content_form_submissions",
            "marketing_content_domains",
            "marketing_content_domain_bindings",
            "marketing_content_approvals",
            "marketing_content_command_idempotency",
        ] {
            assert!(marketing_content_write.sql.contains(table));
        }
        assert!(
            marketing_content_write
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        assert!(
            marketing_content_write
                .sql
                .contains("reject_marketing_content_fact_mutation")
        );
        let marketing_content = marketing_content_write
            .sql
            .lines()
            .filter(|line| line.contains("marketing_content"))
            .collect::<String>();
        assert!(!marketing_content.contains("credential"));
        assert!(!marketing_content.contains("password"));
        let marketing_content_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for table in [
            "marketing_content_read_rows",
            "marketing_content_projection_checkpoints",
            "marketing_content_projection_event_receipts",
        ] {
            assert!(marketing_content_read.sql.contains(table));
        }
        assert!(
            marketing_content_read
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let marketing_content_workflow_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for table in [
            "marketing_content_preview_read_rows",
            "marketing_content_submission_read_rows",
            "marketing_content_domain_read_rows",
            "marketing_content_workflow_projection_checkpoints",
            "marketing_content_workflow_projection_event_receipts",
        ] {
            assert!(marketing_content_workflow_read.sql.contains(table));
        }
        assert!(
            marketing_content_workflow_read
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        assert!(!marketing_content_workflow_read.sql.contains("token_hash"));
        assert!(
            !marketing_content_workflow_read
                .sql
                .contains("verification_hash")
        );
        let marketing_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let marketing_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(marketing_write.sql.contains("marketing_profiles"));
        assert!(
            marketing_write
                .sql
                .contains("marketing_command_idempotency")
        );
        assert!(
            marketing_write
                .sql
                .contains("marketing_consent_append_only")
        );
        assert!(marketing_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(
            marketing_read
                .sql
                .contains("marketing_audience_memberships")
        );
        assert!(marketing_read.sql.contains("source_version"));
        assert!(marketing_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let marketing_crm_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for table in [
            "crm_accepted_lead_handoff_read_rows",
            "crm_opportunity_read_rows",
            "crm_attribution_read_rows",
            "crm_projection_event_receipts",
        ] {
            assert!(marketing_crm_read.sql.contains(table));
        }
        assert!(marketing_crm_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(
            marketing_crm_read
                .sql
                .contains("holding_scope_crm_opportunity_read_rows")
        );
        let sales_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let sales_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(sales_write.sql.contains("sales_aggregates"));
        assert!(sales_write.sql.contains("sales_command_idempotency"));
        assert!(sales_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(sales_read.sql.contains("sales_aggregates"));
        assert!(sales_read.sql.contains("source_version"));
        assert!(sales_read.sql.contains("sales_projection_checkpoints"));
        assert!(sales_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let scm_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let scm_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(scm_write.sql.contains("scm_aggregates"));
        assert!(scm_write.sql.contains("scm_command_idempotency"));
        assert!(scm_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(scm_read.sql.contains("scm_projection_checkpoints"));
        assert!(scm_read.sql.contains("source_version"));
        assert!(scm_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let asset_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let asset_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(asset_write.sql.contains("asset_maintenance_aggregates"));
        assert!(asset_write.sql.contains("asset_custody_transfers"));
        assert!(
            asset_write
                .sql
                .contains("asset_operations_capacity_handoffs")
        );
        assert!(asset_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(asset_write.sql.contains("append-only"));
        assert!(asset_read.sql.contains("asset_maintenance_read_rows"));
        assert!(
            asset_read
                .sql
                .contains("asset_maintenance_projection_checkpoints")
        );
        assert!(asset_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let finance_projection_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            finance_projection_read
                .sql
                .contains("finance_projection_checkpoints")
        );
        assert!(
            finance_projection_read
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let marketing_projection_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            marketing_projection_read
                .sql
                .contains("marketing_projection_checkpoints")
        );
        assert!(
            marketing_projection_read
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let finance_subledger_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let finance_subledger_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            finance_subledger_write
                .sql
                .contains("finance_subledger_aggregates")
        );
        assert!(
            finance_subledger_write
                .sql
                .contains("finance_subledger_command_idempotency")
        );
        assert!(
            finance_subledger_write
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        assert!(
            finance_subledger_read
                .sql
                .contains("finance_subledger_read_rows")
        );
        assert!(
            finance_subledger_read
                .sql
                .contains("finance_subledger_projection_checkpoints")
        );
        assert!(
            finance_subledger_read
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let scheduler = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(scheduler.sql.contains("holding_projection_jobs"));
        assert!(scheduler.sql.contains("holding_projection_claims"));
        assert!(scheduler.sql.contains("lease_until"));
        let scheduler_binding = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for constraint in [
            "holding_projection_jobs_holding_holding_fk",
            "holding_projection_claim_batches_holding_holding_fk",
            "holding_projection_claims_holding_holding_fk",
            "holding_projection_claims_job_binding_fk",
            "holding_projection_claims_batch_binding_fk",
            "holding_projection_rebuilds_holding_holding_fk",
            "holding_projection_rebuilds_job_binding_fk",
        ] {
            assert!(scheduler_binding.sql.contains(constraint), "{constraint}");
        }
        let marketing_delivery = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(marketing_delivery.sql.contains("marketing_deliveries"));
        assert!(
            marketing_delivery
                .sql
                .contains("marketing_delivery_idempotency")
        );
        assert!(marketing_delivery.sql.contains("FORCE ROW LEVEL SECURITY"));
        let marketing_delivery_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            marketing_delivery_read
                .sql
                .contains("marketing_delivery_read_rows")
        );
        assert!(
            marketing_delivery_read
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let marketing_crm_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for table in [
            "crm_accepted_lead_handoff_read_rows",
            "crm_opportunity_read_rows",
            "crm_attribution_read_rows",
            "crm_projection_event_receipts",
        ] {
            assert!(marketing_crm_read.sql.contains(table));
        }
        assert!(marketing_crm_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(
            marketing_crm_read
                .sql
                .contains("holding_scope_crm_projection_event_receipts")
        );
        let marketing_callback = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            marketing_callback
                .sql
                .contains("marketing_provider_callback_inbox")
        );
        assert!(marketing_callback.sql.contains("FORCE ROW LEVEL SECURITY"));
        let marketing_provider_accounts = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(
            marketing_provider_accounts
                .sql
                .contains("marketing_provider_accounts")
        );
        assert!(marketing_provider_accounts.sql.contains("verifier_key_ref"));
        let marketing_provider_accounts_outbound = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for column in ["provider_key", "outbound_credential_ref"] {
            assert!(marketing_provider_accounts_outbound.sql.contains(column));
        }
        assert!(
            marketing_provider_accounts_outbound
                .sql
                .contains("ADD COLUMN IF NOT EXISTS")
        );
        assert!(
            marketing_provider_accounts_outbound
                .sql
                .contains("legacy-unconfigured")
        );
        assert!(
            marketing_provider_accounts_outbound
                .sql
                .contains("legacy-unconfigured-credential")
        );
        assert!(
            !marketing_provider_accounts_outbound
                .sql
                .contains("outbound_secret")
        );
        assert!(
            !marketing_provider_accounts_outbound
                .sql
                .contains("outbound_token")
        );
        let finance_bank_provider_accounts = CONTROL_PLANE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for column in [
            "finance_bank_provider_accounts",
            "provider_account_ref",
            "provider_key",
            "credential_key_ref",
            "holding_registry(holding_id)",
        ] {
            assert!(finance_bank_provider_accounts.sql.contains(column));
        }
        assert!(
            finance_bank_provider_accounts
                .sql
                .contains("state IN ('active', 'suspended')")
        );
        assert!(
            finance_bank_provider_accounts
                .sql
                .contains("version BIGINT NOT NULL CHECK (version > 0)")
        );
        assert!(
            finance_bank_provider_accounts
                .sql
                .contains("UNIQUE (provider_account_ref)")
        );
    }

    #[test]
    fn cqrs_outbox_sequences_are_allocated_under_commit_order_locks() {
        let control = CONTROL_PLANE_MIGRATOR
            .iter()
            .last()
            .expect("control outbox commit-order migration");
        let holding = HOLDING_WRITE_MIGRATOR
            .iter()
            .last()
            .expect("holding outbox commit-order migration");

        for migration in [control, holding] {
            assert!(
                migration.sql.contains("lock_outbox_commit_order"),
                "outbox commit-order lock helper is missing"
            );
            assert!(
                migration.sql.contains("assign_outbox_global_sequence"),
                "outbox sequence trigger function is missing"
            );
            assert!(
                migration.sql.contains("pg_advisory_xact_lock"),
                "outbox transaction-order lock is missing"
            );
            assert!(
                migration
                    .sql
                    .contains("ALTER COLUMN global_sequence\n    DROP DEFAULT"),
                "outbox sequence default remains bypassable"
            );
            assert!(
                migration
                    .sql
                    .contains("BEFORE INSERT ON erpx_cqrs_write.outbox_events"),
                "outbox sequence allocation trigger is missing"
            );
            assert!(
                migration.sql.contains("NEW.global_sequence IS NOT NULL")
                    && migration.sql.contains("ERRCODE = '22023'"),
                "explicit outbox sequences are not rejected"
            );
        }
        assert!(
            holding
                .sql
                .contains("pg_catalog.hashtext(bound_holding::TEXT)")
        );
        assert!(control.sql.contains("1163022424"));
    }

    #[test]
    fn current_holding_tables_are_fail_closed_and_holding_composite() {
        for migration in HOLDING_WRITE_MIGRATOR.iter() {
            if !migration.sql.contains("CREATE TABLE") {
                continue;
            }
            assert!(
                migration.sql.contains("holding_id"),
                "write migration {} lacks holding identity",
                migration.version
            );
            assert!(
                migration.sql.contains("FORCE ROW LEVEL SECURITY"),
                "write migration {} does not force RLS",
                migration.version
            );
            assert!(
                migration.sql.contains("WITH CHECK"),
                "write migration {} lacks a fail-closed write policy",
                migration.version
            );
        }
        for migration in HOLDING_READ_MIGRATOR.iter() {
            if !migration.sql.contains("CREATE TABLE") {
                continue;
            }
            assert!(
                migration.sql.contains("holding_id"),
                "read migration {} lacks holding identity",
                migration.version
            );
            assert!(
                migration.sql.contains("FORCE ROW LEVEL SECURITY"),
                "read migration {} does not force RLS",
                migration.version
            );
            assert!(
                migration.sql.contains("WITH CHECK"),
                "read migration {} lacks a fail-closed write policy",
                migration.version
            );
        }

        let cqrs_write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(cqrs_write.sql.contains("PRIMARY KEY (holding_id"));
        assert!(cqrs_write.sql.contains("UNIQUE (holding_id"));

        let cqrs_read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        assert!(cqrs_read.sql.contains("PRIMARY KEY (holding_id"));
        assert!(
            cqrs_read
                .sql
                .contains("FOREIGN KEY (holding_id, projection_name, generation)")
        );

        let current_authority = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        for contract in [
            "erpx_quality_write.inspection_authority",
            "PRIMARY KEY (holding_id, inspection_id)",
            "UNIQUE (holding_id, inspection_id, aggregate_version)",
            "FORCE ROW LEVEL SECURITY",
            "WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())",
        ] {
            assert!(
                current_authority.sql.contains(contract),
                "missing {contract}"
            );
        }
        let current_authority = between_markers(
            current_authority.sql.as_ref(),
            "CREATE TABLE erpx_quality_write.inspection_authority",
            "-- END SUPPORT_NORMALIZED_AUTHORITY",
        );
        assert!(!current_authority.contains("JSONB"));
        assert!(!current_authority.contains("aggregate_components"));
    }

    #[test]
    fn wave6_paired_context_migrations_keep_write_and_read_models_separate() {
        let write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");

        for schema in [
            "erpx_marketing_write",
            "erpx_crm_write",
            "erpx_procurement_write",
            "erpx_inventory_write",
            "erpx_assets_write",
            "erpx_maintenance_write",
        ] {
            assert!(write.sql.contains(schema), "missing write schema {schema}");
            assert!(
                !read.sql.contains(schema),
                "write schema leaked into read DDL"
            );
        }
        for projection in [
            "marketing.lead_candidates",
            "crm.leads",
            "crm.opportunities",
            "procurement.purchase_orders",
            "inventory.stock_items",
            "assets.assets",
            "maintenance.work_orders",
        ] {
            assert!(read.sql.contains(projection), "missing {projection}");
            assert!(
                !write.sql.contains(projection),
                "read projection leaked into write DDL"
            );
        }
        assert!(write.sql.contains("asset_fact_inbox"));
        assert!(write.sql.contains("source_event_id UUID NOT NULL UNIQUE"));
        assert!(write.sql.contains("'discovery'"));
        assert!(read.sql.contains("'discovery'"));
        assert!(read.sql.contains("generation UUID NOT NULL"));
        assert!(read.sql.contains("projection_generations"));
    }

    #[test]
    fn wave7_transactional_context_migrations_keep_write_and_read_models_separate() {
        let write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");

        for schema in [
            "erpx_sales_write",
            "erpx_people_payroll_write",
            "erpx_pos_write",
        ] {
            assert!(write.sql.contains(schema), "missing write schema {schema}");
            assert!(
                !read.sql.contains(schema),
                "write schema leaked into read DDL"
            );
        }
        for projection in [
            "sales.quotes",
            "sales.orders",
            "people_payroll.employees",
            "people_payroll.leave_requests",
            "people_payroll.expense_claims",
            "people_payroll.payroll_runs",
            "pos.sales",
            "pos.refunds",
        ] {
            assert!(read.sql.contains(projection), "missing {projection}");
            assert!(
                !write.sql.contains(projection),
                "read projection leaked into write DDL"
            );
        }
        assert!(
            write
                .sql
                .contains("reviewer_principal_ref <> requester_principal_ref")
        );
        assert!(
            write
                .sql
                .contains("total_gross_minor - total_deductions_minor = total_net_minor")
        );
        assert!(write.sql.contains("payroll_status <> 'approved'"));
        assert!(write.sql.contains("payroll_status <> 'posted'"));
        assert!(read.sql.contains("generation UUID NOT NULL"));
        assert!(read.sql.contains("projection_generations"));
    }

    #[test]
    fn wave8_dependent_context_migrations_enforce_privacy_and_read_separation() {
        let write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");

        for schema in [
            "erpx_finance_write",
            "erpx_legal_write",
            "erpx_support_write",
        ] {
            assert!(write.sql.contains(schema), "missing write schema {schema}");
            assert!(
                !read.sql.contains(schema),
                "write schema leaked into read DDL"
            );
        }
        for projection in [
            "finance.charts",
            "finance.periods",
            "finance.journals",
            "finance.subledger",
            "finance.trial_balance",
            "finance.bank_transactions",
            "finance.statutory_tax",
            "legal.contracts",
            "legal.documents",
            "legal.access_history",
            "support.tickets",
            "support.incidents",
            "support.sessions",
            "support.capabilities",
        ] {
            assert!(read.sql.contains(projection), "missing {projection}");
            assert!(
                !write.sql.contains(projection),
                "read projection leaked into write DDL"
            );
        }

        assert!(write.sql.contains("secret_hash_sha256 BYTEA"));
        assert!(!read.sql.contains("secret_hash_sha256"));
        assert!(!read.sql.contains("capability_secret"));
        assert!(write.sql.contains("DEFERRABLE INITIALLY DEFERRED"));
        assert!(write.sql.contains("Finance source facts are append-only"));
        assert!(write.sql.contains("Legal evidence is append-only"));
        assert!(read.sql.contains("projection_generations"));
        assert!(read.sql.contains(
            "FOREIGN KEY (holding_id, projection_name, generation, partition_key, ticket_id)"
        ));
        assert!(
            !read
                .sql
                .contains("FOREIGN KEY (projection_name, generation, partition_key, ticket_id)")
        );
    }

    #[test]
    fn current_record_completion_pins_crm_marketing_provenance_and_holding_keys() {
        let write = HOLDING_WRITE_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");
        let read = HOLDING_READ_MIGRATOR
            .iter()
            .next()
            .expect("clean-slate baseline");

        for column in [
            "campaign_id UUID NOT NULL",
            "audience_member_id UUID NOT NULL",
            "marketing_source_version BIGINT NOT NULL",
            "source_event_id UUID NOT NULL",
            "source_global_sequence BIGINT NOT NULL",
        ] {
            assert!(write.sql.contains(column), "missing write column {column}");
        }
        assert!(
            write
                .sql
                .contains("PRIMARY KEY (holding_id, marketing_candidate_id)")
        );
        assert!(write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(write.sql.contains("crm_write_holding_isolation"));
        assert!(read.sql.contains("crm_lead_marketing_provenance"));
        assert!(read.sql.contains("marketing_source_version"));
        assert!(!read.sql.contains("marketing_qualified_lead_evidence"));

        for table in [
            "access_reviews",
            "access_review_items",
            "break_glass_sessions",
            "identity_governance_command_receipts",
            "identity_governance_audit_log",
        ] {
            assert!(
                write.sql.contains(table),
                "missing IAM authority table {table}"
            );
        }
        assert!(write.sql.contains("identity_governance_holding_isolation"));
        assert!(
            write
                .sql
                .contains("CREATE SCHEMA IF NOT EXISTS erpx_quality_write;")
        );
        assert!(read.sql.contains("governance_projection"));
        assert!(
            read.sql
                .contains("identity_governance_read_holding_isolation")
        );
        assert!(read.sql.contains("source_global_sequence BIGINT NOT NULL"));
        for table in [
            "erpx_marketing_write.business_records",
            "business_record_receipts",
            "business_record_audit",
        ] {
            assert!(
                write.sql.contains(table),
                "missing Marketing completion {table}"
            );
        }
        assert!(write.sql.contains("marketing_record_holding_isolation"));
        assert!(read.sql.contains("marketing.business-records"));
        assert!(
            read.sql
                .contains("marketing_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_crm_write.business_records"));
        assert!(write.sql.contains("crm_record_holding_isolation"));
        assert!(read.sql.contains("crm.business-records"));
        assert!(
            read.sql
                .contains("crm_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_finance_write.business_records"));
        assert!(write.sql.contains("finance_record_holding_isolation"));
        assert!(read.sql.contains("finance.business-records"));
        assert!(
            read.sql
                .contains("finance_business_record_read_holding_isolation")
        );
        assert!(
            write
                .sql
                .contains("erpx_procurement_write.business_records")
        );
        assert!(write.sql.contains("procurement_record_holding_isolation"));
        assert!(read.sql.contains("procurement.business-records"));
        assert!(
            read.sql
                .contains("procurement_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_assets_write.business_records"));
        assert!(write.sql.contains("assets_record_holding_isolation"));
        assert!(read.sql.contains("asset.lifecycle-records"));
        assert!(
            read.sql
                .contains("assets_business_record_read_holding_isolation")
        );
        assert!(
            write
                .sql
                .contains("erpx_maintenance_write.business_records")
        );
        assert!(write.sql.contains("maintenance_record_holding_isolation"));
        assert!(read.sql.contains("maintenance.lifecycle-records"));
        assert!(
            read.sql
                .contains("maintenance_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_quality_write.business_records"));
        assert!(write.sql.contains("quality_record_holding_isolation"));
        assert!(read.sql.contains("quality.lifecycle-records"));
        assert!(
            read.sql
                .contains("quality_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_operations_write.business_records"));
        assert!(write.sql.contains("operations_record_holding_isolation"));
        assert!(read.sql.contains("operations.lifecycle-records"));
        assert!(
            read.sql
                .contains("operations_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_commercial_write.business_records"));
        assert!(write.sql.contains("commercial_record_holding_isolation"));
        assert!(read.sql.contains("commercial.lifecycle-records"));
        assert!(
            read.sql
                .contains("commercial_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_pos_write.business_records"));
        assert!(write.sql.contains("pos_record_holding_isolation"));
        assert!(read.sql.contains("pos.lifecycle-records"));
        assert!(
            read.sql
                .contains("pos_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_sales_write.business_records"));
        assert!(write.sql.contains("sales_record_holding_isolation"));
        assert!(read.sql.contains("sales.lifecycle-records"));
        assert!(
            read.sql
                .contains("sales_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("erpx_inventory_write.business_records"));
        assert!(write.sql.contains("inventory_record_holding_isolation"));
        assert!(read.sql.contains("inventory.lifecycle-records"));
        assert!(
            read.sql
                .contains("inventory_business_record_read_holding_isolation")
        );
        assert!(write.sql.contains("inventory_write.reservation_records"));
        assert!(
            write
                .sql
                .contains("inventory_reservation_record_holding_isolation")
        );
        assert!(read.sql.contains("inventory_read.reservation_projection"));
        assert!(
            read.sql
                .contains("inventory_reservation_read_holding_isolation")
        );
    }
}
