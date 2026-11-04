//! Explicit private development assembly. No environment discovery, migrations,
//! public listener, installed service, business readiness or legacy backend proxy.
#[cfg(feature = "finance-field-write")]
use crate::{
    business_http::{BusinessHttpTarget, business_router},
    finance_journal_create::{FinanceJournalCreateConfiguration, FinanceJournalCreateService},
    finance_journal_create_http::{FinanceJournalCreateTarget, finance_journal_create_router},
    finance_journal_edit::FinanceJournalEditService,
    finance_journal_edit_http::{FinanceJournalEditTarget, finance_journal_edit_router},
    finance_journal_review::{FinanceJournalConfiguration, FinanceJournalReviewAdministration},
};
use crate::runtime_database::RuntimeDatabase;
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
    inventory_availability_http::{
        InventoryAvailabilityHttpTarget, inventory_availability_http_router,
    },
    inventory_availability_read::InventoryAvailabilityReadService,
    support_incident_command::SupportIncidentCommand,
    support_incident_http::{SupportIncidentHttpTarget, support_incident_http_router},
    support_evidence_command::SupportEvidenceCommand,
    support_evidence_http::{SupportEvidenceHttpTarget, support_evidence_http_router},
    support_session_command::SupportSessionCommand,
    support_session_http::{SupportSessionHttpTarget, support_session_http_router},
    support_sla_command::{SupportSlaCommand, drain_support_sla},
    support_sla_http::{SupportSlaHttpTarget, support_sla_http_router},
    support_ticket_command::SupportTicketCommand,
    support_read_http::{SupportReadHttpTarget, support_read_http_router},
    support_ticket_http::{SupportTicketHttpTarget, support_ticket_http_router},
    support_ticket_read::SupportTicketReadService,
};
use crate::{
    finance_journal_detail::{FinanceJournalDetailService, FinanceJournalReadConfiguration},
    finance_journal_detail_http::{
        FinanceJournalReadScopeHeaders, FinanceJournalReadTarget, finance_journal_read_router,
    },
    finance_journal_environment::FinanceJournalEnvironment,
};
#[cfg(feature = "finance-field-write")]
use crate::{
    finance_journal_create::FinanceJournalCreateConfiguration as InvoicePostConfiguration,
    finance_service_receipt_http::{FinanceServiceReceiptTarget,finance_service_receipt_router},
    finance_service_invoice_post_http::{FinanceServiceInvoicePostTarget, finance_service_invoice_post_router},
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
        DatabaseConfig, DatabasePool, LeasedDevelopmentDatabase, MigrationTrack, PgRequestLease,
        PoolLimits, load_migrations, verify_current_migration_inventory,
    },
    validate_foundation_address,
};
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, TimestampMillis};
use erpx_server_support::public::sla_drain::{
    DEFAULT_INTERVAL_MS, SlaDrainSchedule, run_sla_drain_tick,
};
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
    #[serde(default)]
    lease_token: Option<String>,
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
    #[serde(default)]
    read_database: Option<DatabaseDocument>,
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
#[serde(rename_all="camelCase",deny_unknown_fields)]
#[cfg_attr(not(feature="files-scan-worker"),allow(dead_code,reason="Decoded only to reject unavailable provider policy in this build"))]
struct FieldFilesPolicyDocument {mode:String,uploads_frozen:bool}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FieldInitialCountDocument {
    holding_id: String,
    files_enabled: bool,
    files_read_only: bool,
    uploads_frozen: bool,
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct ServicePackageDocument {
    provider_holding_id:String,
    holding_ids:Vec<String>,
    review_secret:String,
}
struct ServicePackageConfiguration {
    provider_holding:HoldingId,
    holdings:BTreeSet<HoldingId>,
    signer:Arc<erpx_server_commercial::public::package_review::PackageLifecycleReviewSigner>,
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct AccountSecurityKeyDocument {id:String,key_hex:String}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct AccountSecurityKeysDocument {current:String,keys:Vec<AccountSecurityKeyDocument>}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigurationDocument {
    version: u8,
    mode: String,
    transport: String,
    #[serde(default)]
    environment: Option<String>,
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
    finance_field_service_invoice_posts: Vec<String>,
    #[serde(default)]
    finance_field_service_receipt_writes: Vec<String>,
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
    support_ticket_writes: Vec<FinanceReadDocument>,
    #[serde(default)]
    support_incident_writes: Vec<FinanceReadDocument>,
    #[serde(default)]
    support_session_writes: Vec<FinanceReadDocument>,
    #[serde(default)]
    support_sla_writes: Vec<FinanceReadDocument>,
    #[serde(default)]
    support_evidence_writes: Vec<FinanceReadDocument>,
    #[serde(default)]
    field_service_reads: Vec<FinanceReadDocument>,
    #[serde(default)]
    inventory_availability_reads: Vec<FinanceReadDocument>,
    #[serde(default)]
    field_service_workspace_reads: Vec<String>,
    #[serde(default)]
    field_service_parts_posting_setups: Vec<String>,
    #[serde(default)]
    field_service_parts_prerequisites: Vec<String>,
    #[serde(default)]
    field_service_stock_setups: Vec<String>,
    #[serde(default)]
    field_service_initial_counts: Vec<FieldInitialCountDocument>,
    #[serde(default)]
    field_service_source_record_reads: Vec<FieldAttachmentDocument>,
    #[serde(default)]
    field_attachment_links: Vec<FieldAttachmentDocument>,
    #[serde(default)]
    field_attachment_binds: Vec<FieldAttachmentDocument>,
    #[serde(default)]
    field_files_providers: Option<serde_json::Value>,
    #[serde(default)]
    field_files_policy:Option<FieldFilesPolicyDocument>,
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
    #[serde(default)]
    service_package_lifecycle:Option<ServicePackageDocument>,
    #[serde(default)]
    account_security_keys:Option<AccountSecurityKeysDocument>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Profile {
    Development,
    Production,
}
/// A selected database is either a disposable leased development database or a
/// durable private production database. Both expose the same pool operations.
struct HoldingConfiguration {
    host: String,
    registration: ConfiguredAccountHolding,
    database: RuntimeDatabase,
    read_database: Option<RuntimeDatabase>,
}
/// Only a bounded explicit parser can construct this configuration. Debug never
/// exposes database credentials, key material, cookies or private file contents.
pub struct DevelopmentApiConfiguration {
    profile: Profile,
    environment: String,
    listen: SocketAddr,
    policy: Arc<AccountPolicy>,
    password_workers: usize,
    maximum_requests: usize,
    request_timeout: Duration,
    holdings_registry: Arc<ConfiguredHoldingRegistry>,
    control: RuntimeDatabase,
    holdings: Vec<HoldingConfiguration>,
    finance_journal_reads: BTreeMap<HoldingId, BTreeSet<CompanyId>>,
    finance_field_service_invoice_reads: BTreeSet<HoldingId>,
    finance_field_service_invoice_posts: BTreeSet<HoldingId>,
    finance_field_service_receipt_writes: BTreeSet<HoldingId>,
    finance_journal_posts: BTreeSet<HoldingId>,
    finance_field_service_journal_posts: BTreeSet<HoldingId>,
    finance_journal_creates: BTreeSet<HoldingId>,
    finance_journal_edits: BTreeSet<HoldingId>,
    finance_journal_reversals: BTreeSet<HoldingId>,
    support_ticket_writes: Vec<(HoldingId, CompanyId)>,
    support_incident_writes: Vec<(HoldingId, CompanyId)>,
    support_session_writes: Vec<(HoldingId, CompanyId)>,
    support_sla_writes: Vec<(HoldingId, CompanyId)>,
    support_evidence_writes: Vec<(HoldingId, CompanyId)>,
    field_service_reads: BTreeMap<HoldingId, BTreeSet<CompanyId>>,
    inventory_availability_reads: BTreeMap<HoldingId, BTreeSet<CompanyId>>,
    field_service_workspace_reads: BTreeSet<HoldingId>,
    field_service_parts_posting_setups: BTreeSet<HoldingId>,
    field_service_parts_prerequisites: BTreeSet<HoldingId>,
    field_service_stock_setups: BTreeSet<HoldingId>,
    field_service_initial_counts: BTreeMap<HoldingId, crate::service_initial_physical_stock::CountBindingFeatures>,
    field_service_source_record_reads:
        BTreeMap<HoldingId, crate::field_service_source_record::SourceRecordMode>,
    field_attachment_links: BTreeMap<HoldingId, bool>,
    field_attachment_binds: BTreeMap<HoldingId, bool>,
    #[cfg(feature="files-scan-worker")]
    field_files_providers: Option<serde_json::Value>,
    #[cfg(feature="files-scan-worker")]
    field_files_policy:crate::field_attachment_intake_http::FieldIntakePolicy,
    field_files_companies: BTreeMap<HoldingId,BTreeSet<CompanyId>>,
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
    service_package_lifecycle:Option<ServicePackageConfiguration>,
    account_security_keys:Option<Arc<erpx_server_identity_access::public::account_security::AccountMfaKeyring>>,
}
impl fmt::Debug for DevelopmentApiConfiguration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DevelopmentApiConfiguration([REDACTED])")
    }
}
/// The private production service is loopback-only behind Cloudflare Tunnel.
/// Unlike a disposable development port it may use the installed 3101 slot.
fn validate_private_production_address(address: &str) -> Result<SocketAddr, &'static str> {
    let address: SocketAddr = address
        .parse()
        .map_err(|_| "EXPLICIT_LOOPBACK_ADDRESS_REQUIRED")?;
    if !address.ip().is_loopback()
        || address.port() == 0
        || matches!(
            address.port(),
            3000 | 3001 | 3100 | 3103 | 5432 | 5433 | 5435 | 8080 | 3200..=3299
        )
    {
        return Err("PRIVATE_PRODUCTION_LOOPBACK_PORT_REQUIRED");
    }
    Ok(address)
}
/// Production databases are durable loopback PostgreSQL resources. No lease
/// token, socket, options or remote host may be selected by the document.
fn durable_database(url: &str) -> Result<RuntimeDatabase, &'static str> {
    let endpoint =
        url::Url::parse(url).map_err(|_| "EXACT_PRIVATE_PRODUCTION_DATABASE_REQUIRED")?;
    if !matches!(endpoint.scheme(), "postgres" | "postgresql")
        || endpoint.host_str() != Some("127.0.0.1")
        || endpoint.port() != Some(5432)
        || endpoint.username().is_empty()
        || endpoint.password().is_none_or(str::is_empty)
        || endpoint.path().trim_matches('/').is_empty()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("EXACT_PRIVATE_PRODUCTION_DATABASE_REQUIRED");
    }
    let configuration = DatabaseConfig::from_url(url)
        .map_err(|_| "EXACT_PRIVATE_PRODUCTION_DATABASE_REQUIRED")?;
    let identity = (
        endpoint.path().trim_matches('/').to_owned(),
        endpoint.username().to_owned(),
    );
    Ok(RuntimeDatabase::Durable {
        configuration,
        identity,
    })
}
fn runtime_database(
    document: &DatabaseDocument,
    profile: Profile,
    development_error: &'static str,
) -> Result<RuntimeDatabase, &'static str> {
    match profile {
        Profile::Development => {
            let token = document.lease_token.as_deref().ok_or(development_error)?;
            LeasedDevelopmentDatabase::parse(&document.url, token)
                .map(RuntimeDatabase::Leased)
                .map_err(|_| development_error)
        }
        Profile::Production => {
            if document.lease_token.is_some() {
                return Err("PRODUCTION_API_LEASE_TOKEN_FORBIDDEN");
            }
            durable_database(&document.url)
        }
    }
}
impl DevelopmentApiConfiguration {
    /// Explicit disposable development configuration. Production labels are
    /// rejected here; use `parse_production` for the private production runtime.
    pub fn parse(json: &str) -> Result<Self, &'static str> {
        Self::parse_profile(json, Profile::Development)
    }
    /// Explicit durable private production configuration. It accepts no lease
    /// tokens, requires real Account security keys, email verification, a real
    /// package lifecycle secret and a loopback listener.
    pub fn parse_production(json: &str) -> Result<Self, &'static str> {
        Self::parse_profile(json, Profile::Production)
    }
    fn parse_profile(json: &str, profile: Profile) -> Result<Self, &'static str> {
        if json.len() > 32 * 1024 {
            return Err("PRIVATE_API_CONFIGURATION_INVALID");
        }
        let doc: ConfigurationDocument =
            serde_json::from_str(json).map_err(|_| "PRIVATE_API_CONFIGURATION_INVALID")?;
        #[cfg(not(feature = "finance-field-write"))]
        if !doc.finance_journal_posts.is_empty()
            || !doc.finance_field_service_journal_posts.is_empty()
            || !doc.finance_journal_creates.is_empty()
            || !doc.finance_journal_edits.is_empty()
            || !doc.finance_journal_reversals.is_empty()
        {
            return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
        }
        let environment = match (profile, doc.environment.as_deref()) {
            (Profile::Development, None | Some("development")) => "development".to_owned(),
            (Profile::Production, Some("production")) => "production".to_owned(),
            _ => return Err("PRIVATE_API_ENVIRONMENT_INVALID"),
        };
        let expected_mode = match profile {
            Profile::Development => "development",
            Profile::Production => "production",
        };
        let contributor = doc.account_host == "account.erpx.localhost:3443";
        if doc.version != 1
            || doc.mode != expected_mode
            || doc.transport != "direct-loopback"
            || doc.holdings.len() != if contributor { 3 } else { 2 }
            || !(1..=8).contains(&doc.password_workers)
            || !(1..=64).contains(&doc.maximum_requests)
            || !(250..=60_000).contains(&doc.request_timeout_ms)
        {
            return Err(match profile {
                Profile::Development => "EXPLICIT_LEASED_DEVELOPMENT_API_REQUIRED",
                Profile::Production => "EXPLICIT_PRIVATE_PRODUCTION_API_REQUIRED",
            });
        }
        if profile == Profile::Production
            && (doc.account_bootstrap
                || !doc.require_email_verification
                || doc.account_security_keys.is_none()
                || doc.service_package_lifecycle.is_none())
        {
            return Err("PRODUCTION_API_SECURITY_CONFIGURATION_REQUIRED");
        }
        // Finance still fails closed for production composition. A production
        // document must not mount Finance routes with development semantics.
        if profile == Profile::Production
            && (!doc.finance_journal_reads.is_empty()
                || !doc.finance_field_service_invoice_reads.is_empty()
                || !doc.finance_field_service_invoice_posts.is_empty()
                || !doc.finance_field_service_receipt_writes.is_empty()
                || !doc.finance_journal_posts.is_empty()
                || !doc.finance_field_service_journal_posts.is_empty()
                || !doc.finance_journal_creates.is_empty()
                || !doc.finance_journal_edits.is_empty()
                || !doc.finance_journal_reversals.is_empty())
        {
            return Err("FINANCE_PRODUCTION_COMPOSITION_UNAVAILABLE");
        }
        let listen = match profile {
            Profile::Development => validate_foundation_address(&doc.listen)
                .map_err(|_| "DISPOSABLE_PRIVATE_API_PORT_REQUIRED")?,
            Profile::Production => validate_private_production_address(&doc.listen)?,
        };
        let control = runtime_database(&doc.control, profile, "EXACT_LEASED_API_DATABASE_REQUIRED")?;
        let mut tokens = BTreeSet::new();
        if let Some(token) = &doc.control.lease_token {
            tokens.insert(token.clone());
        }
        let mut urls = BTreeSet::from([doc.control.url.clone()]);
        let mut ids = BTreeSet::new();
        let mut slugs = BTreeSet::new();
        let mut hosts = BTreeSet::from([doc.account_host.clone()]);
        let mut writes = BTreeSet::new();
        let mut holdings = Vec::new();
        let mut origins = Vec::new();
        for h in doc.holdings {
            let holding_id =
                HoldingId::new(&h.holding_id).map_err(|_| "PRIVATE_API_HOLDING_INVALID")?;
            let database = runtime_database(&h.database, profile, "EXACT_LEASED_API_DATABASE_REQUIRED")?;
            if !ids.insert(h.holding_id.clone())
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
            match profile {
                Profile::Development => {
                    let token = h.database.lease_token.as_ref().ok_or("EXACT_LEASED_API_DATABASE_REQUIRED")?;
                    if !tokens.insert(token.clone()) {
                        return Err("DISTINCT_EXACT_API_HOLDINGS_REQUIRED");
                    }
                }
                Profile::Production => {
                    if !urls.insert(h.database.url.clone()) {
                        return Err("DISTINCT_EXACT_API_HOLDINGS_REQUIRED");
                    }
                }
            }
            let read_database = match h.read_database {
                Some(read) => {
                    let selected = runtime_database(&read, profile, "EXACT_LEASED_API_DATABASE_REQUIRED")?;
                    match profile {
                        Profile::Development => {
                            let token = read.lease_token.as_ref().ok_or("EXACT_LEASED_API_DATABASE_REQUIRED")?;
                            if !tokens.insert(token.clone()) {
                                return Err("DISTINCT_EXACT_API_HOLDINGS_REQUIRED");
                            }
                        }
                        Profile::Production => {
                            if !urls.insert(read.url.clone()) {
                                return Err("DISTINCT_EXACT_API_HOLDINGS_REQUIRED");
                            }
                        }
                    }
                    Some(selected)
                }
                None => None,
            };
            origins.push(HoldingOrigin {
                origin: format!("https://{}", h.host),
                host: h.host.clone(),
                audience: format!("holding:{}:{}:{}", environment, h.holding_id, h.host),
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
                read_database,
            });
        }
        let expected_ids = if contributor { BTreeSet::from(["h001".to_owned(), "h002".to_owned(), "h003".to_owned()]) } else { BTreeSet::from(["h001".to_owned(), "h002".to_owned()]) };
        if ids != expected_ids || (contributor && ![("h001", "acme"), ("h002", "northstar"), ("h003", "erpx")].iter().all(|(id,slug)| holdings.iter().any(|h| h.registration.holding_id.as_str() == *id && h.registration.slug == *slug))) {
            return Err("EXACT_H001_H002_RESOURCES_REQUIRED");
        }
        let mut finance_journal_reads: BTreeMap<HoldingId, BTreeSet<CompanyId>> = BTreeMap::new();
        for read in doc.finance_journal_reads {
            let holding = HoldingId::new(&read.holding_id)
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&read.company_id)
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !ids.contains(&read.holding_id)
                || !finance_journal_reads.entry(holding).or_default().insert(company)
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
        let mut finance_field_service_invoice_posts = BTreeSet::new();
        for id in doc.finance_field_service_invoice_posts {
            let holding = HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !cfg!(feature = "finance-field-write")
                || !finance_field_service_invoice_reads.contains(&holding)
                || !finance_field_service_invoice_posts.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_field_service_receipt_writes = BTreeSet::new();
        for id in doc.finance_field_service_receipt_writes {
            let holding=HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !cfg!(feature = "finance-field-write") || !finance_field_service_invoice_posts.contains(&holding) || !finance_field_service_receipt_writes.insert(holding) { return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID"); }
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
        let mut field_service_reads: BTreeMap<HoldingId,BTreeSet<CompanyId>> = BTreeMap::new();
        for read in doc.field_service_reads {
            let holding = HoldingId::new(&read.holding_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&read.company_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !ids.contains(&read.holding_id)
                || !field_service_reads.entry(holding).or_default().insert(company)
            {
                return Err("PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID");
            }
        }
        let mut inventory_availability_reads: BTreeMap<HoldingId, BTreeSet<CompanyId>> =
            BTreeMap::new();
        for read in doc.inventory_availability_reads {
            let holding = HoldingId::new(&read.holding_id)
                .map_err(|_| "PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&read.company_id)
                .map_err(|_| "PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID")?;
            if !ids.contains(&read.holding_id)
                || !inventory_availability_reads
                    .entry(holding)
                    .or_default()
                    .insert(company)
            {
                return Err("PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID");
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
        let account_security_keys=match doc.account_security_keys {
            None=>None,
            Some(input)=>{
                if input.keys.is_empty()||input.keys.len()>8{return Err("PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID");}
                let mut keys=Vec::new();
                for key in input.keys {
                    if key.key_hex.len()!=64{return Err("PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID");}
                    let bytes:[u8;32]=hex::decode(key.key_hex).map_err(|_|"PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID")?.try_into().map_err(|_|"PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID")?;
                    keys.push((key.id,bytes));
                }
                Some(Arc::new(erpx_server_identity_access::public::account_security::AccountMfaKeyring::new(input.current,keys).map_err(|_|"PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID")?))
            }
        };
        let service_package_lifecycle=match doc.service_package_lifecycle {
            None=>None,
            Some(input)=>{
                let provider_holding=HoldingId::new(input.provider_holding_id).map_err(|_|"PRIVATE_API_SERVICE_PACKAGE_CONFIGURATION_INVALID")?;
                if account_security_keys.is_none()||!holdings.iter().any(|h|h.registration.holding_id==provider_holding)||input.holding_ids.is_empty()||input.review_secret.len()>4096{return Err("PRIVATE_API_SERVICE_PACKAGE_CONFIGURATION_INVALID");}
                let mut selected=BTreeSet::new();
                for id in input.holding_ids {
                    let id=HoldingId::new(id).map_err(|_|"PRIVATE_API_SERVICE_PACKAGE_CONFIGURATION_INVALID")?;
                    if !matches!(field_service_source_record_reads.get(&id),Some(crate::field_service_source_record::SourceRecordMode::Enforced))||!selected.insert(id){return Err("PRIVATE_API_SERVICE_PACKAGE_CONFIGURATION_INVALID");}
                }
                let signer=erpx_server_commercial::public::package_review::PackageLifecycleReviewSigner::new(&input.review_secret).map_err(|_|"PRIVATE_API_SERVICE_PACKAGE_CONFIGURATION_INVALID")?;
                Some(ServicePackageConfiguration{provider_holding,holdings:selected,signer:Arc::new(signer)})
            }
        };
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
        let mut field_service_initial_counts = BTreeMap::new();
        for target in doc.field_service_initial_counts {
            let holding = HoldingId::new(target.holding_id)
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            if !field_service_reads.contains_key(&holding)
                || field_service_initial_counts.insert(holding, crate::service_initial_physical_stock::CountBindingFeatures {
                    enabled: target.files_enabled,
                    read_only: target.files_read_only,
                    uploads_frozen: target.uploads_frozen,
                }).is_some()
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
                || !field_service_reads.get(&holding).is_some_and(|required| finance_journal_reads.get(&holding).is_some_and(|companies|required.is_subset(companies)))
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
            environment: environment.clone(),
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
        #[cfg(not(feature="files-scan-worker"))]
        if doc.field_files_providers.is_some() || doc.field_files_policy.is_some(){return Err("PRIVATE_API_FIELD_FILES_FEATURE_REQUIRED");}
        #[cfg(feature="files-scan-worker")]
        let field_files_policy=match &doc.field_files_policy {
            Some(policy) if doc.field_files_providers.is_some()=>crate::field_attachment_intake_http::FieldIntakePolicy{
                mode:match policy.mode.as_str(){"enabled"=>crate::field_attachment_intake_http::FieldIntakeMode::Enabled,"read-only"=>crate::field_attachment_intake_http::FieldIntakeMode::ReadOnly,"disabled"=>crate::field_attachment_intake_http::FieldIntakeMode::Disabled,_=>return Err("PRIVATE_API_FIELD_FILES_POLICY_INVALID")},uploads_frozen:policy.uploads_frozen,
            },
            None if doc.field_files_providers.is_none()=>crate::field_attachment_intake_http::FieldIntakePolicy{mode:crate::field_attachment_intake_http::FieldIntakeMode::Disabled,uploads_frozen:true},
            _=>return Err("PRIVATE_API_FIELD_FILES_POLICY_INVALID"),
        };
        #[cfg(feature="files-scan-worker")]
        let mut field_files_companies=BTreeMap::<HoldingId,BTreeSet<CompanyId>>::new();
        #[cfg(not(feature="files-scan-worker"))]
        let field_files_companies=BTreeMap::<HoldingId,BTreeSet<CompanyId>>::new();
        #[cfg(feature="files-scan-worker")]
        if let Some(ref providers)=doc.field_files_providers {
            let raw=serde_json::to_vec(providers).map_err(|_|"PRIVATE_API_FIELD_FILES_CONFIGURATION_INVALID")?;
            let targets=crate::field_files_runtime_configuration::parse_field_files_runtime_configuration(&raw,&holdings_registry).map_err(|_|"PRIVATE_API_FIELD_FILES_CONFIGURATION_INVALID")?;
            for target in targets {
                if !field_service_reads.contains_key(&target.holding_id) || !field_attachment_links.contains_key(&target.holding_id) || !field_attachment_binds.contains_key(&target.holding_id) {
                    return Err("PRIVATE_API_FIELD_FILES_CONFIGURATION_INVALID");
                }
                if field_files_policy.mode!=crate::field_attachment_intake_http::FieldIntakeMode::Enabled {
                    if let Some(value)=field_attachment_links.get_mut(&target.holding_id){*value=true;}
                    if let Some(value)=field_attachment_binds.get_mut(&target.holding_id){*value=true;}
                }
                field_files_companies.entry(target.holding_id).or_default().insert(target.company_id);
            }
        }
        let mut support_ticket_writes = Vec::new();
        for target in doc.support_ticket_writes {
            let holding = HoldingId::new(&target.holding_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&target.company_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID")?;
            if !holdings.iter().any(|h| h.registration.holding_id == holding)
                || support_ticket_writes
                    .iter()
                    .any(|(h, c)| h == &holding && c == &company)
            {
                return Err("PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID");
            }
            support_ticket_writes.push((holding, company));
        }
        let mut support_incident_writes = Vec::new();
        for target in doc.support_incident_writes {
            let holding = HoldingId::new(&target.holding_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_INCIDENT_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&target.company_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_INCIDENT_CONFIGURATION_INVALID")?;
            if !holdings.iter().any(|h| h.registration.holding_id == holding)
                || support_incident_writes
                    .iter()
                    .any(|(h, c)| h == &holding && c == &company)
            {
                return Err("PRIVATE_API_SUPPORT_INCIDENT_CONFIGURATION_INVALID");
            }
            support_incident_writes.push((holding, company));
        }
        let mut support_session_writes = Vec::new();
        for target in doc.support_session_writes {
            let holding = HoldingId::new(&target.holding_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_SESSION_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&target.company_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_SESSION_CONFIGURATION_INVALID")?;
            if !holdings.iter().any(|h| h.registration.holding_id == holding)
                || support_session_writes
                    .iter()
                    .any(|(h, c)| h == &holding && c == &company)
            {
                return Err("PRIVATE_API_SUPPORT_SESSION_CONFIGURATION_INVALID");
            }
            support_session_writes.push((holding, company));
        }
        let mut support_sla_writes = Vec::new();
        for target in doc.support_sla_writes {
            let holding = HoldingId::new(&target.holding_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&target.company_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID")?;
            if !holdings.iter().any(|h| h.registration.holding_id == holding)
                || support_sla_writes
                    .iter()
                    .any(|(h, c)| h == &holding && c == &company)
            {
                return Err("PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID");
            }
            support_sla_writes.push((holding, company));
        }
        let mut support_evidence_writes = Vec::new();
        for target in doc.support_evidence_writes {
            let holding = HoldingId::new(&target.holding_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_EVIDENCE_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&target.company_id)
                .map_err(|_| "PRIVATE_API_SUPPORT_EVIDENCE_CONFIGURATION_INVALID")?;
            if !holdings.iter().any(|h| h.registration.holding_id == holding)
                || support_evidence_writes
                    .iter()
                    .any(|(h, c)| h == &holding && c == &company)
            {
                return Err("PRIVATE_API_SUPPORT_EVIDENCE_CONFIGURATION_INVALID");
            }
            support_evidence_writes.push((holding, company));
        }
        Ok(Self {
            profile,
            environment,
            #[cfg(feature="files-scan-worker")]
            field_files_providers:doc.field_files_providers,
            #[cfg(feature="files-scan-worker")]
            field_files_policy,
            field_files_companies,
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
            finance_field_service_invoice_posts,
            finance_field_service_receipt_writes,
            finance_journal_posts,
            finance_field_service_journal_posts,
            finance_journal_creates,
            finance_journal_edits,
            finance_journal_reversals,
            support_ticket_writes,
            support_incident_writes,
            support_session_writes,
            support_sla_writes,
            support_evidence_writes,
            field_service_reads,
            inventory_availability_reads,
            field_service_workspace_reads,
            field_service_parts_posting_setups,
            field_service_parts_prerequisites,
            field_service_stock_setups,
            field_service_initial_counts,
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
            service_package_lifecycle,
            account_security_keys,
        })
    }
    pub fn from_file(path: &Path) -> Result<Self, &'static str> {
        Self::read_configuration_file(path, Profile::Development)
    }
    pub fn from_production_file(path: &Path) -> Result<Self, &'static str> {
        Self::read_configuration_file(path, Profile::Production)
    }
    fn read_configuration_file(path: &Path, profile: Profile) -> Result<Self, &'static str> {
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
        Self::parse_profile(&json, profile)
    }
    pub fn listen_address(&self) -> SocketAddr {
        self.listen
    }
}

struct CheckedAssembly {
    router:Router,
    #[cfg(feature="files-scan-worker")]
    files_workers:Vec<crate::files_scan_native::NativeFilesScanWorker<SharedClock>>,
}
/// The assembled private API runtime. Development and production share the
/// same owner routers; only the database binding and environment label differ.
pub struct DevelopmentApiRuntime {
    router: Router,
    listen: SocketAddr,
    pools: Vec<DatabasePool>,
    #[cfg(feature="files-scan-worker")]
    files_workers:Vec<crate::files_scan_native::NativeFilesScanWorker<SharedClock>>,
}
impl DevelopmentApiRuntime {
    pub fn router(&self) -> Router {
        self.router.clone()
    }
    pub fn listen_address(&self) -> SocketAddr {
        self.listen
    }
    pub async fn close(self) {
        #[cfg(feature="files-scan-worker")]
        for worker in self.files_workers {worker.close().await;}
        for pool in &self.pools {
            pool.close().await;
        }
    }
}
/// Does not bind a port. Opens the explicitly selected databases (leased in
/// development, durable in production) and performs read-only migration and
/// registry checks before assembling the real owner services.
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
        Ok(assembly) => Ok(DevelopmentApiRuntime {
            router:assembly.router,
            #[cfg(feature="files-scan-worker")]
            files_workers:assembly.files_workers,
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
) -> Result<CheckedAssembly, &'static str> {
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
                    config.environment.clone(),
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
    if let Some(keys)=&config.account_security_keys {
        use erpx_server_identity_access::public::{account_security::*,account_security_http::account_security_router};
        let mfa=PostgresAccountMfa::new(pools[0].clone(),config.environment.clone(),keys.clone(),clock.clone(),config.holdings_registry.clone()).map_err(|_|"PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID")?;
        let repository=PostgresAccountSecurity::new(pools[0].clone(),config.environment.clone(),clock.clone(),invalidation.clone(),keys.clone(),config.holdings_registry.clone()).map_err(|_|"PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID")?;
        let security=AccountSecurity::new((*config.policy).clone(),Arc::new(mfa),Arc::new(repository),secrets.clone()).map_err(|_|"PRIVATE_API_ACCOUNT_SECURITY_CONFIGURATION_INVALID")?;
        router=router.merge(account_security_router(Arc::new(security)));
    }

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
    #[cfg(feature = "finance-field-write")]
    let mut finance_service_invoice_post_targets = BTreeSet::new();
    #[cfg(not(feature = "finance-field-write"))]
    let finance_service_invoice_post_targets = BTreeSet::new();
    #[cfg(feature = "finance-field-write")]
    let mut finance_service_receipt_targets = BTreeSet::new();
    #[cfg(not(feature = "finance-field-write"))]
    let finance_service_receipt_targets = BTreeSet::new();
    #[cfg(feature = "finance-field-write")]
    let mut finance_post_targets = BTreeSet::new();
    #[cfg(not(feature = "finance-field-write"))]
    let finance_post_targets = BTreeSet::new();
    #[cfg(feature = "finance-field-write")]
    let mut finance_create_targets = BTreeSet::new();
    #[cfg(not(feature = "finance-field-write"))]
    let finance_create_targets = BTreeSet::new();
    #[cfg(feature = "finance-field-write")]
    let mut finance_edit_targets = BTreeSet::new();
    #[cfg(not(feature = "finance-field-write"))]
    let finance_edit_targets = BTreeSet::new();
    #[cfg(feature = "finance-field-write")]
    let mut finance_reversal_targets = BTreeSet::new();
    #[cfg(not(feature = "finance-field-write"))]
    let finance_reversal_targets = BTreeSet::new();
    let mut service_read_targets = BTreeSet::new();
    let mut service_workspace_targets = BTreeSet::new();
    let mut service_parts_posting_targets = BTreeSet::new();
    let mut service_parts_prerequisite_targets = BTreeSet::new();
    let mut service_stock_setup_targets = BTreeSet::new();
    let mut service_initial_count_targets = BTreeSet::new();
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
    #[cfg(not(feature = "finance-field-write"))]
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
        let mut brand_targets = Vec::new();
        let mut organization_targets = Vec::new();
        let mut organization_setup_targets = Vec::new();
        let mut approval_targets = Vec::new();
        let mut handbook_targets = Vec::new();
        let mut learning_targets = Vec::new();
        let mut quality_targets = Vec::new();
        let mut maintenance_targets = Vec::new();
        let mut operations_targets = Vec::new();
        let mut asset_targets = Vec::new();
        let mut bank_targets = Vec::new();
        let mut generic_payment_targets = Vec::new();
        let mut bank_workspace_targets = Vec::new();
        #[cfg(feature = "finance-field-write")]
        let mut invoice_post_targets = Vec::new();
        #[cfg(feature = "finance-field-write")]
        let mut receipt_targets = Vec::new();
        #[cfg(feature = "finance-field-write")]
        let mut post_targets = Vec::new();
        #[cfg(feature = "finance-field-write")]
        let mut create_targets = Vec::new();
        #[cfg(feature = "finance-field-write")]
        let mut edit_targets = Vec::new();
        for (i, holding) in config.holdings.iter().enumerate() {
            let Some(companies) = config
                .finance_journal_reads
                .get(&holding.registration.holding_id)
            else {
                continue;
            };
            // Native journal post and Field handoffs have Company targets.
            // Other journal editing/reversal routers retain their own cutover.
            if companies.len()>1 && [
                &config.finance_journal_reversals,
                &config.finance_journal_creates,&config.finance_journal_edits,
            ].iter().any(|scopes|scopes.contains(&holding.registration.holding_id)) {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
            for company in companies {
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
            generic_payment_targets.push(crate::finance_generic_payment_http::GenericPaymentTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                authorizer: authorizer.clone(),
                configuration: FinanceJournalCreateConfiguration::new(
                    FinanceJournalEnvironment::Development,
                    holding.registration.holding_id.clone(),
                    company.clone(),
                    catalogue.clone(),
                ).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
                clock: Arc::new(SharedClock(clock.clone())),
            });
            bank_targets.push(crate::finance_bank_http::FinanceBankTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                authorizer: authorizer.clone(),
                configuration: FinanceJournalCreateConfiguration::new(
                    FinanceJournalEnvironment::Development,
                    holding.registration.holding_id.clone(),
                    company.clone(),
                    catalogue.clone(),
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
                clock: Arc::new(SharedClock(clock.clone())),
            });
            bank_workspace_targets.push(crate::finance_bank_workspace::FinanceBankWorkspaceTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                service: Arc::new(crate::finance_bank_workspace::FinanceBankWorkspaceService::new(
                    authorizer.clone(),
                    crate::finance_bank_workspace::FinanceBankWorkspaceConfiguration::new(
                        holding.registration.holding_id.clone(),
                        company.clone(),
                        catalogue.clone(),
                    )
                    .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
                    Arc::new(SharedClock(clock.clone())),
                )),
            });
            brand_targets.push(crate::brand_identity_http::BrandIdentityTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                service: Arc::new(crate::brand_identity::BrandIdentityService::new(
                    authorizer.clone(),
                    crate::brand_identity::BrandIdentityConfiguration::new(
                        holding.registration.holding_id.clone(),
                        company.clone(),
                    ),
                    Arc::new(SharedClock(clock.clone())),
                )),
            });
            organization_targets.push(crate::organization_nodes_http::OrganizationNodesTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                service: Arc::new(crate::organization_nodes::OrganizationNodeService::new(
                    authorizer.clone(),
                    crate::organization_nodes::OrganizationNodesConfiguration::new(
                        holding.registration.holding_id.clone(),
                        company.clone(),
                    ),
                    Arc::new(SharedClock(clock.clone())),
                )),
            });
            learning_targets.push(
                erpx_server_learning::public::learning_http::LearningHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    origin: format!("https://{}", holding.host),
                    backend: Arc::new(crate::learning::LearningBackend::new(
                        config.policy.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        Arc::new(crate::learning::LearningService::new(
                            authorizer.clone(),
                            crate::learning::LearningConfiguration::new(
                                holding.registration.holding_id.clone(),
                                company.clone(),
                            ),
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    )),
                },
            );
            quality_targets.push(
                erpx_server_quality::public::quality_http::QualityHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    origin: format!("https://{}", holding.host),
                    backend: Arc::new(crate::quality::QualityBackend::new(
                        config.policy.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        Arc::new(crate::quality::QualityService::new(
                            authorizer.clone(),
                            crate::quality::QualityConfiguration::new(
                                holding.registration.holding_id.clone(),
                                company.clone(),
                            ),
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    )),
                },
            );
            maintenance_targets.push(
                erpx_server_maintenance::public::http::MaintenanceHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    origin: format!("https://{}", holding.host),
                    backend: Arc::new(crate::maintenance::MaintenanceBackend::new(
                        config.policy.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        Arc::new(crate::maintenance::MaintenanceService::new(
                            authorizer.clone(),
                            crate::maintenance::MaintenanceConfiguration::new(
                                holding.registration.holding_id.clone(),
                                company.clone(),
                            ),
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    )),
                },
            );
            operations_targets.push(
                erpx_server_operations::public::operations_http::OperationsHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    origin: format!("https://{}", holding.host),
                    backend: Arc::new(crate::operations::OperationsBackend::new(
                        config.policy.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        Arc::new(crate::operations::OperationsService::new(
                            authorizer.clone(),
                            crate::operations::OperationsConfiguration::new(
                                holding.registration.holding_id.clone(),
                                company.clone(),
                            ),
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    )),
                },
            );
            handbook_targets.push(
                erpx_server_collaboration::public::handbook_http::HandbookHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    origin: format!("https://{}", holding.host),
                    backend: Arc::new(crate::handbook::HandbookBackend::new(
                        config.policy.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        Arc::new(crate::handbook::HandbookService::new(
                            authorizer.clone(),
                            crate::handbook::HandbookConfiguration::new(
                                holding.registration.holding_id.clone(),
                                company.clone(),
                            ),
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    )),
                },
            );
            approval_targets.push(
                erpx_server_workflow::public::approval_oversight_http::ApprovalOversightHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    origin: format!("https://{}", holding.host),
                    backend: Arc::new(crate::approval_oversight::ApprovalOversightBackend::new(
                        config.policy.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        Arc::new(crate::approval_oversight::ApprovalOversightService::new(
                            authorizer.clone(),
                            crate::approval_oversight::ApprovalOversightConfiguration::new(
                                holding.registration.holding_id.clone(),
                                company.clone(),
                            ),
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    )),
                },
            );
            organization_setup_targets.push(
                erpx_server_organization::public::organization_setup_http::OrganizationSetupHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    origin: format!("https://{}", holding.host),
                    backend: Arc::new(crate::organization_setup::OrganizationSetupBackend::new(
                        config.policy.clone(),
                        Arc::new(FinanceJournalReadScopeHeaders),
                        Arc::new(crate::organization_setup::OrganizationSetupService::new(
                            authorizer.clone(),
                            crate::organization_setup::OrganizationSetupConfiguration::new(
                                holding.registration.holding_id.clone(),
                                company.clone(),
                            ),
                            Arc::new(SharedClock(clock.clone())),
                        )),
                    )),
                },
            );
            asset_targets.push(erpx_server_assets::public::asset_http::AssetHttpTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                origin: format!("https://{}", holding.host),
                backend: Arc::new(crate::asset_http::AssetHttpService::new(
                    config.policy.clone(),
                    authorizer.clone(),
                    crate::asset_http::AssetHttpConfiguration::new(
                        holding.registration.holding_id.clone(),
                        company.clone(),
                    ),
                    Arc::new(SharedClock(clock.clone())),
                    Arc::new(crate::asset_http::FinanceAssetCost),
                )),
            });
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
            #[cfg(feature = "finance-field-write")]
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
                        #[cfg(feature = "business-http")]
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
            #[cfg(feature = "finance-field-write")]
            if config.finance_field_service_invoice_posts.contains(&holding.registration.holding_id) {
                let configuration = InvoicePostConfiguration::new(
                    FinanceJournalEnvironment::Development,
                    holding.registration.holding_id.clone(), company.clone(), catalogue.clone(),
                ).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
                finance_service_invoice_post_targets.insert((holding.host.clone(), holding.registration.slug.clone()));
                invoice_post_targets.push(FinanceServiceInvoicePostTarget {
                    host: holding.host.clone(), holding_slug: holding.registration.slug.clone(),
                    authorizer: authorizer.clone(), configuration,
                    clock: Arc::new(SharedClock(clock.clone())),
                });
            }
            #[cfg(feature = "finance-field-write")]
            if config.finance_field_service_receipt_writes.contains(&holding.registration.holding_id) {
                let configuration = InvoicePostConfiguration::new(FinanceJournalEnvironment::Development,holding.registration.holding_id.clone(),company.clone(),catalogue.clone()).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
                finance_service_receipt_targets.insert((holding.host.clone(),holding.registration.slug.clone()));
                let file_features=erpx_server_files::public::payment_proof_binding::PaymentProofBindingFeatures {enabled:false,read_only:false,uploads_frozen:true};
                #[cfg(feature="files-scan-worker")]
                let file_features=if config.field_files_companies.get(&holding.registration.holding_id).is_some_and(|companies|companies.contains(company)) {
                    use crate::field_attachment_intake_http::FieldIntakeMode;
                    erpx_server_files::public::payment_proof_binding::PaymentProofBindingFeatures {enabled:config.field_files_policy.mode==FieldIntakeMode::Enabled,read_only:config.field_files_policy.mode==FieldIntakeMode::ReadOnly,uploads_frozen:config.field_files_policy.uploads_frozen}
                }else{file_features};
                receipt_targets.push(FinanceServiceReceiptTarget {host:holding.host.clone(),holding_slug:holding.registration.slug.clone(),authorizer:authorizer.clone(),configuration,file_features,clock:Arc::new(SharedClock(clock.clone()))});
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
        }
        #[cfg(feature = "finance-field-write")]
        if !invoice_post_targets.is_empty() {
            router = router.merge(finance_service_invoice_post_router(
                config.policy.clone(), config.holdings_registry.clone(),
                Arc::new(FinanceJournalReadScopeHeaders), invoice_post_targets,
            ).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?);
        }
        #[cfg(feature = "finance-field-write")]
        if !receipt_targets.is_empty() {
            router=router.merge(finance_service_receipt_router(config.policy.clone(),config.holdings_registry.clone(),Arc::new(FinanceJournalReadScopeHeaders),receipt_targets).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?);
        }
        #[cfg(feature = "finance-field-write")]
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
        if !generic_payment_targets.is_empty() {
            router = router.merge(
                crate::finance_generic_payment_http::finance_generic_payment_router(
                    config.policy.clone(),
                    config.holdings_registry.clone(),
                    Arc::new(FinanceJournalReadScopeHeaders),
                    generic_payment_targets,
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
        }
        if !bank_targets.is_empty() {
            router = router.merge(
                crate::finance_bank_http::finance_bank_router(
                    config.policy.clone(),
                    config.holdings_registry.clone(),
                    bank_targets,
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
        }
        if !bank_workspace_targets.is_empty() {
            router = router.merge(
                crate::finance_bank_workspace::finance_bank_workspace_router(
                    config.policy.clone(),
                    config.holdings_registry.clone(),
                    Arc::new(FinanceJournalReadScopeHeaders),
                    bank_workspace_targets,
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
        }
        if !brand_targets.is_empty() {
            router = router.merge(
                crate::brand_identity_http::brand_identity_router(
                    config.policy.clone(),
                    config.holdings_registry.clone(),
                    Arc::new(FinanceJournalReadScopeHeaders),
                    brand_targets,
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
        }
        if !organization_targets.is_empty() {
            router = router.merge(
                crate::organization_nodes_http::organization_nodes_router(
                    config.policy.clone(),
                    config.holdings_registry.clone(),
                    Arc::new(FinanceJournalReadScopeHeaders),
                    organization_targets,
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
        }
        if !organization_setup_targets.is_empty() {
            router = router.merge(
                erpx_server_organization::public::organization_setup_http::organization_setup_router(
                    organization_setup_targets,
                )
                .map_err(|_| "PRIVATE_API_ORGANIZATION_CONFIGURATION_INVALID")?,
            );
        }
        if !approval_targets.is_empty() {
            router = router.merge(
                erpx_server_workflow::public::approval_oversight_http::approval_oversight_http_router(
                    approval_targets,
                )
                .map_err(|_| "PRIVATE_API_WORKFLOW_CONFIGURATION_INVALID")?,
            );
        }
        if !handbook_targets.is_empty() {
            router = router.merge(
                erpx_server_collaboration::public::handbook_http::handbook_router(
                    handbook_targets,
                )
                .map_err(|_| "PRIVATE_API_COLLABORATION_CONFIGURATION_INVALID")?,
            );
        }
        if !learning_targets.is_empty() {
            router = router.merge(
                erpx_server_learning::public::learning_http::learning_http_router(learning_targets)
                    .map_err(|_| "PRIVATE_API_LEARNING_CONFIGURATION_INVALID")?,
            );
        }
        if !quality_targets.is_empty() {
            router = router.merge(
                erpx_server_quality::public::quality_http::quality_router(quality_targets)
                    .map_err(|_| "PRIVATE_API_QUALITY_CONFIGURATION_INVALID")?,
            );
        }
        if !maintenance_targets.is_empty() {
            router = router.merge(
                erpx_server_maintenance::public::http::maintenance_router(maintenance_targets)
                    .map_err(|_| "PRIVATE_API_MAINTENANCE_CONFIGURATION_INVALID")?,
            );
        }
        if !operations_targets.is_empty() {
            router = router.merge(
                erpx_server_operations::public::operations_http::operations_router(operations_targets)
                    .map_err(|_| "PRIVATE_API_OPERATIONS_CONFIGURATION_INVALID")?,
            );
        }
        if !asset_targets.is_empty() {
            router = router.merge(
                erpx_server_assets::public::asset_http::asset_router(asset_targets)
                    .map_err(|_| "PRIVATE_API_ASSETS_CONFIGURATION_INVALID")?,
            );
        }
    }
    if !config.field_service_reads.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue()
                .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?,
        );
        let mut reads = Vec::new();
        let mut work_roles = Vec::new();
        let mut reserves = Vec::new();
        for (i, holding) in config.holdings.iter().enumerate() {
            let Some(companies) = config
                .field_service_reads
                .get(&holding.registration.holding_id)
            else {
                continue;
            };
            for company in companies {
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
            if service_read_targets.insert(target.clone()) {
            work_roles.push(crate::native_work_roles_http::NativeWorkRoleTarget {
                control:pools[0].clone(),
                host: target.0.clone(), holding_slug: target.1.clone(),
                inspector: Arc::new(erpx_server_identity_access::public::work_context::HoldingWorkContextInspector::new(
                    pools[0].clone(), pools[i+1].clone(), holding.registration.holding_id.clone(),
                    (*config.policy).clone(), clock.clone(), config.holdings_registry.clone(),
                ).map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?),
            });
            }
            let read_configuration = FieldServiceReservationReadConfiguration::new(
                FieldServicePartsEnvironment::Development,
                holding.registration.holding_id.clone(),
                company.clone(),
                catalogue.clone(),
            )
            .map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?;
            let read_configuration = if let Some(files) = config.field_service_initial_counts.get(&holding.registration.holding_id) {
                service_initial_count_targets.insert(target.clone());
                read_configuration.with_initial_count(files.clone())
            } else {
                read_configuration
            };
            let read_configuration = if let Some(companies)=config.field_files_companies.get(&holding.registration.holding_id) {
                read_configuration.with_field_attachment_companies(companies.clone())
            }else{read_configuration};
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
        if !config.inventory_availability_reads.is_empty() {
            let catalogue = Arc::new(
                current_package_catalogue()
                    .map_err(|_| "PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID")?,
            );
            let mut targets = Vec::new();
            let mut stock_targets = Vec::new();
            for (holding_id, companies) in &config.inventory_availability_reads {
                let index = config
                    .holdings
                    .iter()
                    .position(|h| &h.registration.holding_id == holding_id)
                    .ok_or("PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID")?;
                let holding = &config.holdings[index];
                let authorizer = Arc::new(
                    HoldingWorkAuthorizer::new(
                        pools[0].clone(),
                        pools[index + 1].clone(),
                        holding_id.clone(),
                        (*config.policy).clone(),
                        clock.clone(),
                        config.holdings_registry.clone(),
                    )
                    .map_err(|_| "PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID")?,
                );
                for company in companies {
                    targets.push(InventoryAvailabilityHttpTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        service: Arc::new(InventoryAvailabilityReadService {
                            authorizer: authorizer.clone(),
                            holding: holding_id.clone(),
                            company: company.clone(),
                            catalogue: catalogue.clone(),
                            clock: Arc::new(SharedClock(clock.clone())),
                        }),
                    });
                    stock_targets.push(crate::inventory_stock_http::InventoryStockHttpTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        service: Arc::new(crate::inventory_stock_read::InventoryStockRegisterService {
                            authorizer: authorizer.clone(),
                            holding: holding_id.clone(),
                            company: company.clone(),
                            catalogue: catalogue.clone(),
                            clock: Arc::new(SharedClock(clock.clone())),
                        }),
                    });
                }
            }
            router = router.merge(
                inventory_availability_http_router(
                    config.policy.clone(),
                    Arc::new(FieldServiceReadScopeHeaders),
                    targets,
                )
                .map_err(|_| "PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID")?,
            );
            if !stock_targets.is_empty() {
                router = router.merge(
                    crate::inventory_stock_http::inventory_stock_http_router(
                        config.policy.clone(),
                        Arc::new(FieldServiceReadScopeHeaders),
                        stock_targets,
                    )
                    .map_err(|_| "PRIVATE_API_INVENTORY_AVAILABILITY_CONFIGURATION_INVALID")?,
                );
            }
        }
        router = router.merge(crate::native_work_roles_http::native_work_roles_router(
            config.policy.clone(), config.holdings_registry.clone(), Arc::new(FieldServiceReadScopeHeaders),
            Arc::new(SharedClock(clock.clone())), work_roles,
        ).map_err(|_| "PRIVATE_API_FIELD_SERVICE_CONFIGURATION_INVALID")?);
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
    if !config.support_ticket_writes.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue()
                .map_err(|_| "PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID")?,
        );
        let mut targets = Vec::new();
        let mut read_targets = Vec::new();
        for (holding_id, company) in &config.support_ticket_writes {
            let index = config
                .holdings
                .iter()
                .position(|h| &h.registration.holding_id == holding_id)
                .ok_or("PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID")?;
            let holding = &config.holdings[index];
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[index + 1].clone(),
                    holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID")?,
            );
            read_targets.push(SupportReadHttpTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                service: Arc::new(SupportTicketReadService {
                    authorizer: authorizer.clone(),
                    holding: holding_id.clone(),
                    company: company.clone(),
                    catalogue: catalogue.clone(),
                    clock: Arc::new(SharedClock(clock.clone())),
                }),
            });
            targets.push(SupportTicketHttpTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                command: Arc::new(SupportTicketCommand {
                    authorizer,
                    holding: holding_id.clone(),
                    company: company.clone(),
                    catalogue: catalogue.clone(),
                    clock: Arc::new(SharedClock(clock.clone())),
                }),
            });
        }
        router = router.merge(
            support_ticket_http_router(
                config.policy.clone(),
                Arc::new(FieldServiceReadScopeHeaders),
                targets,
            )
            .map_err(|_| "PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID")?,
        );
        router = router.merge(
            support_read_http_router(
                config.policy.clone(),
                config.holdings_registry.clone(),
                Arc::new(FieldServiceReadScopeHeaders),
                read_targets,
            )
            .map_err(|_| "PRIVATE_API_SUPPORT_TICKET_CONFIGURATION_INVALID")?,
        );
    }
    if !config.support_evidence_writes.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue()
                .map_err(|_| "PRIVATE_API_SUPPORT_EVIDENCE_CONFIGURATION_INVALID")?,
        );
        let mut targets = Vec::new();
        for (holding_id, company) in &config.support_evidence_writes {
            let index = config
                .holdings
                .iter()
                .position(|h| &h.registration.holding_id == holding_id)
                .ok_or("PRIVATE_API_SUPPORT_EVIDENCE_CONFIGURATION_INVALID")?;
            let holding = &config.holdings[index];
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[index + 1].clone(),
                    holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_SUPPORT_EVIDENCE_CONFIGURATION_INVALID")?,
            );
            targets.push(SupportEvidenceHttpTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                command: Arc::new(SupportEvidenceCommand {
                    authorizer,
                    holding: holding_id.clone(),
                    company: company.clone(),
                    catalogue: catalogue.clone(),
                    clock: Arc::new(SharedClock(clock.clone())),
                }),
            });
        }
        router = router.merge(
            support_evidence_http_router(
                config.policy.clone(),
                Arc::new(FieldServiceReadScopeHeaders),
                targets,
            )
            .map_err(|_| "PRIVATE_API_SUPPORT_EVIDENCE_CONFIGURATION_INVALID")?,
        );
    }
    if !config.support_sla_writes.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue()
                .map_err(|_| "PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID")?,
        );
        let mut targets = Vec::new();
        for (holding_id, company) in &config.support_sla_writes {
            let index = config
                .holdings
                .iter()
                .position(|h| &h.registration.holding_id == holding_id)
                .ok_or("PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID")?;
            let holding = &config.holdings[index];
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[index + 1].clone(),
                    holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID")?,
            );
            targets.push(SupportSlaHttpTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                command: Arc::new(SupportSlaCommand {
                    authorizer,
                    holding: holding_id.clone(),
                    company: company.clone(),
                    catalogue: catalogue.clone(),
                    clock: Arc::new(SharedClock(clock.clone())),
                }),
            });
        }
        router = router.merge(
            support_sla_http_router(
                config.policy.clone(),
                Arc::new(FieldServiceReadScopeHeaders),
                targets,
            )
            .map_err(|_| "PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID")?,
        );
    }
    if !config.support_session_writes.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue()
                .map_err(|_| "PRIVATE_API_SUPPORT_SESSION_CONFIGURATION_INVALID")?,
        );
        let mut targets = Vec::new();
        for (holding_id, company) in &config.support_session_writes {
            let index = config
                .holdings
                .iter()
                .position(|h| &h.registration.holding_id == holding_id)
                .ok_or("PRIVATE_API_SUPPORT_SESSION_CONFIGURATION_INVALID")?;
            let holding = &config.holdings[index];
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[index + 1].clone(),
                    holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_SUPPORT_SESSION_CONFIGURATION_INVALID")?,
            );
            targets.push(SupportSessionHttpTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                command: Arc::new(SupportSessionCommand {
                    authorizer,
                    holding: holding_id.clone(),
                    company: company.clone(),
                    catalogue: catalogue.clone(),
                    clock: Arc::new(SharedClock(clock.clone())),
                }),
            });
        }
        router = router.merge(
            support_session_http_router(
                config.policy.clone(),
                Arc::new(FieldServiceReadScopeHeaders),
                targets,
            )
            .map_err(|_| "PRIVATE_API_SUPPORT_SESSION_CONFIGURATION_INVALID")?,
        );
    }
    if !config.support_incident_writes.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue()
                .map_err(|_| "PRIVATE_API_SUPPORT_INCIDENT_CONFIGURATION_INVALID")?,
        );
        let mut targets = Vec::new();
        for (holding_id, company) in &config.support_incident_writes {
            let index = config
                .holdings
                .iter()
                .position(|h| &h.registration.holding_id == holding_id)
                .ok_or("PRIVATE_API_SUPPORT_INCIDENT_CONFIGURATION_INVALID")?;
            let holding = &config.holdings[index];
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[index + 1].clone(),
                    holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_SUPPORT_INCIDENT_CONFIGURATION_INVALID")?,
            );
            targets.push(SupportIncidentHttpTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                command: Arc::new(SupportIncidentCommand {
                    authorizer,
                    holding: holding_id.clone(),
                    company: company.clone(),
                    catalogue: catalogue.clone(),
                    clock: Arc::new(SharedClock(clock.clone())),
                }),
            });
        }
        router = router.merge(
            support_incident_http_router(
                config.policy.clone(),
                Arc::new(FieldServiceReadScopeHeaders),
                targets,
            )
            .map_err(|_| "PRIVATE_API_SUPPORT_INCIDENT_CONFIGURATION_INVALID")?,
        );
    }
    #[cfg(feature="files-scan-worker")]
    let (files_router,files_workers,field_files_targets)=field_files::assemble(config,pools,clock.clone()).await?;
    #[cfg(feature="files-scan-worker")]
    { router=router.merge(files_router); }
    #[cfg(not(feature="files-scan-worker"))]
    let field_files_targets=BTreeSet::new();
    let service_package_provider_host=if let Some(packages)=&config.service_package_lifecycle {
        let provider=config.holdings.iter().find(|h|h.registration.holding_id==packages.provider_holding).ok_or("PRIVATE_API_SERVICE_PACKAGE_CONFIGURATION_INVALID")?;
        let targets=config.holdings.iter().enumerate().filter(|(_,h)|packages.holdings.contains(&h.registration.holding_id)).map(|(index,h)|crate::field_service_package_http::ServicePackageHttpTarget{holding_id:h.registration.holding_id.clone(),audience:format!("holding:{}:{}:{}",config.environment,h.registration.holding_id.as_str(),h.host),write:pools[index+1].clone()}).collect();
        router=router.merge(crate::field_service_package_http::service_package_router(pools[0].clone(),config.policy.clone(),config.holdings_registry.clone(),Arc::new(SharedClock(clock.clone())),provider.host.clone(),packages.provider_holding.clone(),targets,packages.signer.clone()).map_err(|_|"PRIVATE_API_SERVICE_PACKAGE_CONFIGURATION_INVALID")?);
        Some(provider.host.clone())
    }else{None};
    let state = Arc::new(Boundary {
        field_files_targets,
        service_package_provider_host,
        account_mfa:config.account_security_keys.is_some(),
        policy: config.policy.clone(),
        loopback: config.listen.to_string(),
        permits: Arc::new(Semaphore::new(config.maximum_requests)),
        timeout: config.request_timeout,
        finance_targets,
        finance_service_invoice_targets,
        finance_service_invoice_post_targets,
        finance_service_receipt_targets,
        finance_post_targets,
        finance_create_targets,
        finance_edit_targets,
        finance_reversal_targets,
        brand_targets: config
            .finance_journal_reads
            .keys()
            .filter_map(|holding_id| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        generic_payment_targets: config
            .finance_journal_reads
            .keys()
            .filter_map(|holding_id| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        bank_targets: config
            .finance_journal_reads
            .keys()
            .filter_map(|holding_id| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        bank_workspace_targets: config
            .finance_journal_reads
            .keys()
            .filter_map(|holding_id| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        service_read_targets,
        service_workspace_targets,
        service_parts_posting_targets,
        service_parts_prerequisite_targets,
        service_stock_setup_targets,
        service_initial_count_targets,
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
        support_ticket_targets: config
            .support_ticket_writes
            .iter()
            .filter_map(|(holding_id, _)| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        support_incident_targets: config
            .support_incident_writes
            .iter()
            .filter_map(|(holding_id, _)| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        support_session_targets: config
            .support_session_writes
            .iter()
            .filter_map(|(holding_id, _)| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        support_sla_targets: config
            .support_sla_writes
            .iter()
            .filter_map(|(holding_id, _)| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        support_evidence_targets: config
            .support_evidence_writes
            .iter()
            .filter_map(|(holding_id, _)| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        inventory_availability_targets: config
            .inventory_availability_reads
            .keys()
            .filter_map(|holding_id| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        inventory_stock_targets: config
            .inventory_availability_reads
            .keys()
            .filter_map(|holding_id| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        organization_targets: config
            .finance_journal_reads
            .keys()
            .filter_map(|holding_id| {
                config.holdings.iter().find(|h| &h.registration.holding_id == holding_id).map(|h| (h.host.clone(), h.registration.slug.clone()))
            })
            .collect(),
        account_bootstrap: config.account_bootstrap,
    });
    Ok(CheckedAssembly { router:router.layer(middleware::from_fn_with_state(state,boundary)), #[cfg(feature="files-scan-worker")] files_workers })
}
#[derive(Clone)]
struct SharedClock(Arc<dyn Clock + Send + Sync>);
impl Clock for SharedClock {
    fn now(&self) -> TimestampMillis {
        self.0.now()
    }
}
struct Boundary {
    service_package_provider_host:Option<String>,
    account_mfa:bool,
    policy: Arc<AccountPolicy>,
    loopback: String,
    permits: Arc<Semaphore>,
    timeout: Duration,
    finance_targets: BTreeSet<(String, String)>,
    finance_service_invoice_targets: BTreeSet<(String, String)>,
    finance_service_invoice_post_targets: BTreeSet<(String, String)>,
    finance_service_receipt_targets: BTreeSet<(String, String)>,
    finance_post_targets: BTreeSet<(String, String)>,
    finance_create_targets: BTreeSet<(String, String)>,
    finance_edit_targets: BTreeSet<(String, String)>,
    finance_reversal_targets: BTreeSet<(String, String)>,
    brand_targets: BTreeSet<(String, String)>,
    bank_targets: BTreeSet<(String, String)>,
    generic_payment_targets: BTreeSet<(String, String)>,
    bank_workspace_targets: BTreeSet<(String, String)>,
    service_read_targets: BTreeSet<(String, String)>,
    service_workspace_targets: BTreeSet<(String, String)>,
    service_parts_posting_targets: BTreeSet<(String, String)>,
    service_parts_prerequisite_targets: BTreeSet<(String, String)>,
    service_stock_setup_targets: BTreeSet<(String, String)>,
    service_initial_count_targets: BTreeSet<(String, String)>,
    field_files_targets: BTreeSet<(String,String)>,
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
    support_ticket_targets: BTreeSet<(String, String)>,
    support_incident_targets: BTreeSet<(String, String)>,
    support_session_targets: BTreeSet<(String, String)>,
    support_sla_targets: BTreeSet<(String, String)>,
    support_evidence_targets: BTreeSet<(String, String)>,
    inventory_availability_targets: BTreeSet<(String, String)>,
    inventory_stock_targets: BTreeSet<(String, String)>,
    organization_targets: BTreeSet<(String, String)>,
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
    let account_mfa=state.account_mfa&&request.method()==axum::http::Method::POST&&matches!(request.uri().path(),"/api/v1/account/iam/mfa/enroll"|"/api/v1/account/iam/mfa/verify");
    let service_packages=state.service_package_provider_host.as_deref()==Some(host)&&matches!((request.method(),request.uri().path()),
        (&axum::http::Method::GET,"/api/v1/platform/commercial"|"/api/v1/platform/commercial/packages")|
        (&axum::http::Method::POST,"/api/v1/platform/commercial/packages/preview"|"/api/v1/platform/commercial/packages/apply"));
    let finance_workspace = match request
        .uri()
        .path()
        .split('/')
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["","api","v1","holdings",slug,"finance","journals"] if request.method()==axum::http::Method::GET => {
            state.finance_targets.contains(&(host.to_owned(),(*slug).to_owned()))
        }
        ["","api","v1","holdings",slug,"finance","sales","invoices"]
            if matches!(request.method(), &axum::http::Method::GET | &axum::http::Method::POST) =>
        {
            state.finance_targets.contains(&(host.to_owned(),(*slug).to_owned()))
        }
        ["","api","v1","holdings",slug,"finance","field-work"] if request.method()==axum::http::Method::GET => {
            let key=(host.to_owned(),(*slug).to_owned());
            state.finance_targets.contains(&key)&&state.finance_service_invoice_targets.contains(&key)
        }
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
    let finance_field_journal_flow=matches!(request.method(),&axum::http::Method::GET|&axum::http::Method::POST)
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice(){
            ["","api","v1","holdings",slug,"finance","journals",id,"flow"] if !id.is_empty()=>state.finance_service_invoice_post_targets.contains(&(host.to_owned(),(*slug).to_owned())),
            _=>false,
        };
    let finance_invoice_post = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "invoices", id, "post"|"flow"|"confirm-no-charge"] if !id.is_empty() =>
                state.finance_service_invoice_post_targets.contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_receipt_review = match (request.method(),request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","bank","statements","import"])
        | (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","payments"]) => state.finance_service_receipt_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        (&axum::http::Method::GET,["","api","v1","holdings",slug,"finance","payments",_id,"workspace"|"runtime"]) => state.finance_service_receipt_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        (&axum::http::Method::GET,["","api","v1","holdings",slug,"finance","invoices",id,"payment-runtime"]) if !id.is_empty() => state.finance_service_receipt_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        (&axum::http::Method::GET,["","api","v1","holdings",slug,"finance","sales","receipts",id,"workspace"|"runtime"])
        | (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","payments",id,"approve"|"release"|"confirm"]) if !id.is_empty() => state.finance_service_receipt_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        _ => false,
    };
    let finance_invoice_runtime = request.method() == axum::http::Method::GET
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "invoices", id, "post-runtime"] if !id.is_empty() =>
                state.finance_service_invoice_post_targets.contains(&(host.to_owned(), (*slug).to_owned())),
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
    let service_initial_count = match (
        request.method(),
        request.uri().path().split('/').collect::<Vec<_>>().as_slice(),
    ) {
        (&axum::http::Method::GET | &axum::http::Method::POST,
            ["", "api", "v1", "holdings", slug, "field-service", "initial-physical-stock"])
        | (&axum::http::Method::GET,
            ["", "api", "v1", "holdings", slug, "field-service", "initial-physical-stock", _])
        | (&axum::http::Method::GET,
            ["", "api", "v1", "holdings", slug, "field-service", "initial-physical-stock", _, "stock-count"])
        | (&axum::http::Method::GET | &axum::http::Method::POST,
            ["", "api", "v1", "holdings", slug, "field-service", "initial-physical-stock", _, "flow"])
        | (&axum::http::Method::POST,
            ["", "api", "v1", "holdings", slug, "field-service", "initial-physical-stock", _, "update" | "submit" | "approve" | "post" | "reject" | "cancel"])
        | (&axum::http::Method::POST,
            ["", "api", "v1", "holdings", slug, "field-service", "initial-physical-stock", _, "stock-count", "flow" | "submit" | "approve" | "post" | "reject" | "cancel"])
            => state.service_initial_count_targets.contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let service_initial_count = service_initial_count
        && request.uri().path().split('/').nth(7) != Some("masters");
    let service_stock_setup = match (
        request.method(),
        request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice(),
    ) {
        (&axum::http::Method::GET | &axum::http::Method::POST,
         ["", "api", "v1", "holdings", slug, "field-service", "customer-preparation"])
        | (&axum::http::Method::POST,
         ["", "api", "v1", "holdings", slug, "field-service", "customer-preparation", "confirmations"])
        | (
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
    let field_files = match (request.method(),request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET|&axum::http::Method::POST,["","api","v1","holdings",slug,"field-service","attachment-sets"])
        | (&axum::http::Method::POST,["","api","v1","holdings",slug,"field-service","attachment-sets",_,"files"])
        | (&axum::http::Method::PUT,["","api","v1","holdings",slug,"field-service","uploads",_,"content"])
        | (&axum::http::Method::GET,["","api","v1","holdings",slug,"field-service","attachment-links",_,"status"|"content"]) => state.field_files_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        (&axum::http::Method::GET|&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","payment-files","attachment-sets"])
        | (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","payment-files","attachment-sets",_,"files"])
        | (&axum::http::Method::PUT,["","api","v1","holdings",slug,"finance","payment-files","uploads",_,"content"])
        | (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","payment-files","attachment-links",_,"withdraw"])
        | (&axum::http::Method::GET,["","api","v1","holdings",slug,"finance","payment-files","attachment-links",_,"status"|"content"]) => state.field_files_targets.contains(&(host.to_owned(),(*slug).to_owned())) && state.finance_service_receipt_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        (&axum::http::Method::GET|&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","journal-files","attachment-sets"])
        | (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","journal-files","attachment-sets",_,"files"])
        | (&axum::http::Method::PUT,["","api","v1","holdings",slug,"finance","journal-files","uploads",_,"content"])
        | (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","journal-files","attachment-links",_,"withdraw"])
        | (&axum::http::Method::GET,["","api","v1","holdings",slug,"finance","journal-files","attachment-links",_,"status"|"content"])=>state.field_files_targets.contains(&(host.to_owned(),(*slug).to_owned()))&&state.finance_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        (&axum::http::Method::POST,["","api","v1","holdings",slug,"finance","journal-files","attachment-sets",_,"bind"])=>state.field_files_targets.contains(&(host.to_owned(),(*slug).to_owned()))&&state.finance_targets.contains(&(host.to_owned(),(*slug).to_owned()))&&state.field_attachment_bind_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        _=>false,
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
    let field_operations = matches!(request.method(), &axum::http::Method::GET | &axum::http::Method::POST) && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
        ["", "api", "v1", "holdings", slug, "field-service", "operations"] => state.service_source_record_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        ["", "api", "v1", "holdings", slug, "field-service", "operations", id] => !id.is_empty() && state.service_source_record_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        _ => false,
    };
    let field_support = request.method() == axum::http::Method::GET && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
        ["", "api", "v1", "holdings", slug, "field-service", "support", id] => !id.is_empty() && state.service_source_record_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        _ => false,
    };
    let field_work_roles = matches!(request.method(), &axum::http::Method::GET | &axum::http::Method::POST)
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "field-service", "work-roles"] | ["", "api", "v1", "holdings", slug, "field-service", "work-roles", "current" | "initialize" | "companies" | "preferences" | "workspace-preference" | "access" | "context"] => state.service_read_targets.contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let field_evidence = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "field-service", "executive-evidence", "search" | "open"] => state.service_source_record_targets.contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let field_collaboration_write = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "field-service", "collaboration"] => state.service_source_record_targets.contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let field_collaboration = field_evidence || field_collaboration_write || request.method() == axum::http::Method::GET
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "field-service", "collaboration" | "inbox-candidates" | "executive"] => state.service_source_record_targets.contains(&(host.to_owned(),(*slug).to_owned())),
            _ => false,
        };
    let field_inbox_state=request.method()==axum::http::Method::POST && match request.uri().path().split('/').collect::<Vec<_>>().as_slice(){
        ["", "api", "v1", "holdings", slug, "field-service", "inbox", id, "state"] if !id.is_empty()=>state.service_source_record_targets.contains(&(host.to_owned(),(*slug).to_owned())),
        _=>false,
    };
    let bootstrap = state.account_bootstrap
        && match (request.method(), request.uri().path()) {
            (&axum::http::Method::POST, "/api/auth/sign-in/email" | "/api/auth/sign-out")
            | (
                &axum::http::Method::GET | &axum::http::Method::DELETE,
                "/api/v1/account/device-accounts",
            )
            | (
                &axum::http::Method::POST,
                "/api/v1/account/device-accounts/initialize"
                | "/api/v1/account/device-accounts/active"
                | "/api/v1/account/device-accounts/sign-in",
            )
            | (
                &axum::http::Method::OPTIONS,
                "/api/v1/account/device-accounts"
                | "/api/v1/account/device-accounts/initialize"
                | "/api/v1/account/device-accounts/active"
                | "/api/v1/account/device-accounts/sign-in",
            ) => host == state.policy.account_host,
            (&axum::http::Method::GET, "/sso/authorize") => host == state.policy.account_host,
            (&axum::http::Method::GET, "/sso/start" | "/sso/callback") => state
                .finance_targets
                .iter()
                .chain(state.service_read_targets.iter())
                .any(|(target_host, _)| target_host == host),
            _ => false,
        };
    let support_ticket = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "support"] => state
                .support_ticket_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let support_incident = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "support", "incidents"] => state
                .support_incident_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let support_session = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "support", "sessions"] => state
                .support_session_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let support_sla = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "support", "sla"] => state
                .support_sla_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let support_evidence = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "support", "evidence"] => state
                .support_evidence_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_opening_balances = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "opening-balances", "preview"]
            | ["", "api", "v1", "holdings", slug, "finance", "opening-balances", "import"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_report = request.method() == axum::http::Method::GET
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "reports"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_setup = match *request.method() {
        axum::http::Method::GET => match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "charts", _, "accounts"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        },
        axum::http::Method::POST => match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "charts"]
            | ["", "api", "v1", "holdings", slug, "finance", "charts", "import"]
            | ["", "api", "v1", "holdings", slug, "finance", "charts", _, "accounts"]
            | ["", "api", "v1", "holdings", slug, "finance", "charts", _, "accounts", _, "status"]
            | ["", "api", "v1", "holdings", slug, "finance", "periods"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        },
        _ => false,
    };
    let finance_invoice_read = request.method() == axum::http::Method::GET
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "invoices"]
            | ["", "api", "v1", "holdings", slug, "finance", "invoices", "aging"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_sales_invoice_edit = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "invoices", id]
            | ["", "api", "v1", "holdings", slug, "finance", "invoices", id, "void"]
                if !id.is_empty() =>
            {
                state
                    .finance_targets
                    .contains(&(host.to_owned(), (*slug).to_owned()))
            }
            _ => false,
        };
    let finance_sales_receipts = match *request.method() {
        axum::http::Method::GET => match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "sales", "receipts", _]
            | ["", "api", "v1", "holdings", slug, "finance", "sales", "receipts", _, "export"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        },
        axum::http::Method::POST => match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "sales", "receipts", _, "issue"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        },
        _ => false,
    };
    let finance_sales_defaults = match request.method() {
        &axum::http::Method::GET | &axum::http::Method::POST => {
            match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
                ["", "api", "v1", "holdings", slug, "finance", "sales", "defaults"] => state
                    .finance_targets
                    .contains(&(host.to_owned(), (*slug).to_owned())),
                ["", "api", "v1", "holdings", slug, "finance", "sales", "defaults", _, "decide"] => {
                    request.method() == axum::http::Method::POST
                        && state
                            .finance_targets
                            .contains(&(host.to_owned(), (*slug).to_owned()))
                }
                _ => false,
            }
        }
        _ => false,
    };
    let finance_sales_invoice_create = request.method() == axum::http::Method::POST
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "finance", "sales", "invoices"] => state
                .finance_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let brand = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET | &axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "brand"]) => state
            .brand_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let generic_payment = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "finance", "payments", "generic"]) => state
            .generic_payment_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let finance_supplier_bill = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "finance", "payables", id]) if !id.is_empty() =>
            state.finance_targets.contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "finance", "payables"]) =>
            state.finance_targets.contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "finance", "payables", id]) if !id.is_empty() =>
            state.finance_targets.contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "finance", "payables", id, "post" | "void"]) if !id.is_empty() =>
            state.finance_targets.contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let finance_period_close = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "finance", "periods", id, "workspace"]) if !id.is_empty() =>
            state.finance_targets.contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "finance", "periods", id, "begin-close" | "cancel-close" | "close" | "reopen"]) if !id.is_empty() =>
            state.finance_targets.contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let bank = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "finance", "bank", "statements", "generic-import"]) => state
            .bank_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "finance", "bank", "transactions", id, "reconcile"]) if !id.is_empty() => state
            .bank_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "finance", "bank"]) => state
            .bank_workspace_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let inventory_availability = request.method() == axum::http::Method::GET
        && match request.uri().path().split('/').collect::<Vec<_>>().as_slice() {
            ["", "api", "v1", "holdings", slug, "inventory", "availability" | "movements"] => state
                .inventory_availability_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let inventory_stock = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "inventory", "stock-register"]) => state
            .inventory_stock_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "inventory", "receipts", id]) if !id.is_empty() => state
            .inventory_stock_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let learning = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "learning", "workspace"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "learning", "eligibility", person]) if !person.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::GET | &axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "learning", "assignments", id]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "learning", "courses"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "learning", "courses", id]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "learning", "assignments"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "learning", "certificates", id, "revoke"]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let handbook = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET | &axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "handbook"])
        | (&axum::http::Method::GET | &axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "handbook", "editors"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::GET | &axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "handbook", id]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "handbook", id, "actions"]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let workflow_approvals = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "workflow", "approvals"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "workflow", "approvals", kind, record_id, "evidence"])
            if !kind.is_empty() && !record_id.is_empty() => state
                .organization_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let organization_setup = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "people", "organization", "levels"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "people", "organization", "templates"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "people", "organization", "templates", id]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "people", "organization", "templates", id, "apply"]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let organization_nodes = match (request.method(), request.uri().path().split('/').collect::<Vec<_>>().as_slice()) {
        (&axum::http::Method::GET, ["", "api", "v1", "holdings", slug, "people", "organization"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "people", "organization", "changes"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "people", "organization", "changes", "preview"]) => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        (&axum::http::Method::POST, ["", "api", "v1", "holdings", slug, "people", "organization", "changes", id, "review" | "cancel"]) if !id.is_empty() => state
            .organization_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    if !(matches!(
        request.uri().path(),
        "/api/auth/get-session" | "/api/health" | "/api/ready"
    ) || account_mfa || service_packages || field_work_roles || field_operations || field_support || field_collaboration || field_inbox_state || support_ticket || support_incident || support_session || support_sla || support_evidence || inventory_availability || inventory_stock || organization_nodes || organization_setup || workflow_approvals || handbook || learning || brand || bank || generic_payment || finance_supplier_bill || finance_period_close || finance_workspace || finance_invoice_read || finance_opening_balances || finance_report || finance_setup || finance_sales_invoice_create || finance_sales_invoice_edit || finance_sales_defaults || finance_sales_receipts
        || finance_invoice_runtime
        || finance_field_journal_flow
        || finance_invoice_post
        || finance_receipt_review
        || finance_post
        || finance_create
        || finance_edit
        || finance_reverse
        || service_read
        || service_parts_posting
        || service_parts_prerequisite
        || service_stock_setup
        || service_initial_count
        || service_reserve
        || field_files
        || field_attachment
        || field_attachment_bind
        || bootstrap)
        || !(bootstrap
            || account_mfa
            || service_packages
            || field_work_roles
            || field_inbox_state
            || field_evidence
            || field_collaboration_write
            || field_operations
            || support_ticket
            || support_incident
            || support_session
            || support_sla
            || support_evidence
            || organization_setup
            || workflow_approvals
            || handbook
            || learning
            || brand
            || bank
            || generic_payment
            || finance_supplier_bill
            || finance_period_close
            || finance_field_journal_flow
        || finance_invoice_post
            || finance_receipt_review
            || finance_post
            || finance_create
            || finance_edit
            || finance_reverse
            || finance_opening_balances
            || finance_setup
            || finance_sales_invoice_create
            || finance_sales_invoice_edit
            || finance_sales_defaults
            || finance_sales_receipts
            || service_reserve
            || service_parts_posting
            || service_parts_prerequisite
            || service_stock_setup
            || service_initial_count
            || field_files
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
/// One native Support SLA drain over the configured Holding databases. The
/// caller supplies a reviewed development configuration; no HTTP route or TS
/// worker is involved.
pub async fn run_support_sla_drain_file(path: &Path) -> Result<String, &'static str> {
    run_support_sla_drain_file_with_profile(path, Profile::Development).await
}

async fn run_support_sla_drain_file_with_profile(
    path: &Path,
    profile: Profile,
) -> Result<String, &'static str> {
    let configuration = DevelopmentApiConfiguration::read_configuration_file(path, profile)?;
    let limits = PoolLimits {
        max_connections: 3,
        acquire_timeout: Duration::from_secs(5),
    };
    let control = configuration
        .control
        .connect(limits)
        .await
        .map_err(|_| "PRIVATE_API_DATABASE_UNAVAILABLE")?;
    let mut pools = vec![control];
    for holding in &configuration.holdings {
        pools.push(
            holding
                .database
                .connect(limits)
                .await
                .map_err(|_| "PRIVATE_API_DATABASE_UNAVAILABLE")?,
        );
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "SUPPORT_SLA_CLOCK_UNAVAILABLE")?
        .as_millis() as i64;
    let mut total = 0_i64;
    for (holding_id, company) in &configuration.support_sla_writes {
        let index = configuration
            .holdings
            .iter()
            .position(|h| &h.registration.holding_id == holding_id)
            .ok_or("PRIVATE_API_SUPPORT_SLA_CONFIGURATION_INVALID")?;
        let mut tx = pools[index + 1]
            .raw()
            .begin()
            .await
            .map_err(|_| "PRIVATE_API_DATABASE_UNAVAILABLE")?;
        total += drain_support_sla(&mut tx, company, now)
            .await
            .map_err(|_| "SUPPORT_SLA_DRAIN_FAILED")?;
        tx.commit()
            .await
            .map_err(|_| "PRIVATE_API_DATABASE_UNAVAILABLE")?;
    }
    Ok(format!(r#"{{"drained":{total}}}"#))
}


/// Durable Support SLA worker. Reuses the owner schedule and the one-shot
/// drain; each tick reopens the reviewed configuration and rechecks the
/// purchase, and a failure backs off without stopping the supervisor.
pub async fn run_support_sla_worker_file(
    path: &Path,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), &'static str> {
    let now = SystemAccountClock.now().get();
    let mut schedule =
        SlaDrainSchedule::start(now, DEFAULT_INTERVAL_MS).map_err(|_| "SUPPORT_SLA_SCHEDULE_INVALID")?;
    let mut shutdown = Box::pin(shutdown);
    loop {
        let now = SystemAccountClock.now().get();
        let delay = schedule.next_delay_ms(now);
        if delay > 0 {
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                _ = tokio::time::sleep(Duration::from_millis(delay as u64)) => continue,
            }
        }
        let outcome = run_sla_drain_tick(&mut schedule, now, |_| async {
            run_support_sla_drain_file_with_profile(path, Profile::Production)
                .await
                .map(|_| 0_i64)
        })
        .await;
        if let Err(error) = outcome {
            eprintln!("production Support SLA worker tick failed: {error}; retrying");
        }
        let next = schedule.next_delay_ms(SystemAccountClock.now().get());
        if next > 0 {
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                _ = tokio::time::sleep(Duration::from_millis(next as u64)) => {}
            }
        }
    }
}

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

/// Assemble the private production business API from an explicit durable
/// configuration. The same owner routers, Account authority and migration
/// inventory checks used by development are mounted; only the database binding
/// and environment label differ.
pub async fn assemble_production_api(
    config: DevelopmentApiConfiguration,
    clock: Arc<dyn Clock + Send + Sync>,
    secrets: Arc<dyn AuthenticationSecrets>,
) -> Result<DevelopmentApiRuntime, &'static str> {
    if config.profile != Profile::Production {
        return Err("EXPLICIT_PRIVATE_PRODUCTION_API_REQUIRED");
    }
    assemble_development_api(config, clock, secrets).await
}

/// Operator entry for the private production runtime. It binds loopback only
/// and never discovers or modifies the installed service.
pub async fn run_production_api_file(
    path: &Path,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), &'static str> {
    let configuration = DevelopmentApiConfiguration::from_production_file(path)?;
    let runtime = assemble_production_api(
        configuration,
        Arc::new(SystemAccountClock),
        Arc::new(SystemAuthenticationSecrets),
    )
    .await?;
    let listener = match tokio::net::TcpListener::bind(runtime.listen_address()).await {
        Ok(listener) => listener,
        Err(_) => {
            runtime.close().await;
            return Err("PRIVATE_PRODUCTION_LISTEN_FAILED");
        }
    };
    eprintln!(
        "Private production API listening on {}; readiness remains unavailable.",
        runtime.listen_address()
    );
    let result = axum::serve(listener, runtime.router())
        .with_graceful_shutdown(shutdown)
        .await;
    runtime.close().await;
    result.map_err(|_| "PRIVATE_PRODUCTION_HTTP_FAILED")
}

/// The scoped worker uses the same reviewed Holding registry as the API. Each
/// projection database is explicit, separately leased and HoldingRead-migrated.
pub async fn run_service_count_worker_file(
    path: &Path,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), &'static str> {
    run_outbox_worker_file(path,shutdown,false,None,Profile::Development).await
}
pub async fn run_field_dependency_worker_file(path:&Path,shutdown:impl std::future::Future<Output=()>+Send+'static)->Result<(),&'static str>{
    run_outbox_worker_file(path,shutdown,true,None,Profile::Development).await.map_err(|code|match code{
        "COUNT_WORKER_DISTINCT_READ_DATABASE_REQUIRED"=>"FIELD_WORKER_DISTINCT_READ_DATABASE_REQUIRED",
        "COUNT_WORKER_SCHEMA_UNAVAILABLE"=>"FIELD_WORKER_SCHEMA_UNAVAILABLE",
        "COUNT_WORKER_DATABASE_UNAVAILABLE"=>"FIELD_WORKER_DATABASE_UNAVAILABLE",
        "COUNT_WORKER_SCHEMA_MISMATCH"=>"FIELD_WORKER_SCHEMA_MISMATCH",
        "COUNT_WORKER_BINDINGS_MISMATCH"=>"FIELD_WORKER_BINDINGS_MISMATCH",
        "COUNT_WORKER_QUERY_CANCEL_FAILED"=>"FIELD_WORKER_QUERY_CANCEL_FAILED",
        "COUNT_WORKER_CLOSE_TIMEOUT"=>"FIELD_WORKER_CLOSE_TIMEOUT",other=>other,
    })
}
struct FieldDependencyRetry {
    holding: HoldingId,
    event: String,
    dead_at: i64,
    reason: String,
}
/// Operator-only process entry, admitted by the same private configuration,
/// enforced Holding registry and migration checks as the native worker.
pub async fn retry_field_dependency_worker_file(path:&Path,holding:&str,event:&str,dead_at:i64,reason:&str)->Result<(),&'static str>{
    let holding=HoldingId::new(holding).map_err(|_|"FIELD_RETRY_HOLDING_INVALID")?;
    if event.is_empty()||event.len()>180||reason.trim().chars().count()<3||reason.len()>2000{return Err("FIELD_RETRY_INPUT_INVALID");}
    run_outbox_worker_file(path,std::future::ready(()),true,Some(FieldDependencyRetry{holding,event:event.into(),dead_at,reason:reason.into()}),Profile::Development).await
}
async fn run_outbox_worker_file(path:&Path,shutdown:impl std::future::Future<Output=()>+Send+'static,field:bool,retry:Option<FieldDependencyRetry>,profile:Profile)->Result<(),&'static str>{
    let config = DevelopmentApiConfiguration::read_configuration_file(path, profile)?;
    if field {
        if !config.field_service_source_record_reads.values().any(|m|matches!(m,crate::field_service_source_record::SourceRecordMode::Enforced)){return Err("FIELD_WORKER_ENFORCED_SOURCE_TARGET_REQUIRED");}
    } else if config.field_service_initial_counts.is_empty(){return Err("COUNT_WORKER_TARGET_REQUIRED");}
    if let Some(target)=&retry
        && (!field||!config.holdings.iter().any(|h|h.registration.holding_id==target.holding)
            ||!matches!(config.field_service_source_record_reads.get(&target.holding),Some(crate::field_service_source_record::SourceRecordMode::Enforced))){
        return Err("FIELD_RETRY_ENFORCED_HOLDING_REQUIRED");
    }
    // Startup and admission share the same explicit Holding partition. A
    // registered but shadow/disabled Holding must never be drained implicitly.
    let selected = |id: &HoldingId| if field {
        matches!(config.field_service_source_record_reads.get(id), Some(crate::field_service_source_record::SourceRecordMode::Enforced))
            && retry.as_ref().is_none_or(|target|target.holding==*id)
    } else { config.field_service_initial_counts.contains_key(id) };
    for holding in &config.holdings {
        if selected(&holding.registration.holding_id) && holding.read_database.is_none() {
            return Err("COUNT_WORKER_DISTINCT_READ_DATABASE_REQUIRED");
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let control_files = load_migrations(&root, MigrationTrack::Control).await.map_err(|_| "COUNT_WORKER_SCHEMA_UNAVAILABLE")?;
    let write_files = load_migrations(&root, MigrationTrack::HoldingWrite).await.map_err(|_| "COUNT_WORKER_SCHEMA_UNAVAILABLE")?;
    let read_files = load_migrations(&root, MigrationTrack::HoldingRead).await.map_err(|_| "COUNT_WORKER_SCHEMA_UNAVAILABLE")?;
    let limits = PoolLimits { max_connections: 3, acquire_timeout: Duration::from_secs(5) };
    let control = config.control.connect(limits).await.map_err(|_| "COUNT_WORKER_DATABASE_UNAVAILABLE")?;
    let mut opened = vec![control.clone()];
    let mut worker_names = Vec::<String>::new();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let result = async {
        verify_current_migration_inventory(&control, MigrationTrack::Control, &control_files).await.map_err(|_| "COUNT_WORKER_SCHEMA_MISMATCH")?;
        let registry = config.holdings_registry.clone();
        PgRequestLease::new(control.clone()).run(move |c| Box::pin(async move {
            lock_holding_binding_sources(c).await?;
            registry.verify_startup(c).await
        })).await.map_err(|_| "COUNT_WORKER_BINDINGS_MISMATCH")?;
        let mut pairs = Vec::new();
        #[cfg(feature="field-push-worker")]
        let mut push_targets=Vec::new();
        for holding in &config.holdings {
            if !selected(&holding.registration.holding_id) { continue; }
            let kind=if field{"field"}else{"count"};
            let write_name = format!("erpx-{kind}-{run_id}-w-{}", holding.registration.holding_id.as_str());
            let read_name = format!("erpx-{kind}-{run_id}-r-{}", holding.registration.holding_id.as_str());
            let write = holding.database.connect_named(limits, &write_name).await.map_err(|_| "COUNT_WORKER_DATABASE_UNAVAILABLE")?;
            opened.push(write.clone());
            worker_names.push(write_name);
            let read = holding.read_database.as_ref().ok_or("COUNT_WORKER_DISTINCT_READ_DATABASE_REQUIRED")?
                .connect_named(limits, &read_name).await.map_err(|_| "COUNT_WORKER_DATABASE_UNAVAILABLE")?;
            opened.push(read.clone());
            worker_names.push(read_name);
            verify_current_migration_inventory(&write, MigrationTrack::HoldingWrite, &write_files).await.map_err(|_| "COUNT_WORKER_SCHEMA_MISMATCH")?;
            verify_current_migration_inventory(&read, MigrationTrack::HoldingRead, &read_files).await.map_err(|_| "COUNT_WORKER_SCHEMA_MISMATCH")?;
            #[cfg(feature="field-push-worker")]
            if field {push_targets.push(crate::field_push_worker::FieldPushTarget{
                control:control.raw().clone(),holding:write.raw().clone(),
                audience:format!("holding:{}:{}:{}",config.environment,holding.registration.holding_id.as_str(),holding.host),
                origin:format!("https://{}",holding.host),
                environment:config.environment.clone(),
            });}
            pairs.push((write.raw().clone(), read.raw().clone()));
        }
        if let Some(target)=&retry {
            if pairs.len()!=1{return Err("FIELD_RETRY_EXACT_HOLDING_REQUIRED");}
            let mut c=pairs[0].0.acquire().await.map_err(|_|"FIELD_RETRY_DATABASE_UNAVAILABLE")?;
            let (owned,eligible)=crate::field_dependency_worker::predicates();
            use erpx_server_shared::public::Clock;
            let now=erpx_server_identity_access::public::SystemAccountClock.now().get();
            let audit=format!("native-field-retry-{}",uuid::Uuid::new_v4());
            let applied=erpx_server_workflow::public::service_count_outbox::retry_owned_dead_letter(
                &mut c,&owned,&eligible,erpx_server_workflow::public::service_count_outbox::DeadLetterRetry{
                    event_id:&target.event,dead_letter_at:target.dead_at,now,holding_id:target.holding.as_str(),reason:&target.reason,audit_id:&audit,
                },
            ).await.map_err(|_|"FIELD_RETRY_FAILED")?;
            if !applied{return Err("FIELD_RETRY_NOT_CURRENT_OR_ELIGIBLE");}
        }else if field {
            #[cfg(feature="field-push-worker")]
            {
                let provider=crate::field_push_worker::configured_provider()?;
                if provider.is_none(){eprintln!("FIELD_PUSH_DISABLED: configuration absent; pending jobs retained");}
                let push=async {
                    loop {
                        if let Some(provider)=provider.as_ref(){
                            for target in &push_targets {
                                if crate::field_push_worker::run_once(target,Some(provider),&erpx_server_identity_access::public::SystemAccountClock).await.is_err(){
                                    eprintln!("FIELD_PUSH_TURN_UNAVAILABLE");
                                }
                            }
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                };
                tokio::select!{_=shutdown=>{},_=crate::field_dependency_worker::run(pairs,std::future::pending())=>{},_=push=>{}}
            }
            #[cfg(not(feature="field-push-worker"))]
            crate::field_dependency_worker::run(pairs,shutdown).await;
        }else{crate::service_count_worker::run(pairs, shutdown).await;}
        Ok(())
    }.await;
    // A dropped SQLx future can leave a backend waiting on a table lock. Cancel
    // only queries carrying this unguessable worker-run identity, using Control
    // so an occupied Holding pool cannot prevent shutdown. Never terminate a
    // backend or cancel another process's query.
    if !worker_names.is_empty() {
        let cancelled = tokio::time::timeout(Duration::from_secs(2),
            sqlx::query("SELECT pg_cancel_backend(pid) FROM pg_stat_activity WHERE application_name=ANY($1) AND usename=current_user AND backend_type='client backend' AND state='active' AND pid<>pg_backend_pid()")
                .bind(&worker_names).fetch_all(control.raw())).await;
        if !matches!(cancelled, Ok(Ok(_))) { return Err("COUNT_WORKER_QUERY_CANCEL_FAILED"); }
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        for pool in opened { pool.close().await; }
    }).await.map_err(|_| "COUNT_WORKER_CLOSE_TIMEOUT")?;
    result
}

#[cfg(feature="files-scan-worker")]
mod field_files;

#[cfg(feature="files-scan-worker")]
pub async fn run_field_files_worker_file(path:&Path,shutdown:impl std::future::Future<Output=()>+Send)->Result<(), &'static str>{
    run_field_files_worker_file_with_profile(path,shutdown,Profile::Development).await
}
#[cfg(feature="files-scan-worker")]
async fn run_field_files_worker_file_with_profile(path:&Path,shutdown:impl std::future::Future<Output=()>+Send,profile:Profile)->Result<(), &'static str>{
    field_files::run_workers(DevelopmentApiConfiguration::read_configuration_file(path,profile)?,shutdown).await
}
#[cfg(feature="files-scan-worker")]
pub async fn run_production_files_worker_file(path:&Path,shutdown:impl std::future::Future<Output=()>+Send)->Result<(), &'static str>{
    run_field_files_worker_file_with_profile(path,shutdown,Profile::Production).await
}

/// Durable private production Field worker supervisor. Each worker claims its
/// own durable PostgreSQL work; a worker that stops is restarted after a
/// bounded delay until the operator shuts the process down. The supervisor
/// never discovers targets: the explicit production document owns the Holding
/// partition.
pub async fn run_production_workers_file(
    path: &Path,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), &'static str> {
    let config = DevelopmentApiConfiguration::from_production_file(path)?;
    if !config
        .field_service_source_record_reads
        .values()
        .any(|mode| matches!(mode, crate::field_service_source_record::SourceRecordMode::Enforced))
    {
        return Err("FIELD_WORKER_ENFORCED_SOURCE_TARGET_REQUIRED");
    }
    let (stop_sender, stop) = tokio::sync::watch::channel(false);
    let mut handles = Vec::new();
    {
        let path = path.to_owned();
        let mut stop = stop.clone();
        handles.push(tokio::spawn(async move {
            loop {
                if *stop.borrow() {
                    return Ok::<(), &'static str>(());
                }
                let mut worker_stop = stop.clone();
                let result = run_outbox_worker_file(
                    &path,
                    async move {
                        let _ = worker_stop.changed().await;
                    },
                    true,
                    None,
                    Profile::Production,
                )
                .await;
                if *stop.borrow() {
                    return Ok(());
                }
                if let Err(code) = result {
                    eprintln!("production Field dependency worker stopped: {code}; restarting");
                }
                tokio::select! {
                    _ = stop.changed() => return Ok(()),
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        }));
    }
    #[cfg(feature = "files-scan-worker")]
    if config.field_files_providers.is_some() {
        let path = path.to_owned();
        let mut stop = stop.clone();
        handles.push(tokio::spawn(async move {
            loop {
                if *stop.borrow() {
                    return Ok::<(), &'static str>(());
                }
                let mut worker_stop = stop.clone();
                let result = run_field_files_worker_file_with_profile(
                    &path,
                    async move {
                        let _ = worker_stop.changed().await;
                    },
                    Profile::Production,
                )
                .await;
                if *stop.borrow() {
                    return Ok(());
                }
                if let Err(code) = result {
                    eprintln!("production Field files worker stopped: {code}; restarting");
                }
                tokio::select! {
                    _ = stop.changed() => return Ok(()),
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        }));
    }
    if !config.field_service_initial_counts.is_empty() {
        let path = path.to_owned();
        let mut stop = stop.clone();
        handles.push(tokio::spawn(async move {
            loop {
                if *stop.borrow() {
                    return Ok::<(), &'static str>(());
                }
                let mut worker_stop = stop.clone();
                let result = run_outbox_worker_file(
                    &path,
                    async move {
                        let _ = worker_stop.changed().await;
                    },
                    false,
                    None,
                    Profile::Production,
                )
                .await;
                if *stop.borrow() {
                    return Ok(());
                }
                if let Err(code) = result {
                    eprintln!("production Field count worker stopped: {code}; restarting");
                }
                tokio::select! {
                    _ = stop.changed() => return Ok(()),
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        }));
    }
    shutdown.await;
    let _ = stop_sender.send(true);
    let mut first_error = None;
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(code)) => {
                if first_error.is_none() {
                    first_error = Some(code);
                }
            }
            Err(_) => {
                if first_error.is_none() {
                    first_error = Some("PRODUCTION_WORKER_PANICKED");
                }
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}
