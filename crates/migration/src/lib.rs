//! Deterministic, anonymized source fixtures used by migration and
//! reconciliation tests. The fixture contains no credentials or customer data.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

pub mod g13;

/// The control-plane write migration set is applied only to the control write
/// database by the server/worker composition roots.
///
/// Keep this composition root in sync with the embedded migration directory;
/// changing this marker also makes migration additions visible to incremental
/// builds that cache the SQLx embed macro. Authorization migration 0006,
/// provisioning metadata migration 0007, customer scope bindings migration
/// 0008, Stripe webhook audit migration 0009, object access bindings migration
/// 0010, durable worker readiness migration 0011, and support module binding
/// migration 0013 are part of the control write schema. The tenant write and
/// read sets include Operations migrations 0012 and 0002 respectively. The
/// tenant-write set also includes the Finance banking import/reconciliation
/// migration 0028, the tenant-read banking projection migration 0029, and the
/// Marketing Journey write migrations 0044-0045 and Marketing content write
/// migration 0047 are part of the tenant-write set. The tenant-read set also
/// includes Marketing content projection migrations 0036-0037 and the
/// payload-free, rebuildable G13 target import projection 0040. The G13
/// projection never becomes a command source.
/// Control migration 0047 is the durable anonymous Marketing abuse window;
/// it stores only coarse tenant/holding/hostname/resource counters. Control
/// migration 0048 adds the recurring Marketing delivery scheduler lease and
/// dead-letter tables; it coordinates workers without becoming tenant source
/// state.
/// Control migration 0049 adds the Marketing provider onboarding lifecycle,
/// channel capabilities, and verification metadata. It stores no secret
/// values. Control migration 0050 adds account-bound channel routes without
/// credential material. Control migration 0051 adds bounded deterministic
/// provider-account priority metadata. Control migration 0052 plus forward
/// migration 0054 add durable, redacted tenant backup/restore-test job
/// coordination and attempt/run attribution. Control read migration 0007
/// adds the secret-free rebuildable backup-job history projection. Control
/// migration 0053 adds tenant-bound, bounded backup retention actions; forward
/// migration 0055 enforces their composite tenant/holding binding; control read
/// migration 0008 adds their secret-free rebuildable report projection. Control
/// migration 0056 adds bounded object-inventory reconciliation evidence and
/// control read migration 0009 adds its rebuildable projection.
/// Tenant-write migration 0051 adds the tenant-scoped Operations SLA
/// notification inbox and claim/lease state; it never stores provider
/// credentials or message bodies.
/// Tenant-write migration 0052 adds append-only Thai statutory tax facts
/// linked to immutable posted journal lines; tenant-read migration 0041 adds
/// only their rebuildable, payload-free tax report projection.
/// Control migration 0057 adds tenant-bound, two-person owner-seeding metadata;
/// recovery material remains in the identity write schema and is never copied
/// to a control read database. Control migration 0058 adds durable G13 run,
/// transition, idempotency, checkpoint, work-item, reconciliation, and rollback
/// evidence state; control-read migration 0011 adds only rebuildable migration
/// status and reconciliation projections. Control migration 0059 adds bounded
/// read-only Legacy source snapshot evidence and control-read migration 0012
/// projects only source hash/count metadata. Tenant-write migration 0050 adds
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
/// Control migration 0064 adds composite tenant/holding/worker binding
/// constraints to the tenant projection scheduler. It is defense-in-depth
/// for recovery and direct-SQL paths; scheduler authorization remains in the
/// repository and worker composition.
/// Control migration 0065 requires content-addressed source fences on G13
/// work items; repository enqueue paths additionally bind each fence to the
/// immutable migration run source contract hash.
/// Tenant-read migration 0042 adds the durable Marketing delivery projection
/// cursor; it stores only source-event position metadata and is rebuilt from
/// the tenant-write delivery outbox.
/// Control migration 0067 makes physical PITR cluster-scoped, rejects new
/// tenant PITR jobs, and stores only redacted chain, WAL continuity, recovery
/// point, rehearsal, retention, and promotion-audit metadata.
/// Control migration 0068 adds payload-free cluster recovery markers used by
/// isolated local timestamp/LSN rehearsals.
/// Control migration 0069 adds exact customer/tenant/root-holding bindings,
/// hashed 30-minute platform tenant-administration contexts, Stripe binding
/// metadata, bounded grace extensions, tenant-visible platform access audit,
/// and rebuildable tenant/invoice summaries. It performs no automatic
/// binding, activation, billing, suspension, or deletion.
/// Control migration 0070 adds ERPX-managed/customer-data-plane profiles,
/// hash-only agent enrollment and Root capabilities, signed desired/observed
/// configuration state, customer approval records, and canonical business-unit
/// configuration. It stores secret references only and defaults already-bound
/// tenants to ERPX-managed execution without changing routing.
pub static CONTROL_PLANE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The control read database receives only rebuildable projection tables.
pub static CONTROL_READ_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_read");

/// Per-holding write and read schemas are independently rebuildable and never
/// share the control-plane migration directory.
pub static TENANT_WRITE_MIGRATOR: sqlx::migrate::Migrator =
    sqlx::migrate!("./migrations_tenant_write");
pub static TENANT_READ_MIGRATOR: sqlx::migrate::Migrator =
    sqlx::migrate!("./migrations_tenant_read");

pub async fn migrate_control_plane(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    CONTROL_PLANE_MIGRATOR.run(pool).await
}

pub async fn migrate_control_read(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    CONTROL_READ_MIGRATOR.run(pool).await
}

pub async fn migrate_tenant_write(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    TENANT_WRITE_MIGRATOR.run(pool).await
}

pub async fn migrate_tenant_read(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    TENANT_READ_MIGRATOR.run(pool).await
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
        assert!(CONTROL_PLANE_MIGRATOR.iter().count() >= 3);
        let initial = CONTROL_PLANE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 1)
            .expect("initial migration");
        let command_receipts = CONTROL_PLANE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 2)
            .expect("command receipt migration");
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
            .find(|migration| migration.version == 3)
            .expect("platform audit migration");
        assert!(audits.sql.contains("erpx_control.global_read_audits"));
        assert!(CONTROL_READ_MIGRATOR.iter().count() >= 5);
        let support_sla = CONTROL_PLANE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 17)
            .expect("support SLA notification migration");
        assert!(support_sla.sql.contains("support_sla_notifications"));
        assert!(support_sla.sql.contains("claim_expires_at"));
        let read = CONTROL_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 1)
            .expect("initial read migration");
        let detail_reads = CONTROL_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 2)
            .expect("Customer 360 detail read migration");
        assert!(read.sql.contains("erpx_control.customers"));
        assert!(!read.sql.contains("outbox_events"));
        assert!(
            detail_reads
                .sql
                .contains("erpx_control.customer_access_audit_summaries")
        );
        let incident_reads = CONTROL_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 3)
            .expect("incident projection migration");
        assert!(incident_reads.sql.contains("erpx_control.incidents"));
        assert!(detail_reads.sql.contains("erpx_control.customer_notes"));
        let projection_status = CONTROL_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 4)
            .expect("projection status migration");
        assert!(
            projection_status
                .sql
                .contains("erpx_control.tenant_projection_status")
        );
        let projection_sql = projection_status.sql.to_ascii_lowercase();
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
        assert!(projection_sql.contains("attempts between 0 and 25"));
        assert!(projection_sql.contains("octet_length(last_error) <= 1000"));
        let freshness = CONTROL_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 5)
            .expect("projection freshness migration");
        assert!(freshness.sql.contains("published_at"));
        let checkpoint = CONTROL_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 6)
            .expect("projection status checkpoint migration");
        assert!(
            checkpoint
                .sql
                .contains("tenant_projection_status_checkpoints")
        );
        assert!(checkpoint.sql.contains("source_updated_at"));
        assert!(checkpoint.sql.contains("source_job_id"));
    }

    #[test]
    fn tenant_write_and_read_migrations_are_separate_and_rls_backed() {
        assert!(TENANT_WRITE_MIGRATOR.iter().count() >= 3);
        assert!(TENANT_READ_MIGRATOR.iter().count() >= 3);
        let write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 1)
            .expect("initial tenant write");
        let operations_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 12)
            .expect("Operations tenant write");
        let read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 1)
            .expect("initial tenant read");
        let operations_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 2)
            .expect("Operations tenant read");
        assert!(write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(write.sql.contains("erpx_tenant.outbox_events"));
        assert!(
            operations_write
                .sql
                .contains("erpx_tenant.operations_events")
        );
        assert!(operations_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(!read.sql.contains("outbox_events"));
        assert!(read.sql.contains("erpx_tenant.customer_summary"));
        assert!(
            operations_read
                .sql
                .contains("erpx_tenant.operations_read_rows")
        );
        assert!(operations_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let finance_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 17)
            .expect("Finance tenant write");
        let finance_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 17)
            .expect("Finance tenant read");
        assert!(finance_write.sql.contains("finance_posted_journals"));
        assert!(finance_write.sql.contains("finance_command_idempotency"));
        assert!(finance_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(finance_read.sql.contains("finance_posted_journals"));
        assert!(finance_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let finance_matching = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 30)
            .expect("Finance bank matching tenant write");
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
        let finance_match_facts = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 31)
            .expect("Finance posted journal match facts tenant write");
        assert!(finance_match_facts.sql.contains("total_minor"));
        assert!(
            finance_match_facts
                .sql
                .contains("finance_posted_journals_immutable")
        );
        let finance_match_backfill = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 32)
            .expect("Finance posted journal match facts backfill tenant write");
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
        let finance_budgets = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 33)
            .expect("Finance budgets tenant write");
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
                .contains("FOREIGN KEY (holding_ref, budget_id)")
        );
        assert!(finance_budgets.sql.contains("FORCE ROW LEVEL SECURITY"));
        let finance_close_reviews = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 34)
            .expect("Finance close reviews tenant write");
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
        let finance_close_control_evidence = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 35)
            .expect("Finance close control evidence tenant write");
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
                .contains("NEW.holding_ref")
        );
        assert!(
            finance_close_control_evidence
                .sql
                .contains("FORCE ROW LEVEL SECURITY")
        );
        let finance_close_control_evidence_versions = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 37)
            .expect("Finance close control evidence versions tenant write");
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
        let marketing_crm_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 36)
            .expect("Marketing CRM tenant write");
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
        let marketing_content_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 47)
            .expect("Marketing content tenant write");
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
        assert!(!marketing_content_write.sql.contains("credential"));
        assert!(!marketing_content_write.sql.contains("password"));
        let marketing_content_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 36)
            .expect("Marketing content tenant read projection");
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
        let marketing_content_workflow_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 37)
            .expect("Marketing content workflow tenant read projection");
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
        let marketing_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 18)
            .expect("Marketing tenant write");
        let marketing_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 18)
            .expect("Marketing tenant read");
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
        let marketing_crm_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 30)
            .expect("Marketing CRM tenant read projection");
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
                .contains("tenant_scope_crm_opportunity_read_rows")
        );
        let sales_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 19)
            .expect("Sales tenant write");
        let sales_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 19)
            .expect("Sales tenant read");
        assert!(sales_write.sql.contains("sales_aggregates"));
        assert!(sales_write.sql.contains("sales_command_idempotency"));
        assert!(sales_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(sales_read.sql.contains("sales_aggregates"));
        assert!(sales_read.sql.contains("source_version"));
        assert!(sales_read.sql.contains("sales_projection_checkpoints"));
        assert!(sales_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let scm_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 20)
            .expect("SCM tenant write");
        let scm_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 20)
            .expect("SCM tenant read");
        assert!(scm_write.sql.contains("scm_aggregates"));
        assert!(scm_write.sql.contains("scm_command_idempotency"));
        assert!(scm_write.sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(scm_read.sql.contains("scm_projection_checkpoints"));
        assert!(scm_read.sql.contains("source_version"));
        assert!(scm_read.sql.contains("FORCE ROW LEVEL SECURITY"));
        let asset_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 21)
            .expect("Asset/Maintenance tenant write");
        let asset_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 21)
            .expect("Asset/Maintenance tenant read");
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
        let finance_projection_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 22)
            .expect("Finance projection checkpoint migration");
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
        let marketing_projection_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 24)
            .expect("Marketing projection checkpoint migration");
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
        let finance_subledger_write = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 23)
            .expect("Finance subledger tenant write");
        let finance_subledger_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 23)
            .expect("Finance subledger tenant read");
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
            .find(|migration| migration.version == 25)
            .expect("tenant projection scheduler migration");
        assert!(scheduler.sql.contains("tenant_projection_jobs"));
        assert!(scheduler.sql.contains("tenant_projection_claims"));
        assert!(scheduler.sql.contains("lease_until"));
        let scheduler_binding = CONTROL_PLANE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 64)
            .expect("tenant projection scheduler binding migration");
        for constraint in [
            "tenant_projection_jobs_tenant_holding_fk",
            "tenant_projection_claim_batches_tenant_holding_fk",
            "tenant_projection_claims_tenant_holding_fk",
            "tenant_projection_claims_job_binding_fk",
            "tenant_projection_claims_batch_binding_fk",
            "tenant_projection_rebuilds_tenant_holding_fk",
            "tenant_projection_rebuilds_job_binding_fk",
        ] {
            assert!(scheduler_binding.sql.contains(constraint), "{constraint}");
        }
        let marketing_delivery = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 26)
            .expect("Marketing delivery migration");
        assert!(marketing_delivery.sql.contains("marketing_deliveries"));
        assert!(
            marketing_delivery
                .sql
                .contains("marketing_delivery_idempotency")
        );
        assert!(marketing_delivery.sql.contains("FORCE ROW LEVEL SECURITY"));
        let marketing_delivery_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 26)
            .expect("Marketing delivery read projection migration");
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
        let marketing_crm_read = TENANT_READ_MIGRATOR
            .iter()
            .find(|migration| migration.version == 30)
            .expect("Marketing CRM read projection migration");
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
                .contains("tenant_scope_crm_projection_event_receipts")
        );
        let marketing_callback = TENANT_WRITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 27)
            .expect("Marketing provider callback inbox migration");
        assert!(
            marketing_callback
                .sql
                .contains("marketing_provider_callback_inbox")
        );
        assert!(marketing_callback.sql.contains("FORCE ROW LEVEL SECURITY"));
        let marketing_provider_accounts = CONTROL_PLANE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 26)
            .expect("Marketing provider account migration");
        assert!(
            marketing_provider_accounts
                .sql
                .contains("marketing_provider_accounts")
        );
        assert!(marketing_provider_accounts.sql.contains("verifier_key_ref"));
        let marketing_provider_accounts_outbound = CONTROL_PLANE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 46)
            .expect("Marketing provider account outbound migration");
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
        assert!(!marketing_provider_accounts_outbound.sql.contains("secret"));
        assert!(!marketing_provider_accounts_outbound.sql.contains("token"));
        let finance_bank_provider_accounts = CONTROL_PLANE_MIGRATOR
            .iter()
            .find(|migration| migration.version == 27)
            .expect("Finance bank-provider account migration");
        for column in [
            "finance_bank_provider_accounts",
            "provider_account_ref",
            "provider_key",
            "credential_key_ref",
            "tenant_registry(tenant_id)",
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
}
