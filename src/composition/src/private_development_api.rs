//! Explicit private development assembly. No environment discovery, migrations,
//! public listener, installed service, business readiness or legacy backend proxy.
#[cfg(feature = "business-http")]
use crate::{
    business_http::{BusinessHttpTarget, business_router},
    finance_journal_create::{FinanceJournalCreateConfiguration, FinanceJournalCreateService},
    finance_journal_create_http::{FinanceJournalCreateTarget, finance_journal_create_router},
    finance_journal_edit::FinanceJournalEditService,
    finance_journal_edit_http::{FinanceJournalEditTarget, finance_journal_edit_router},
    finance_journal_review::{FinanceJournalConfiguration, FinanceJournalReviewAdministration},
};
use crate::{
    field_service_http::{FieldServiceHttpTarget, field_service_http_router},
    field_service_parts::{
        FieldServicePartsAdministration, FieldServicePartsConfiguration,
        FieldServicePartsEnvironment, OrderCreateDispatcher, RequestDispatcher,
    },
    field_service_reservation_read::{
        FieldServiceReservationReadConfiguration, FieldServiceReservationReadService,
    },
    field_service_reservation_read_http::{
        FieldServiceReadScopeHeaders, FieldServiceReservationReadTarget,
        field_service_reservation_read_router,
    },
};
use crate::{
    finance_journal_detail::{FinanceJournalDetailService, FinanceJournalReadConfiguration},
    finance_journal_detail_http::{
        FinanceJournalReadScopeHeaders, FinanceJournalReadTarget, finance_journal_read_router,
    },
    finance_journal_environment::FinanceJournalEnvironment,
};
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::{
    AccountAuthentication, AccountPolicy, Argon2Passwords, AuthenticationSecrets, HoldingOrigin,
    PostgresAccountRepository, SystemAccountClock, SystemAuthenticationSecrets,
    account_auth_router,
    account_holding_bindings::{
        ConfiguredAccountHolding, ConfiguredHoldingRegistry, lock_holding_binding_sources,
    },
    account_revocation::{AccountHoldingWriter, PostgresAccountWorkContextInvalidation},
    account_session_router,
    account_sso::{
        AccountSso, PostgresAccountSso, SsoPolicy, SystemSsoCryptography, account_sso_router,
    },
    effective_authority::HoldingWorkAuthorizer,
};
use erpx_server_runtime::public::{
    foundation_router,
    postgres::{
        DatabasePool, LeasedDevelopmentDatabase, MigrationTrack, PgRequestLease, PoolLimits,
        load_migrations, verify_current_migration_inventory,
    },
    validate_foundation_address,
};
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, TimestampMillis};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Read,
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Semaphore;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DatabaseDocument {
    url: String,
    lease_token: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HoldingDocument {
    holding_id: String,
    slug: String,
    host: String,
    write_binding: String,
    read_binding: String,
    files_binding: String,
    database: DatabaseDocument,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FinanceReadDocument {
    holding_id: String,
    company_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FieldAttachmentDocument {
    holding_id: String,
    mode: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigurationDocument {
    version: u8,
    mode: String,
    transport: String,
    listen: String,
    account_host: String,
    require_email_verification: bool,
    password_workers: usize,
    maximum_requests: usize,
    request_timeout_ms: u64,
    control: DatabaseDocument,
    holdings: Vec<HoldingDocument>,
    #[serde(default)]
    finance_journal_reads: Vec<FinanceReadDocument>,
    #[serde(default)]
    finance_field_service_invoice_reads: Vec<String>,
    #[serde(default)]
    finance_journal_posts: Vec<String>,
    #[serde(default)]
    finance_field_service_journal_posts: Vec<String>,
    #[serde(default)]
    finance_journal_creates: Vec<String>,
    #[serde(default)]
    finance_journal_edits: Vec<String>,
    #[serde(default)]
    finance_journal_reversals: Vec<String>,
    #[serde(default)]
    field_service_reads: Vec<FinanceReadDocument>,
    #[serde(default)]
    field_service_workspace_reads: Vec<String>,
    #[serde(default)]
    field_service_parts_posting_setups: Vec<String>,
    #[serde(default)]
    field_service_parts_prerequisites: Vec<String>,
    #[serde(default)]
    field_service_stock_setups: Vec<String>,
    #[serde(default)]
    field_service_source_record_reads: Vec<FieldAttachmentDocument>,
    #[serde(default)]
    field_attachment_links: Vec<FieldAttachmentDocument>,
    #[serde(default)]
    field_attachment_binds: Vec<FieldAttachmentDocument>,
    #[serde(default)]
    field_service_work_reads: Vec<String>,
    #[serde(default)]
    field_service_detail_reads: Vec<String>,
    #[serde(default)]
    field_service_reservations: Vec<String>,
    #[serde(default)]
    field_service_issues: Vec<String>,
    #[serde(default)]
    field_service_requests: Vec<String>,
    #[serde(default)]
    field_service_order_creates: Vec<String>,
    #[serde(default)]
    field_service_visits: Vec<String>,
    #[serde(default)]
    field_service_visit_completions: Vec<String>,
    #[serde(default)]
    field_service_visit_recoveries: Vec<String>,
    #[serde(default)]
    field_service_order_cancellations: Vec<String>,
    #[serde(default)]
    field_service_billing_releases: Vec<String>,
    #[serde(default)]
    field_service_agreement_reviews: Vec<String>,
    #[serde(default)]
    field_service_agreement_creates: Vec<String>,
    #[serde(default)]
    account_bootstrap: bool,
}
struct HoldingConfiguration {
    host: String,
    registration: ConfiguredAccountHolding,
    database: LeasedDevelopmentDatabase,
}
/// Only a bounded explicit parser can construct this configuration. Debug never
/// exposes database credentials, key material, cookies or private file contents.
pub struct DevelopmentApiConfiguration {
    listen: SocketAddr,
    policy: Arc<AccountPolicy>,
    password_workers: usize,
    maximum_requests: usize,
    request_timeout: Duration,
    holdings_registry: Arc<ConfiguredHoldingRegistry>,
    control: LeasedDevelopmentDatabase,
    holdings: Vec<HoldingConfiguration>,
    finance_journal_reads: BTreeMap<HoldingId, CompanyId>,
    finance_field_service_invoice_reads: BTreeSet<HoldingId>,
    finance_journal_posts: BTreeSet<HoldingId>,
    finance_field_service_journal_posts: BTreeSet<HoldingId>,
    finance_journal_creates: BTreeSet<HoldingId>,
    finance_journal_edits: BTreeSet<HoldingId>,
    finance_journal_reversals: BTreeSet<HoldingId>,
    field_service_reads: BTreeMap<HoldingId, CompanyId>,
    field_service_workspace_reads: BTreeSet<HoldingId>,
    field_service_parts_posting_setups: BTreeSet<HoldingId>,
    field_service_parts_prerequisites: BTreeSet<HoldingId>,
    field_service_stock_setups: BTreeSet<HoldingId>,
    field_service_source_record_reads:
        BTreeMap<HoldingId, crate::field_service_source_record::SourceRecordMode>,
    field_attachment_links: BTreeMap<HoldingId, bool>,
    field_attachment_binds: BTreeMap<HoldingId, bool>,
    field_service_work_reads: BTreeSet<HoldingId>,
    field_service_detail_reads: BTreeSet<HoldingId>,
    field_service_reservations: BTreeSet<HoldingId>,
    field_service_issues: BTreeSet<HoldingId>,
    field_service_requests: BTreeSet<HoldingId>,
    field_service_order_creates: BTreeSet<HoldingId>,
    field_service_visits: BTreeSet<HoldingId>,
    field_service_visit_completions: BTreeSet<HoldingId>,
    field_service_visit_recoveries: BTreeSet<HoldingId>,
    field_service_order_cancellations: BTreeSet<HoldingId>,
    field_service_billing_releases: BTreeSet<HoldingId>,
    field_service_agreement_reviews: BTreeSet<HoldingId>,
    field_service_agreement_creates: BTreeSet<HoldingId>,
    account_bootstrap: bool,
}
impl fmt::Debug for DevelopmentApiConfiguration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DevelopmentApiConfiguration([REDACTED])")
    }
}
impl DevelopmentApiConfiguration {
    pub fn parse(json: &str) -> Result<Self, &'static str> {
        if json.len() > 32 * 1024 {
            return Err("PRIVATE_API_CONFIGURATION_INVALID");
        }
        let doc: ConfigurationDocument =
            serde_json::from_str(json).map_err(|_| "PRIVATE_API_CONFIGURATION_INVALID")?;
        #[cfg(not(feature = "business-http"))]
        if !doc.finance_journal_posts.is_empty()
            || !doc.finance_field_service_journal_posts.is_empty()
            || !doc.finance_journal_creates.is_empty()
            || !doc.finance_journal_edits.is_empty()
            || !doc.finance_journal_reversals.is_empty()
        {
            return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
        }
        if doc.version != 1
            || doc.mode != "development"
            || doc.transport != "direct-loopback"
            || doc.holdings.len() != 2
            || !(1..=8).contains(&doc.password_workers)
            || !(1..=64).contains(&doc.maximum_requests)
            || !(250..=60_000).contains(&doc.request_timeout_ms)
        {
            return Err("EXPLICIT_LEASED_DEVELOPMENT_API_REQUIRED");
        }
        let listen = validate_foundation_address(&doc.listen)
            .map_err(|_| "DISPOSABLE_PRIVATE_API_PORT_REQUIRED")?;
        let control = LeasedDevelopmentDatabase::parse(&doc.control.url, &doc.control.lease_token)
            .map_err(|_| "EXACT_LEASED_API_DATABASE_REQUIRED")?;
        let mut tokens = BTreeSet::from([doc.control.lease_token]);
        let mut ids = BTreeSet::new();
        let mut slugs = BTreeSet::new();
        let mut hosts = BTreeSet::from([doc.account_host.clone()]);
        let mut writes = BTreeSet::new();
        let mut holdings = Vec::new();
        let mut origins = Vec::new();
        for h in doc.holdings {
            let holding_id =
                HoldingId::new(&h.holding_id).map_err(|_| "PRIVATE_API_HOLDING_INVALID")?;
            if !tokens.insert(h.database.lease_token.clone())
                || !ids.insert(h.holding_id.clone())
                || !slugs.insert(h.slug.clone())
                || !hosts.insert(h.host.clone())
                || !writes.insert(h.write_binding.clone())
                || h.slug.is_empty()
                || h.slug.len() > 128
                || !h
                    .slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
                || [&h.write_binding, &h.read_binding, &h.files_binding]
                    .iter()
                    .any(|s| {
                        s.is_empty()
                            || s.len() > 128
                            || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
            {
                return Err("DISTINCT_EXACT_API_HOLDINGS_REQUIRED");
            }
            let database =
                LeasedDevelopmentDatabase::parse(&h.database.url, &h.database.lease_token)
                    .map_err(|_| "EXACT_LEASED_API_DATABASE_REQUIRED")?;
            origins.push(HoldingOrigin {
                origin: format!("https://{}", h.host),
                host: h.host.clone(),
                audience: format!("holding:development:{}:{}", h.holding_id, h.host),
            });
            holdings.push(HoldingConfiguration {
                host: h.host,
                registration: ConfiguredAccountHolding {
                    holding_id,
                    slug: h.slug,
                    write_binding: h.write_binding,
                    read_binding: h.read_binding,
                    files_binding: h.files_binding,
                },
                database,
            });
        }
        if ids != BTreeSet::from(["h001".to_owned(), "h002".to_owned()]) {
            return Err("EXACT_H001_H002_RESOURCES_REQUIRED");
        }
        let mut finance_journal_reads = BTreeMap::new();
        for read in doc.finance_journal_reads {
            let holding = HoldingId::new(&read.holding_id)
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&read.company_id)
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !ids.contains(&read.holding_id)
                || finance_journal_reads.insert(holding, company).is_some()
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_field_service_invoice_reads = BTreeSet::new();
        for id in doc.finance_field_service_invoice_reads {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_reads.contains_key(&holding)
                || !finance_field_service_invoice_reads.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_journal_posts = BTreeSet::new();
        for id in doc.finance_journal_posts {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_reads.contains_key(&holding)
                || !finance_journal_posts.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_journal_creates = BTreeSet::new();
        for id in doc.finance_journal_creates {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_reads.contains_key(&holding)
                || !finance_journal_creates.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_journal_reversals = BTreeSet::new();
        let mut finance_journal_edits = BTreeSet::new();
        for id in doc.finance_journal_edits {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_reads.contains_key(&holding)
                || !finance_journal_edits.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        for id in doc.finance_journal_reversals {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_reads.contains_key(&holding)
                || !finance_journal_reversals.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_reads = BTreeMap::new();
        for read in doc.field_service_reads {
            let holding = HoldingId::new(&read.holding_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&read.company_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !ids.contains(&read.holding_id)
                || field_service_reads.insert(holding, company).is_some()
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_attachment_links = BTreeMap::new();
        for target in doc.field_attachment_links {
            let holding = HoldingId::new(target.holding_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            let read_only = match target.mode.as_str() {
                "enabled" => false,
                "read-only" => true,
                _ => return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID"),
            };
            if !field_service_reads.contains_key(&holding)
                || field_attachment_links.insert(holding, read_only).is_some()
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_attachment_binds = BTreeMap::new();
        for target in doc.field_attachment_binds {
            let holding = HoldingId::new(target.holding_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            let read_only = match target.mode.as_str() {
                "enabled" => false,
                "read-only" => true,
                _ => return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID"),
            };
            if !field_service_reads.contains_key(&holding)
                || field_attachment_binds.insert(holding, read_only).is_some()
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_source_record_reads = BTreeMap::new();
        for target in doc.field_service_source_record_reads {
            let holding = HoldingId::new(target.holding_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            let mode = match target.mode.as_str() {
                "disabled" => crate::field_service_source_record::SourceRecordMode::Disabled,
                "shadow" => crate::field_service_source_record::SourceRecordMode::Shadow,
                "enforced" => crate::field_service_source_record::SourceRecordMode::Enforced,
                _ => return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID"),
            };
            if !field_service_reads.contains_key(&holding)
                || field_service_source_record_reads
                    .insert(holding, mode)
                    .is_some()
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_parts_prerequisites = BTreeSet::new();
        for id in doc.field_service_parts_prerequisites {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_parts_prerequisites.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_stock_setups = BTreeSet::new();
        for id in doc.field_service_stock_setups {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_stock_setups.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_parts_posting_setups = BTreeSet::new();
        for id in doc.field_service_parts_posting_setups {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_parts_posting_setups.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_workspace_reads = BTreeSet::new();
        for id in doc.field_service_workspace_reads {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_workspace_reads.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_work_reads = BTreeSet::new();
        for id in doc.field_service_work_reads {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_work_reads.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_detail_reads = BTreeSet::new();
        for id in doc.field_service_detail_reads {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_detail_reads.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_reservations = BTreeSet::new();
        for id in doc.field_service_reservations {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_reservations.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_issues = BTreeSet::new();
        for id in doc.field_service_issues {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reservations.contains(&holding)
                || field_service_reads.get(&holding) != finance_journal_reads.get(&holding)
                || !finance_journal_reads.contains_key(&holding)
                || !field_service_issues.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_requests = BTreeSet::new();
        for id in doc.field_service_requests {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_requests.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_order_creates = BTreeSet::new();
        for id in doc.field_service_order_creates {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_order_creates.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_visits = BTreeSet::new();
        for id in doc.field_service_visits {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding) || !field_service_visits.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_visit_completions = BTreeSet::new();
        for id in doc.field_service_visit_completions {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_visit_completions.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_visit_recoveries = BTreeSet::new();
        for id in doc.field_service_visit_recoveries {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_visit_recoveries.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_order_cancellations = BTreeSet::new();
        for id in doc.field_service_order_cancellations {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_order_cancellations.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_billing_releases = BTreeSet::new();
        for id in doc.field_service_billing_releases {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_billing_releases.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_agreement_reviews = BTreeSet::new();
        for id in doc.field_service_agreement_reviews {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_agreement_reviews.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut field_service_agreement_creates = BTreeSet::new();
        for id in doc.field_service_agreement_creates {
            let holding = HoldingId::new(id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || !field_service_agreement_creates.insert(holding)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_field_service_journal_posts = BTreeSet::new();
        for id in doc.finance_field_service_journal_posts {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_posts.contains(&holding)
                || !field_service_issues.contains(&holding)
                || !finance_field_service_journal_posts.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        if doc.account_bootstrap
            && finance_journal_reads.is_empty()
            && field_service_reads.is_empty()
        {
            return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
        }
        let policy = Arc::new(AccountPolicy {
            account_origin: format!("https://{}", doc.account_host),
            account_host: doc.account_host,
            environment: "development".into(),
            require_email_verification: doc.require_email_verification,
            holdings: origins,
        });
        policy
            .validate()
            .map_err(|_| "PRIVATE_API_AUTHORITY_INVALID")?;
        let holdings_registry = Arc::new(
            ConfiguredHoldingRegistry::new(
                &policy,
                holdings.iter().map(|h| h.registration.clone()).collect(),
            )
            .map_err(|_| "PRIVATE_API_BINDINGS_INVALID")?,
        );
        Ok(Self {
            listen,
            policy,
            password_workers: doc.password_workers,
            maximum_requests: doc.maximum_requests,
            request_timeout: Duration::from_millis(doc.request_timeout_ms),
            holdings_registry,
            control,
            holdings,
            finance_journal_reads,
            finance_field_service_invoice_reads,
            finance_journal_posts,
            finance_field_service_journal_posts,
            finance_journal_creates,
            finance_journal_edits,
            finance_journal_reversals,
            field_service_reads,
            field_service_workspace_reads,
            field_service_parts_posting_setups,
            field_service_parts_prerequisites,
            field_service_stock_setups,
            field_service_source_record_reads,
            field_attachment_links,
            field_attachment_binds,
            field_service_work_reads,
            field_service_detail_reads,
            field_service_reservations,
            field_service_issues,
            field_service_requests,
            field_service_order_creates,
            field_service_visits,
            field_service_visit_completions,
            field_service_visit_recoveries,
            field_service_order_cancellations,
            field_service_billing_releases,
            field_service_agreement_reviews,
            field_service_agreement_creates,
            account_bootstrap: doc.account_bootstrap,
        })
    }
    pub fn from_file(path: &Path) -> Result<Self, &'static str> {
        if !path.is_absolute() {
            return Err("PRIVATE_ABSOLUTE_API_CONFIGURATION_REQUIRED");
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits());
        }
        let file = options
            .open(path)
            .map_err(|_| "PRIVATE_API_CONFIGURATION_REQUIRED")?;
        let meta = file
            .metadata()
            .map_err(|_| "PRIVATE_API_CONFIGURATION_REQUIRED")?;
        if !meta.is_file() || meta.len() > 32 * 1024 {
            return Err("PRIVATE_API_CONFIGURATION_REQUIRED");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err("PRIVATE_API_CONFIGURATION_REQUIRED");
            }
        }
        let mut json = String::new();
        file.take(32 * 1024 + 1)
            .read_to_string(&mut json)
            .map_err(|_| "PRIVATE_API_CONFIGURATION_INVALID")?;
        Self::parse(&json)
    }
    pub fn listen_address(&self) -> SocketAddr {
        self.listen
    }
}

pub struct DevelopmentApiRuntime {
    router: Router,
    listen: SocketAddr,
    pools: Vec<DatabasePool>,
}
impl DevelopmentApiRuntime {
    pub fn router(&self) -> Router {
        self.router.clone()
    }
    pub fn listen_address(&self) -> SocketAddr {
        self.listen
    }
    pub async fn close(self) {
        for pool in &self.pools {
            pool.close().await;
        }
    }
}
/// Does not bind a port. Opens only the exact leased candidate databases and
/// performs read-only migration/registry checks before assembling real services.
pub async fn assemble_development_api(
    config: DevelopmentApiConfiguration,
    clock: Arc<dyn Clock + Send + Sync>,
    secrets: Arc<dyn AuthenticationSecrets>,
) -> Result<DevelopmentApiRuntime, &'static str> {
    crate::module_assembly::validate_build()?;
    let limits = PoolLimits {
        max_connections: 3,
        acquire_timeout: Duration::from_secs(5),
    };
    let control = config
        .control
        .connect(limits)
        .await
        .map_err(|_| "PRIVATE_API_DATABASE_UNAVAILABLE")?;
    let mut pools = vec![control.clone()];
    for holding in &config.holdings {
        match holding.database.connect(limits).await {
            Ok(pool) => pools.push(pool),
            Err(_) => {
                for p in &pools {
                    p.close().await;
                }
                return Err("PRIVATE_API_DATABASE_UNAVAILABLE");
            }
        }
    }
    let result = assemble_checked(&config, &pools, clock, secrets).await;
    match result {
        Ok(router) => Ok(DevelopmentApiRuntime {
            router,
            listen: config.listen,
            pools,
        }),
        Err(error) => {
            for pool in &pools {
                pool.close().await;
            }
            Err(error)
        }
    }
}
async fn assemble_checked(
    config: &DevelopmentApiConfiguration,
    pools: &[DatabasePool],
    clock: Arc<dyn Clock + Send + Sync>,
    secrets: Arc<dyn AuthenticationSecrets>,
) -> Result<Router, &'static str> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let control_files = load_migrations(&root, MigrationTrack::Control)
        .await
        .map_err(|_| "PRIVATE_API_REVIEWED_SCHEMA_UNAVAILABLE")?;
    let holding_files = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .map_err(|_| "PRIVATE_API_REVIEWED_SCHEMA_UNAVAILABLE")?;
    verify_current_migration_inventory(&pools[0], MigrationTrack::Control, &control_files)
        .await
        .map_err(|_| "PRIVATE_API_SCHEMA_MISMATCH")?;
    for pool in &pools[1..] {
        verify_current_migration_inventory(pool, MigrationTrack::HoldingWrite, &holding_files)
            .await
            .map_err(|_| "PRIVATE_API_SCHEMA_MISMATCH")?;
    }
    let registry = config.holdings_registry.clone();
    PgRequestLease::new(pools[0].clone())
        .run(move |c| {
            Box::pin(async move {
                lock_holding_binding_sources(c).await?;
                registry.verify_startup(c).await
            })
        })
        .await
        .map_err(|_| "PRIVATE_API_BINDINGS_MISMATCH")?;
    // One shared bounded password worker, never one independent limit per route.
    let passwords = Arc::new(
        Argon2Passwords::new(config.password_workers)
            .map_err(|_| "PRIVATE_API_PASSWORD_WORKER_UNAVAILABLE")?,
    );
    let writers = config
        .holdings
        .iter()
        .enumerate()
        .map(|(i, h)| {
            (
                h.registration.holding_id.clone(),
                AccountHoldingWriter {
                    write_binding: h.registration.write_binding.clone(),
                    pool: pools[i + 1].clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let invalidation = Arc::new(
        PostgresAccountWorkContextInvalidation::new(writers, clock.clone())
            .map_err(|_| "PRIVATE_API_BINDINGS_MISMATCH")?,
    );
    let auth = Arc::new(
        AccountAuthentication::new(
            (*config.policy).clone(),
            Arc::new(
                PostgresAccountRepository::from_control_database(
                    pools[0].clone(),
                    "development".into(),
                    config.policy.require_email_verification,
                    invalidation.clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_ACCOUNT_UNAVAILABLE")?,
            ),
            passwords,
            secrets.clone(),
            clock.clone(),
        )
        .map_err(|_| "PRIVATE_API_ACCOUNT_UNAVAILABLE")?,
    );
    let mut router = foundation_router();
    if config.account_bootstrap {
        let sso_policy = SsoPolicy::new((*config.policy).clone())
            .map_err(|_| "PRIVATE_API_ACCOUNT_UNAVAILABLE")?;
        let sso = Arc::new(AccountSso::new(
            sso_policy.clone(),
            Arc::new(PostgresAccountSso::from_control_database(
                pools[0].clone(),
                sso_policy,
                clock.clone(),
                config.holdings_registry.clone(),
            )),
            Arc::new(SystemSsoCryptography),
            secrets,
            clock.clone(),
        ));
        router = router
            .merge(account_auth_router(auth))
            .merge(account_sso_router(sso));
    } else {
        router = router.merge(account_session_router(auth));
    }
    let mut finance_targets = BTreeSet::new();
    let mut finance_service_invoice_targets = BTreeSet::new();
    #[cfg(feature = "business-http")]
    let mut finance_post_targets = BTreeSet::new();
    #[cfg(not(feature = "business-http"))]
    let finance_post_targets = BTreeSet::new();
    #[cfg(feature = "business-http")]
    let mut finance_create_targets = BTreeSet::new();
    #[cfg(not(feature = "business-http"))]
    let finance_create_targets = BTreeSet::new();
    #[cfg(feature = "business-http")]
    let mut finance_edit_targets = BTreeSet::new();
    #[cfg(not(feature = "business-http"))]
    let finance_edit_targets = BTreeSet::new();
    #[cfg(feature = "business-http")]
    let mut finance_reversal_targets = BTreeSet::new();
    #[cfg(not(feature = "business-http"))]
    let finance_reversal_targets = BTreeSet::new();
    let mut service_read_targets = BTreeSet::new();
    let mut service_workspace_targets = BTreeSet::new();
    let mut service_parts_posting_targets = BTreeSet::new();
    let mut service_parts_prerequisite_targets = BTreeSet::new();
    let mut service_stock_setup_targets = BTreeSet::new();
    let mut field_attachment_targets = BTreeSet::new();
    let mut field_attachment_bind_targets = BTreeSet::new();
    let mut service_work_targets = BTreeSet::new();
    let mut service_source_record_targets = BTreeSet::new();
    let mut service_detail_targets = BTreeSet::new();
    let mut service_reservation_targets = BTreeSet::new();
    let mut service_issue_targets = BTreeSet::new();
    let mut service_request_targets = BTreeSet::new();
    let mut service_order_create_targets = BTreeSet::new();
    let mut service_visit_targets = BTreeSet::new();
    let mut service_billing_targets = BTreeSet::new();
    let mut service_agreement_review_targets = BTreeSet::new();
    let mut service_agreement_create_targets = BTreeSet::new();
    #[cfg(not(feature = "business-http"))]
    if !config.finance_journal_posts.is_empty()
        || !config.finance_field_service_journal_posts.is_empty()
        || !config.finance_journal_creates.is_empty()
        || !config.finance_journal_edits.is_empty()
        || !config.finance_journal_reversals.is_empty()
    {
        return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
    }
    if !config.finance_journal_reads.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue().map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
        );
        let mut targets = Vec::new();
        #[cfg(feature = "business-http")]
        let mut post_targets = Vec::new();
        #[cfg(feature = "business-http")]
        let mut create_targets = Vec::new();
        #[cfg(feature = "business-http")]
        let mut edit_targets = Vec::new();
        for (i, holding) in config.holdings.iter().enumerate() {
            let Some(company) = config
                .finance_journal_reads
                .get(&holding.registration.holding_id)
            else {
                continue;
            };
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[i + 1].clone(),
                    holding.registration.holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
            let configuration = FinanceJournalReadConfiguration::new(
                FinanceJournalEnvironment::Development,
                holding.registration.holding_id.clone(),
                company.clone(),
                catalogue.clone(),
            )
            .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            let configuration = if config
                .field_service_issues
                .contains(&holding.registration.holding_id)
            {
                configuration.with_field_service_parts()
            } else {
                configuration
            };
            let configuration = if config
                .finance_field_service_invoice_reads
                .contains(&holding.registration.holding_id)
            {
                finance_service_invoice_targets
                    .insert((holding.host.clone(), holding.registration.slug.clone()));
                configuration.with_field_service_invoices()
            } else {
                configuration
            };
            finance_targets.insert((holding.host.clone(), holding.registration.slug.clone()));
            #[cfg(feature = "business-http")]
            {
                let post = config
                    .finance_journal_posts
                    .contains(&holding.registration.holding_id);
                let reverse = config
                    .finance_journal_reversals
                    .contains(&holding.registration.holding_id);
                if post || reverse {
                    let action_config = FinanceJournalConfiguration::new_private_actions(
                        FinanceJournalEnvironment::Development,
                        holding.registration.holding_id.clone(),
                        company.clone(),
                        catalogue.clone(),
                        post,
                        reverse,
                    )
                    .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
                    let action_config = if config
                        .finance_field_service_journal_posts
                        .contains(&holding.registration.holding_id)
                    {
                        action_config
                            .with_field_service_post()
                            .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?
                    } else {
                        action_config
                    };
                    let target = (holding.host.clone(), holding.registration.slug.clone());
                    if post {
                        finance_post_targets.insert(target.clone());
                    }
                    if reverse {
                        finance_reversal_targets.insert(target);
                    }
                    post_targets.push(BusinessHttpTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        legal: None,
                        finance: Some(Arc::new(FinanceJournalReviewAdministration::new(
                            authorizer.clone(),
                            action_config,
                            Arc::new(SharedClock(clock.clone())),
                        ))),
                    });
                }
                if config
                    .finance_journal_creates
                    .contains(&holding.registration.holding_id)
                {
                    let create_config = FinanceJournalCreateConfiguration::new(
                        FinanceJournalEnvironment::Development,
                        holding.registration.holding_id.clone(),
                        company.clone(),
                        catalogue.clone(),
                    )
                    .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
                    finance_create_targets
                        .insert((holding.host.clone(), holding.registration.slug.clone()));
                    create_targets.push(FinanceJournalCreateTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        service: Arc::new(FinanceJournalCreateService::new(
                            authorizer.clone(),
                            create_config,
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    });
                }
                if config
                    .finance_journal_edits
                    .contains(&holding.registration.holding_id)
                {
                    let edit_config = FinanceJournalCreateConfiguration::new(
                        FinanceJournalEnvironment::Development,
                        holding.registration.holding_id.clone(),
                        company.clone(),
                        catalogue.clone(),
                    )
                    .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
                    finance_edit_targets
                        .insert((holding.host.clone(), holding.registration.slug.clone()));
                    edit_targets.push(FinanceJournalEditTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        service: Arc::new(FinanceJournalEditService::new(
                            authorizer.clone(),
                            edit_config,
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    });
                }
            }
            targets.push(FinanceJournalReadTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                service: Arc::new(FinanceJournalDetailService::new(
                    authorizer,
                    configuration,
                    Arc::new(SharedClock(clock.clone())),
                )),
            });
        }
        #[cfg(feature = "business-http")]
        {
            if !create_targets.is_empty() {
                router = router.merge(
                    finance_journal_create_router(
                        config.policy.clone(),
                        config.holdings_registry.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        create_targets,
                    )
                    .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
                );
            }
            if !edit_targets.is_empty() {
                router = router.merge(
                    finance_journal_edit_router(
                        config.policy.clone(),
                        config.holdings_registry.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        edit_targets,
                    )
                    .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
                );
            }
            if !post_targets.is_empty() {
                router = router.merge(
                    business_router(
                        config.policy.clone(),
                        config.holdings_registry.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        post_targets,
                    )
                    .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
                );
            }
        }
        router = router.merge(
            finance_journal_read_router(
                config.policy.clone(),
                config.holdings_registry.clone(),
                Arc::new(FinanceJournalReadScopeHeaders),
                targets,
            )
            .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
        );
    }
    if !config.field_service_reads.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue()
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?,
        );
        let mut reads = Vec::new();
        let mut reserves = Vec::new();
        for (i, holding) in config.holdings.iter().enumerate() {
            let Some(company) = config
                .field_service_reads
                .get(&holding.registration.holding_id)
            else {
                continue;
            };
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[i + 1].clone(),
                    holding.registration.holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?,
            );
            let target = (holding.host.clone(), holding.registration.slug.clone());
            if config
                .field_service_issues
                .contains(&holding.registration.holding_id)
            {
                service_issue_targets.insert(target.clone());
            }
            if config
                .field_service_requests
                .contains(&holding.registration.holding_id)
            {
                service_request_targets.insert(target.clone());
            }
            if config
                .field_service_order_creates
                .contains(&holding.registration.holding_id)
            {
                service_order_create_targets.insert(target.clone());
            }
            service_read_targets.insert(target.clone());
            let read_configuration = FieldServiceReservationReadConfiguration::new(
                FieldServicePartsEnvironment::Development,
                holding.registration.holding_id.clone(),
                company.clone(),
                catalogue.clone(),
            )
            .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            let read_configuration = if config
                .field_service_stock_setups
                .contains(&holding.registration.holding_id)
            {
                service_stock_setup_targets.insert(target.clone());
                read_configuration.with_stock_setup()
            } else {
                read_configuration
            };
            let read_configuration = if config
                .field_service_parts_prerequisites
                .contains(&holding.registration.holding_id)
            {
                service_parts_prerequisite_targets.insert(target.clone());
                read_configuration.with_parts_prerequisites()
            } else {
                read_configuration
            };
            let read_configuration = if config
                .field_service_parts_posting_setups
                .contains(&holding.registration.holding_id)
            {
                service_parts_posting_targets.insert(target.clone());
                read_configuration.with_parts_posting_setup()
            } else {
                read_configuration
            };
            let read_configuration = if config
                .field_service_requests
                .contains(&holding.registration.holding_id)
            {
                read_configuration.with_requests()
            } else {
                read_configuration
            };
            let read_configuration = if config
                .field_service_order_creates
                .contains(&holding.registration.holding_id)
            {
                read_configuration.with_order_creates()
            } else {
                read_configuration
            };
            let read_configuration = if config
                .field_service_issues
                .contains(&holding.registration.holding_id)
            {
                read_configuration.with_issues()
            } else {
                read_configuration
            };
            let read_configuration = if let Some(read_only) = config
                .field_attachment_links
                .get(&holding.registration.holding_id)
            {
                field_attachment_targets.insert(target.clone());
                read_configuration.with_attachments(*read_only)
            } else {
                read_configuration
            };
            let read_configuration = if let Some(read_only) = config
                .field_attachment_binds
                .get(&holding.registration.holding_id)
            {
                field_attachment_bind_targets.insert(target.clone());
                read_configuration.with_attachment_binds(*read_only)
            } else {
                read_configuration
            };
            let workspace_enabled = config
                .field_service_workspace_reads
                .contains(&holding.registration.holding_id);
            if workspace_enabled {
                service_workspace_targets.insert(target.clone());
            }
            let read_configuration = if workspace_enabled {
                read_configuration.with_workspace()
            } else {
                read_configuration
            };
            let read_configuration = if let Some(mode) = config
                .field_service_source_record_reads
                .get(&holding.registration.holding_id)
            {
                service_source_record_targets.insert(target.clone());
                read_configuration.with_source_record(*mode)
            } else {
                read_configuration
            };
            let work_enabled = config
                .field_service_work_reads
                .contains(&holding.registration.holding_id);
            if work_enabled {
                service_work_targets.insert(target.clone());
            }
            let read_configuration = if work_enabled {
                read_configuration.with_work()
            } else {
                read_configuration
            };
            let detail_enabled = config
                .field_service_detail_reads
                .contains(&holding.registration.holding_id);
            if detail_enabled {
                service_detail_targets.insert(target.clone());
            }
            let read_configuration = if detail_enabled {
                read_configuration.with_detail()
            } else {
                read_configuration
            };
            let visit_enabled = config
                .field_service_visits
                .contains(&holding.registration.holding_id);
            let completion_enabled = config
                .field_service_visit_completions
                .contains(&holding.registration.holding_id);
            let recovery_enabled = config
                .field_service_visit_recoveries
                .contains(&holding.registration.holding_id);
            let cancel_enabled = config
                .field_service_order_cancellations
                .contains(&holding.registration.holding_id);
            let billing_enabled = config
                .field_service_billing_releases
                .contains(&holding.registration.holding_id);
            let agreement_review_enabled = config
                .field_service_agreement_reviews
                .contains(&holding.registration.holding_id);
            let agreement_create_enabled = config
                .field_service_agreement_creates
                .contains(&holding.registration.holding_id);
            if agreement_create_enabled {
                service_agreement_create_targets.insert(target.clone());
            }
            let read_configuration = if agreement_create_enabled {
                read_configuration.with_agreement_creates()
            } else {
                read_configuration
            };
            if agreement_review_enabled {
                service_agreement_review_targets.insert(target.clone());
            }
            let read_configuration = if agreement_review_enabled {
                read_configuration.with_agreement_reviews()
            } else {
                read_configuration
            };
            if visit_enabled || completion_enabled || recovery_enabled || cancel_enabled {
                service_visit_targets.insert(target.clone());
            }
            let read_configuration = if visit_enabled {
                read_configuration.with_visits()
            } else {
                read_configuration
            };
            let read_configuration = if completion_enabled {
                read_configuration.with_completions()
            } else {
                read_configuration
            };
            let read_configuration = if recovery_enabled {
                read_configuration.with_recoveries()
            } else {
                read_configuration
            };
            let read_configuration = if cancel_enabled {
                read_configuration.with_cancellations()
            } else {
                read_configuration
            };
            if billing_enabled {
                service_billing_targets.insert(target.clone());
            }
            let read_configuration = if billing_enabled {
                read_configuration.with_billings()
            } else {
                read_configuration
            };
            reads.push(FieldServiceReservationReadTarget {
                host: target.0.clone(),
                holding_slug: target.1.clone(),
                service: Arc::new(FieldServiceReservationReadService::new(
                    authorizer.clone(),
                    read_configuration,
                    Arc::new(SharedClock(clock.clone())),
                )),
            });
            let reserve_enabled = config
                .field_service_reservations
                .contains(&holding.registration.holding_id);
            let request_enabled = config
                .field_service_requests
                .contains(&holding.registration.holding_id);
            let order_create_enabled = config
                .field_service_order_creates
                .contains(&holding.registration.holding_id);
            if reserve_enabled
                || request_enabled
                || order_create_enabled
                || visit_enabled
                || completion_enabled
                || recovery_enabled
                || cancel_enabled
                || billing_enabled
                || agreement_review_enabled
                || agreement_create_enabled
            {
                service_reservation_targets.insert(target.clone());
                let action = FieldServicePartsConfiguration::new(
                    FieldServicePartsEnvironment::Development,
                    holding.registration.holding_id.clone(),
                    company.clone(),
                    erpx_server_commercial::public::purchases::PackageEnforcementMode::Enforced,
                )
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
                let service = Arc::new(FieldServicePartsAdministration::new(
                    authorizer,
                    action,
                    Arc::new(SharedClock(clock.clone())),
                    Arc::new(SharedClock(clock.clone())),
                ));
                let issue_enabled = config
                    .field_service_issues
                    .contains(&holding.registration.holding_id);
                reserves.push(
                    if request_enabled
                        || order_create_enabled
                        || visit_enabled
                        || completion_enabled
                        || recovery_enabled
                        || cancel_enabled
                        || billing_enabled
                        || agreement_review_enabled
                        || agreement_create_enabled
                    {
                        FieldServiceHttpTarget::new_private_creates(
                            target.0,
                            target.1,
                            service,
                            request_enabled.then(|| {
                                Arc::new(RequestDispatcher {
                                    clock: Arc::new(SharedClock(clock.clone())),
                                })
                                    as Arc<dyn crate::field_service_parts::OtherPost>
                            }),
                            order_create_enabled.then(|| {
                                Arc::new(OrderCreateDispatcher {
                                    clock: Arc::new(SharedClock(clock.clone())),
                                })
                                    as Arc<dyn crate::field_service_parts::OtherPost>
                            }),
                            (
                                (visit_enabled
                                    || completion_enabled
                                    || recovery_enabled
                                    || cancel_enabled
                                    || billing_enabled)
                                    .then(|| {
                                        Arc::new(crate::field_service_parts::VisitDispatcher {
                                            allow_visits: visit_enabled,
                                            allow_completion: completion_enabled,
                                            allow_recovery: recovery_enabled,
                                            allow_cancel: cancel_enabled,
                                            allow_billing: billing_enabled,
                                            clock: Arc::new(SharedClock(clock.clone())),
                                        })
                                            as Arc<dyn crate::field_service_parts::OtherPost>
                                    }),
                                agreement_review_enabled.then(|| {
                                    Arc::new(
                                        crate::field_service_parts::AgreementReviewDispatcher {
                                            clock: Arc::new(SharedClock(clock.clone())),
                                        },
                                    )
                                        as Arc<dyn crate::field_service_parts::OtherPost>
                                }),
                                agreement_create_enabled.then(|| {
                                    Arc::new(
                                        crate::field_service_parts::AgreementCreateDispatcher {
                                            clock: Arc::new(SharedClock(clock.clone())),
                                        },
                                    )
                                        as Arc<dyn crate::field_service_parts::OtherPost>
                                }),
                            ),
                            (reserve_enabled, issue_enabled),
                        )
                    } else if issue_enabled {
                        FieldServiceHttpTarget::new_private_parts(target.0, target.1, service)
                    } else {
                        FieldServiceHttpTarget::new_private_reserve(target.0, target.1, service)
                    },
                );
            }
        }
        router = router.merge(
            field_service_reservation_read_router(
                config.policy.clone(),
                config.holdings_registry.clone(),
                Arc::new(FieldServiceReadScopeHeaders),
                reads,
            )
            .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?,
        );
        if !reserves.is_empty() {
            router = router.merge(
                field_service_http_router(
                    config.policy.clone(),
                    config.holdings_registry.clone(),
                    Arc::new(FieldServiceReadScopeHeaders),
                    reserves,
                )
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?,
            );
        }
    }
    let state = Arc::new(Boundary {
        policy: config.policy.clone(),
        loopback: config.listen.to_string(),
        permits: Arc::new(Semaphore::new(config.maximum_requests)),
        timeout: config.request_timeout,
        finance_targets,
        finance_service_invoice_targets,
        finance_post_targets,
        finance_create_targets,
        finance_edit_targets,
        finance_reversal_targets,
        service_read_targets,
        service_workspace_targets,
        service_parts_posting_targets,
        service_parts_prerequisite_targets,
        service_stock_setup_targets,
        field_attachment_targets,
        field_attachment_bind_targets,
        service_work_targets,
        service_source_record_targets,
        service_detail_targets,
        service_reservation_targets,
        service_issue_targets,
        service_request_targets,
        service_order_create_targets,
        service_visit_targets,
        service_billing_targets,
        service_agreement_review_targets,
        service_agreement_create_targets,
        account_bootstrap: config.account_bootstrap,
    });
    Ok(router.layer(middleware::from_fn_with_state(state, boundary)))
}
struct SharedClock(Arc<dyn Clock + Send + Sync>);
impl Clock for SharedClock {
    fn now(&self) -> TimestampMillis {
        self.0.now()
    }
}
struct Boundary {
    policy: Arc<AccountPolicy>,
    loopback: String,
    permits: Arc<Semaphore>,
    timeout: Duration,
    finance_targets: BTreeSet<(String, String)>,
    finance_service_invoice_targets: BTreeSet<(String, String)>,
    finance_post_targets: BTreeSet<(String, String)>,
    finance_create_targets: BTreeSet<(String, String)>,
    finance_edit_targets: BTreeSet<(String, String)>,
    finance_reversal_targets: BTreeSet<(String, String)>,
    service_read_targets: BTreeSet<(String, String)>,
    service_workspace_targets: BTreeSet<(String, String)>,
    service_parts_posting_targets: BTreeSet<(String, String)>,
    service_parts_prerequisite_targets: BTreeSet<(String, String)>,
    service_stock_setup_targets: BTreeSet<(String, String)>,
    field_attachment_targets: BTreeSet<(String, String)>,
    field_attachment_bind_targets: BTreeSet<(String, String)>,
    service_work_targets: BTreeSet<(String, String)>,
    service_source_record_targets: BTreeSet<(String, String)>,
    service_detail_targets: BTreeSet<(String, String)>,
    service_reservation_targets: BTreeSet<(String, String)>,
    service_issue_targets: BTreeSet<(String, String)>,
    service_request_targets: BTreeSet<(String, String)>,
    service_order_create_targets: BTreeSet<(String, String)>,
    service_visit_targets: BTreeSet<(String, String)>,
    service_billing_targets: BTreeSet<(String, String)>,
    service_agreement_review_targets: BTreeSet<(String, String)>,
    service_agreement_create_targets: BTreeSet<(String, String)>,
    account_bootstrap: bool,
}
enum BoundaryDenial {
    InvalidAuthority,
    ProxyMetadata,
    NotFound,
}
impl BoundaryDenial {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidAuthority => {
                rejected(StatusCode::BAD_REQUEST, "INVALID_REQUEST_AUTHORITY")
            }
            Self::ProxyMetadata => rejected(StatusCode::FORBIDDEN, "UNTRUSTED_PROXY_METADATA"),
            Self::NotFound => (StatusCode::NOT_FOUND, "Not Found").into_response(),
        }
    }
}
fn validate_request(state: &Boundary, request: &Request) -> Result<(), BoundaryDenial> {
    let mut values = request.headers().get_all(header::HOST).iter();
    let host = values
        .next()
        .and_then(|h| h.to_str().ok())
        .ok_or(BoundaryDenial::InvalidAuthority)?;
    if values.next().is_some()
        || request.uri().authority().is_some()
        || request.uri().scheme().is_some()
        || request.headers().get_all(header::ORIGIN).iter().count() > 1
    {
        return Err(BoundaryDenial::InvalidAuthority);
    }
    if [
        "forwarded",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-forwarded-port",
        "x-original-host",
    ]
    .iter()
    .any(|h| request.headers().contains_key(*h))
    {
        return Err(BoundaryDenial::ProxyMetadata);
    }
    if state.policy.audience(host).is_none()
        && !(host == state.loopback && matches!(request.uri().path(), "/api/health" | "/api/ready"))
    {
        return Err(BoundaryDenial::NotFound);
    }
    let finance_workspace = match request
        .uri()
        .path()
        .split('/')
        .collect::<Vec<_>>()
        .as_slice()
    {
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "finance",
            "invoices",
            id,
            "workspace",
        ] if !id.is_empty() => state
            .finance_service_invoice_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "finance",
            "journals",
            id,
            "workspace",
        ] if !id.is_empty() => state
            .finance_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        ["", "api", "v1", "holdings", slug, "finance", "journals", id] if !id.is_empty() => {
            state
                .service_issue_targets
                .contains(&(host.to_owned(), (*slug).to_owned()))
                && state
                    .finance_targets
                    .contains(&(host.to_owned(), (*slug).to_owned()))
        }
        _ => false,
    };
    let finance_create = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["", "api", "v1", "holdings", slug, "finance", "journals"] => state
                .finance_create_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_edit = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "finance",
                "journals",
                id,
                "edit",
            ] if !id.is_empty() => state
                .finance_edit_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_post = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "finance",
                "journals",
                id,
                "post",
            ] if !id.is_empty() => state
                .finance_post_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_reverse = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "finance",
                "journals",
                id,
                "reverse",
            ] if !id.is_empty() => state
                .finance_reversal_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let service_read = match request
        .uri()
        .path()
        .split('/')
        .collect::<Vec<_>>()
        .as_slice()
    {
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "orders",
            id,
            "detail",
        ] if !id.is_empty() => state
            .service_detail_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "workspace",
        ] => state
            .service_workspace_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "orders",
            id,
            "source-record",
        ] if !id.is_empty() => state
            .service_source_record_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        ["", "api", "v1", "holdings", slug, "field-service", "work"] => state
            .service_work_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            order,
            kind @ ("reservations" | "issues"),
            reservation,
        ] if !order.is_empty()
            && !reservation.is_empty()
            && (kind == &"reservations"
                || state
                    .service_issue_targets
                    .contains(&(host.to_owned(), (*slug).to_owned()))) =>
        {
            state
                .service_read_targets
                .contains(&(host.to_owned(), (*slug).to_owned()))
        }
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "agreements",
            "creates",
            key,
        ] if !key.is_empty() => state
            .service_agreement_create_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "agreements",
            "reviews",
            key,
        ] if !key.is_empty() => state
            .service_agreement_review_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "billings",
            key,
        ] if !key.is_empty() => state
            .service_billing_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "visits",
            key,
        ] if !key.is_empty() => state
            .service_visit_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "requests",
            key,
        ] if !key.is_empty() => state
            .service_request_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "field-service",
            "orders",
            key,
        ] if !key.is_empty() => state
            .service_order_create_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let service_parts_posting = match (
        request.method(),
        request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice(),
    ) {
        (
            &axum::http::Method::GET | &axum::http::Method::POST,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "field-service",
                "parts-setup",
            ],
        )
        | (
            &axum::http::Method::POST,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "field-service",
                "parts-setup",
                "existing",
            ],
        ) => state
            .service_parts_posting_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let service_parts_prerequisite = match (
        request.method(),
        request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice(),
    ) {
        (
            &axum::http::Method::GET,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "field-service",
                "parts-setup",
                "prerequisites",
            ],
        )
        | (
            &axum::http::Method::POST,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "field-service",
                "parts-setup",
                "charts" | "accounts" | "periods" | "rates",
            ],
        ) => state
            .service_parts_prerequisite_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let service_stock_setup = match (
        request.method(),
        request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice(),
    ) {
        (
            &axum::http::Method::GET,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "field-service",
                "parts-setup",
                "stock",
            ],
        )
        | (
            &axum::http::Method::POST,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "field-service",
                "parts-setup",
                "masters",
                "products" | "warehouses",
            ],
        ) => state
            .service_stock_setup_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let service_reserve = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["", "api", "v1", "holdings", slug, "field-service"] => state
                .service_reservation_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let field_attachment = match (
        request.method(),
        request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice(),
    ) {
        (
            &axum::http::Method::GET,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "attachment-links",
                id,
                "status",
            ],
        )
        | (
            &axum::http::Method::POST,
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "attachment-links",
                id,
                "withdraw",
            ],
        ) if !id.is_empty() => state
            .field_attachment_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let field_attachment_bind = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "attachment-sets",
                id,
                "bind",
            ] if !id.is_empty() => state
                .field_attachment_bind_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let bootstrap = state.account_bootstrap
        && match (request.method(), request.uri().path()) {
            (&axum::http::Method::POST, "/api/auth/sign-in/email") => {
                host == state.policy.account_host
            }
            (&axum::http::Method::GET, "/sso/authorize") => host == state.policy.account_host,
            (&axum::http::Method::GET, "/sso/start" | "/sso/callback") => state
                .finance_targets
                .iter()
                .chain(state.service_read_targets.iter())
                .any(|(target_host, _)| target_host == host),
            _ => false,
        };
    if !(matches!(
        request.uri().path(),
        "/api/auth/get-session" | "/api/health" | "/api/ready"
    ) || finance_workspace
        || finance_post
        || finance_create
        || finance_edit
        || finance_reverse
        || service_read
        || service_parts_posting
        || service_parts_prerequisite
        || service_stock_setup
        || service_reserve
        || field_attachment
        || field_attachment_bind
        || bootstrap)
        || !(bootstrap
            || finance_post
            || finance_create
            || finance_edit
            || finance_reverse
            || service_reserve
            || service_parts_posting
            || service_parts_prerequisite
            || service_stock_setup
            || field_attachment
            || field_attachment_bind
            || matches!(
                *request.method(),
                axum::http::Method::GET | axum::http::Method::HEAD
            ))
    {
        return Err(BoundaryDenial::NotFound);
    }
    Ok(())
}
async fn boundary(State(state): State<Arc<Boundary>>, request: Request, next: Next) -> Response {
    if let Err(denial) = validate_request(&state, &request) {
        return private(denial.into_response());
    }
    let Ok(permit) = state.permits.clone().try_acquire_owned() else {
        return private(rejected(
            StatusCode::SERVICE_UNAVAILABLE,
            "PRIVATE_API_BUSY",
        ));
    };
    let result = tokio::time::timeout(state.timeout, next.run(request)).await;
    drop(permit);
    private(result.unwrap_or_else(|_| {
        rejected(
            StatusCode::SERVICE_UNAVAILABLE,
            "PRIVATE_API_REQUEST_TIMEOUT",
        )
    }))
}
fn rejected(status: StatusCode, code: &'static str) -> Response {
    (status,axum::Json(serde_json::json!({"error":{"code":code,"message":"The private API request is unavailable."}}))).into_response()
}
fn private(mut response: Response) -> Response {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

/// Explicit CLI assembly only. This does not install or select the native API
/// for any gateway/Next process, and the mandatory configuration remains leased
/// disposable Development configuration even when this feature is compiled.
pub async fn run_development_api_file(
    path: &Path,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), &'static str> {
    let configuration = DevelopmentApiConfiguration::from_file(path)?;
    let runtime = assemble_development_api(
        configuration,
        Arc::new(SystemAccountClock),
        Arc::new(SystemAuthenticationSecrets),
    )
    .await?;
    let listener = match tokio::net::TcpListener::bind(runtime.listen_address()).await {
        Ok(listener) => listener,
        Err(_) => {
            runtime.close().await;
            return Err("PRIVATE_API_LISTEN_FAILED");
        }
    };
    eprintln!(
        "Private development session API listening on {}; readiness remains unavailable.",
        runtime.listen_address()
    );
    let result = axum::serve(listener, runtime.router())
        .with_graceful_shutdown(shutdown)
        .await;
    runtime.close().await;
    result.map_err(|_| "PRIVATE_API_HTTP_FAILED")
}
