//! Native R3 Finance-owned Assets financial workspace reads.
//!
//! Additive owner ingress for the two retained Next routes:
//!   GET /api/v1/holdings/{holding_slug}/assets/financial-assets
//!   GET /api/v1/holdings/{holding_slug}/assets/financial-assets/{...path}
//!
//! Every data field below is read from the Holding Postgres pool by SQLx in
//! this module. Company session, Finance role, purchased-capability revision
//! and the opaque command context token are supplied through the native
//! `AssetsFinancialAuthority` port; composition owns that binding, so this
//! module never reads them from a caller-controlled header beyond the exact
//! registered `(host, holding_slug, company)` target. The Next handlers and
//! their app re-exports are deliberately untouched. POST commands remain in
//! the TypeScript owner unit and are reported as a parity blocker.

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, get, header, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;
use sqlx::Row;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

// ---------------------------------------------------------------------------
// domain: bounded subpaths, owner SQL literals and DTOs
// ---------------------------------------------------------------------------

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

const RESERVED: [&str; 6] = ["prerequisites", "charts", "accounts", "periods", "rates", "disposals"];

pub const ASSET_LIST_SQL: &str = concat!(
    "SELECT id,asset_number,name,status,version,currency,cost_minor,accumulated_depreciation_minor ",
    "FROM finance_assets WHERE company_id=$1 ORDER BY asset_number,id"
);
pub const ASSET_DETAIL_SQL: &str = concat!(
    "SELECT a.id,a.account_id,a.asset_number,a.name,a.status,a.version,a.currency,",
    "a.cost_minor,a.residual_minor,a.useful_life_months,a.in_service_date,",
    "a.accumulated_depreciation_minor,a.depreciation_account_id,",
    "a.depreciation_expense_account_id,a.activation_journal_id,c.chart_id ",
    "FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id AND c.company_id=a.company_id ",
    "WHERE a.id=$1 AND a.company_id=$2"
);
pub const DISPOSALS_SQL: &str = concat!(
    "SELECT id,status,version,asset_version,entry_date,proceeds_minor,journal_id,created_by ",
    "FROM finance_asset_disposals WHERE asset_id=$1 AND company_id=$2 ORDER BY created_at,id"
);
pub const EVENTS_SQL: &str = concat!(
    "SELECT id,event_type,asset_version,created_at FROM finance_asset_events ",
    "WHERE asset_id=$1 AND company_id=$2 ORDER BY created_at,id"
);
pub const DEPRECIATIONS_SQL: &str = concat!(
    "SELECT id,depreciation_month,entry_date,amount_minor,journal_id ",
    "FROM finance_asset_depreciations WHERE asset_id=$1 AND company_id=$2 ORDER BY ordinal,id"
);
pub const PHYSICAL_LINKS_SQL: &str = concat!(
    "SELECT id,code,name,lifecycle_status AS status,version FROM operational_assets ",
    "WHERE company_id=$1 AND finance_asset_id=$2 AND ($3::text IS NULL OR branch_id=$3) ",
    "AND ($4::text IS NULL OR department_id=$4) ORDER BY id"
);
pub const ACCOUNT_OPTIONS_SQL: &str = concat!(
    "SELECT a.id,a.chart_id,a.code,a.name,a.category,a.status,a.currency ",
    "FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id ",
    "WHERE a.company_id=$1 AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r ",
    "WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved') ",
    "ORDER BY a.code,a.id"
);
pub const PERIOD_OPTIONS_SQL: &str = concat!(
    "SELECT id,name,start_date,end_date,state FROM finance_periods ",
    "WHERE company_id=$1 ORDER BY end_date DESC,id"
);
pub const DIMENSIONS_SQL: &str = concat!(
    "SELECT id,code,name,required FROM finance_dimension_definitions ",
    "WHERE company_id=$1 AND status='active' ORDER BY code"
);
pub const DIMENSION_VALUES_SQL: &str = concat!(
    "SELECT dimension_id,code,name FROM finance_dimension_values ",
    "WHERE company_id=$1 AND status='active' ORDER BY code"
);
pub const PREREQ_CHARTS_SQL: &str = concat!(
    "SELECT c.id,c.name,c.base_currency,c.status,c.version FROM finance_charts c ",
    "WHERE c.company_id=$1 ORDER BY c.id"
);
pub const PREREQ_ACCOUNTS_SQL: &str = concat!(
    "SELECT a.id,a.chart_id,a.code,a.name,a.category,a.currency,a.control_kind,a.status,a.version ",
    "FROM finance_accounts a WHERE a.company_id=$1 ORDER BY a.id"
);
pub const PREREQ_PERIODS_SQL: &str = concat!(
    "SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version AS version ",
    "FROM finance_periods p WHERE p.company_id=$1 ORDER BY p.id"
);
pub const PREREQ_RATES_SQL: &str = concat!(
    "SELECT r.id,r.currency,r.effective_date,r.currency_per_usd::text AS currency_per_usd,r.confirmed_at ",
    "FROM company_fx_rates r WHERE r.company_id=$1 ORDER BY r.id"
);
pub const PREREQ_MONEY_SQL: &str = concat!(
    "SELECT 'settings' AS kind FROM company_money_settings WHERE company_id=$1 ",
    "UNION ALL SELECT 'scale:'||currency AS kind FROM company_currency_scales WHERE company_id=$1 ",
    "ORDER BY kind"
);
/// The retained Finance owner source guard: every linked row must belong to
/// the same Company and the asset's account must still exist.
pub const SOURCE_INVALID_SQL: &str = concat!(
    "SELECT EXISTS(SELECT 1 FROM finance_assets a LEFT JOIN finance_accounts c ",
    "ON c.id=a.account_id AND c.company_id=a.company_id WHERE a.id=$1 AND a.company_id=$2 AND c.id IS NULL) ",
    "OR EXISTS(SELECT 1 FROM finance_asset_disposals d JOIN finance_assets a ON a.id=d.asset_id ",
    "WHERE a.id=$1 AND a.company_id=$2 AND d.company_id<>a.company_id) ",
    "OR EXISTS(SELECT 1 FROM finance_asset_events e JOIN finance_assets a ON a.id=e.asset_id ",
    "WHERE a.id=$1 AND a.company_id=$2 AND e.company_id<>a.company_id) ",
    "OR EXISTS(SELECT 1 FROM finance_asset_depreciations d JOIN finance_assets a ON a.id=d.asset_id ",
    "WHERE a.id=$1 AND a.company_id=$2 AND d.company_id<>a.company_id) ",
    "OR EXISTS(SELECT 1 FROM finance_asset_policy_editions p JOIN finance_assets a ON a.id=p.asset_id ",
    "WHERE a.id=$1 AND a.company_id=$2 AND p.company_id<>a.company_id) ",
    "OR EXISTS(SELECT 1 FROM finance_asset_policy_runs r JOIN finance_assets a ON a.id=r.asset_id ",
    "JOIN finance_asset_policy_editions p ON p.id=r.policy_id WHERE a.id=$1 AND a.company_id=$2 ",
    "AND (r.company_id<>a.company_id OR p.company_id<>a.company_id OR p.asset_id<>a.id)) AS invalid"
);

pub enum Subpath {
    Root,
    Prerequisites,
    Asset(String),
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'))
}

/// Only the three GET shapes retained by the Next catch-all are accepted.
/// Reference setup and asset commands are POST-only and stay TypeScript-owned.
pub fn parse_subpath(wildcard: &str) -> Option<Subpath> {
    let parts: Vec<&str> = wildcard.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [] => Some(Subpath::Root),
        [one] if *one == "prerequisites" => Some(Subpath::Prerequisites),
        [one] if !RESERVED.contains(one) && valid_id(one) => {
            Some(Subpath::Asset((*one).to_owned()))
        }
        _ => None,
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ASSETS_FINANCIAL_READ_UNAVAILABLE",
        "The native Assets financial read is unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ASSETS_FINANCIAL_SOURCE_CHANGED",
        "The original Asset financial/physical owner relationship requires review.",
    )
}

fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "ASSETS_FINANCIAL_REQUEST_INVALID",
        "Use one unambiguous Assets financial read request.",
    )
}

fn day(value: &str) -> String {
    value.chars().take(10).collect()
}

fn rate_text(value: &str) -> String {
    if !value.contains('.') {
        return value.to_owned();
    }
    let trimmed = value.trim_end_matches('0');
    trimmed.strip_suffix('.').unwrap_or(trimmed).to_owned()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSummary {
    pub id: String,
    pub asset_number: String,
    pub name: String,
    pub status: String,
    pub version: i64,
    pub currency: String,
    pub cost_minor: i64,
    pub book_value_minor: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFull {
    #[serde(flatten)]
    pub summary: AssetSummary,
    pub chart_id: String,
    pub account_id: String,
    pub depreciation_account_id: Option<String>,
    pub depreciation_expense_account_id: Option<String>,
    pub residual_minor: i64,
    pub useful_life_months: i64,
    pub in_service_date: String,
    pub accumulated_depreciation_minor: i64,
    pub activation_journal_id: Option<String>,
    pub can_edit: bool,
    pub can_cancel: bool,
    pub can_recognize: bool,
    pub can_prepare_disposal: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisposalView {
    pub id: String,
    pub asset_id: String,
    pub status: String,
    pub version: i64,
    pub asset_version: i64,
    pub entry_date: String,
    pub proceeds_minor: i64,
    pub journal_id: Option<String>,
    pub can_post: bool,
    pub can_cancel: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventView {
    pub id: String,
    pub action: String,
    pub asset_version: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepreciationView {
    pub id: String,
    pub month: String,
    pub entry_date: String,
    pub amount_minor: i64,
    pub journal_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalLinkView {
    pub id: String,
    pub code: String,
    pub name: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDetailView {
    pub asset: AssetFull,
    pub disposals: Vec<DisposalView>,
    pub events: Vec<EventView>,
    pub depreciations: Vec<DepreciationView>,
    pub physical_links: Vec<PhysicalLinkView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOption {
    pub id: String,
    pub chart_id: String,
    pub code: String,
    pub name: String,
    pub category: String,
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodOption {
    pub id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dimension {
    pub id: String,
    pub code: String,
    pub name: String,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionValue {
    pub dimension_id: String,
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub company_id: String,
    pub available: bool,
    pub context_key: Option<String>,
    pub can_create: bool,
    pub can_read_physical_register: bool,
    pub can_register_physical: bool,
    pub assets: Vec<AssetSummary>,
    pub detail: Option<AssetDetailView>,
    pub accounts: Vec<AccountOption>,
    pub periods: Vec<PeriodOption>,
    pub dimensions: Vec<Dimension>,
    pub dimension_values: Vec<DimensionValue>,
    pub unavailable_reason: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartOption {
    pub id: String,
    pub name: String,
    pub base_currency: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrereqAccount {
    pub id: String,
    pub chart_id: String,
    pub code: String,
    pub name: String,
    pub category: String,
    pub currency: Option<String>,
    pub control_kind: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrereqPeriod {
    pub id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub state: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateView {
    pub id: String,
    pub currency: String,
    pub effective_date: String,
    pub currency_per_usd: String,
    pub confirmed_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prerequisites {
    pub company_id: String,
    pub available: bool,
    pub context_key: Option<String>,
    pub can_create_chart: bool,
    pub can_create_account: bool,
    pub can_create_period: bool,
    pub can_confirm_rate: bool,
    pub charts: Vec<ChartOption>,
    pub accounts: Vec<PrereqAccount>,
    pub periods: Vec<PrereqPeriod>,
    pub rates: Vec<RateView>,
    pub unavailable_reason: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// application: owner ports and pure payload assembly
// ---------------------------------------------------------------------------

pub type ReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug)]
pub struct ReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub company: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct WorkspaceAuthority {
    pub company_id: String,
    pub user_id: String,
    /// `None` when the native authority/purchase admission is available.
    pub reason: Option<&'static str>,
    pub context_key: Option<String>,
    pub can_create: bool,
    pub can_read_physical_register: bool,
    pub can_register_physical: bool,
    pub can_write: bool,
    pub can_approve: bool,
    pub independent: bool,
    pub recognition_settled: bool,
    pub disposal_settled: bool,
}

#[derive(Clone, Debug)]
pub struct PrerequisiteAuthority {
    pub company_id: String,
    pub reason: Option<&'static str>,
    pub context_key: Option<String>,
    pub can_create_chart: bool,
    pub can_create_account: bool,
    pub can_create_period: bool,
    pub can_confirm_rate: bool,
}

/// Composition-owned authority. It mints no data row and executes no SQL; the
/// opaque context tokens are the exact owner command tokens, never a grant.
pub trait AssetsFinancialAuthority: Send + Sync {
    fn workspace<'a>(
        &'a self,
        request: &'a ReadRequest<'a>,
        asset_id: Option<&'a str>,
    ) -> ReadFuture<'a, WorkspaceAuthority>;
    fn prerequisites<'a>(
        &'a self,
        request: &'a ReadRequest<'a>,
    ) -> ReadFuture<'a, PrerequisiteAuthority>;
}

#[derive(Clone, Debug, Default)]
pub struct AssetRow {
    pub id: String,
    pub account_id: String,
    pub asset_number: String,
    pub name: String,
    pub status: String,
    pub version: i64,
    pub currency: String,
    pub cost_minor: i64,
    pub residual_minor: i64,
    pub useful_life_months: i64,
    pub in_service_date: String,
    pub accumulated_depreciation_minor: i64,
    pub depreciation_account_id: Option<String>,
    pub depreciation_expense_account_id: Option<String>,
    pub activation_journal_id: Option<String>,
    pub chart_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct AssetRows {
    pub assets: Vec<AssetRow>,
    pub detail: Option<AssetRow>,
    pub disposals: Vec<(String, String, i64, i64, String, i64, Option<String>, String)>,
    pub events: Vec<(String, String, i64, i64)>,
    pub depreciations: Vec<(String, String, String, i64, String)>,
    pub physical_links: Vec<(String, String, String, String, i64)>,
    pub accounts: Vec<(String, String, String, String, String, Option<String>, String)>,
    pub periods: Vec<(String, String, String, String, String)>,
    pub dimensions: Vec<(String, String, String, i64)>,
    pub dimension_values: Vec<(String, String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct PrerequisiteRows {
    pub charts: Vec<(String, String, String, String, i64)>,
    pub accounts: Vec<(
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        i64,
    )>,
    pub periods: Vec<(String, String, String, String, String, i64)>,
    pub rates: Vec<(String, String, String, String, i64)>,
    pub money_count: usize,
}

pub trait AssetsFinancialReadBackend: Send + Sync {
    fn workspace<'a>(
        &'a self,
        request: &'a ReadRequest<'a>,
        asset_id: Option<&'a str>,
    ) -> ReadFuture<'a, Workspace>;
    fn prerequisites<'a>(
        &'a self,
        request: &'a ReadRequest<'a>,
    ) -> ReadFuture<'a, Prerequisites>;
}

pub fn summary(row: &AssetRow) -> AssetSummary {
    AssetSummary {
        id: row.id.clone(),
        asset_number: row.asset_number.clone(),
        name: row.name.clone(),
        status: row.status.clone(),
        version: row.version,
        currency: row.currency.clone(),
        cost_minor: row.cost_minor,
        book_value_minor: row.cost_minor - row.accumulated_depreciation_minor,
    }
}

fn unavailable_workspace(company: &str, auth: &WorkspaceAuthority) -> Workspace {
    Workspace {
        company_id: company.to_owned(),
        available: false,
        context_key: None,
        can_create: false,
        can_read_physical_register: false,
        can_register_physical: false,
        assets: Vec::new(),
        detail: None,
        accounts: Vec::new(),
        periods: Vec::new(),
        dimensions: Vec::new(),
        dimension_values: Vec::new(),
        unavailable_reason: auth.reason,
    }
}

fn unavailable_prerequisites(company: &str, auth: &PrerequisiteAuthority) -> Prerequisites {
    Prerequisites {
        company_id: company.to_owned(),
        available: false,
        context_key: None,
        can_create_chart: false,
        can_create_account: false,
        can_create_period: false,
        can_confirm_rate: false,
        charts: Vec::new(),
        accounts: Vec::new(),
        periods: Vec::new(),
        rates: Vec::new(),
        unavailable_reason: auth.reason,
    }
}

pub fn assemble_workspace(
    company: &str,
    auth: WorkspaceAuthority,
    rows: AssetRows,
    can_read_physical: bool,
) -> Workspace {
    if auth.reason.is_some() {
        return unavailable_workspace(company, &auth);
    }
    let detail = rows.detail.map(|asset| {
        let draft = asset.status == "draft";
        let has_accounts = asset.depreciation_account_id.is_some()
            && asset.depreciation_expense_account_id.is_some();
        let can_prepare_disposal = asset.status == "in_service"
            && asset.activation_journal_id.is_some()
            && auth.can_write
            && auth.disposal_settled
            && !rows.disposals.iter().any(|row| row.1 == "draft");
        let asset_version = asset.version;
        let asset_id = asset.id.clone();
        AssetDetailView {
            asset: AssetFull {
                summary: summary(&asset),
                chart_id: asset.chart_id.clone(),
                account_id: asset.account_id.clone(),
                depreciation_account_id: asset.depreciation_account_id.clone(),
                depreciation_expense_account_id: asset.depreciation_expense_account_id.clone(),
                residual_minor: asset.residual_minor,
                useful_life_months: asset.useful_life_months,
                in_service_date: asset.in_service_date.clone(),
                accumulated_depreciation_minor: asset.accumulated_depreciation_minor,
                activation_journal_id: asset.activation_journal_id.clone(),
                can_edit: draft && auth.can_write && auth.can_create,
                can_cancel: draft && auth.can_write && auth.recognition_settled,
                can_recognize: draft
                    && asset.activation_journal_id.is_none()
                    && has_accounts
                    && auth.can_approve
                    && auth.recognition_settled
                    && auth.independent,
                can_prepare_disposal,
            },
            disposals: rows
                .disposals
                .into_iter()
                .map(
                    |(id, status, version, disposal_asset_version, entry_date, proceeds_minor, journal_id, created_by)| {
                        let draft = status == "draft" && disposal_asset_version == asset_version;
                        DisposalView {
                            id,
                            asset_id: asset_id.clone(),
                            status,
                            version,
                            asset_version: disposal_asset_version,
                            entry_date: day(&entry_date),
                            proceeds_minor,
                            journal_id,
                            can_post: draft
                                && auth.can_approve
                                && auth.disposal_settled
                                && created_by != auth.user_id,
                            can_cancel: draft && auth.can_write && auth.disposal_settled,
                        }
                    },
                )
                .collect(),
            events: rows
                .events
                .into_iter()
                .map(|(id, action, asset_version, created_at)| EventView {
                    id,
                    action,
                    asset_version,
                    created_at,
                })
                .collect(),
            depreciations: rows
                .depreciations
                .into_iter()
                .map(|(id, month, entry_date, amount_minor, journal_id)| DepreciationView {
                    id,
                    month,
                    entry_date: day(&entry_date),
                    amount_minor,
                    journal_id,
                })
                .collect(),
            physical_links: if can_read_physical {
                rows.physical_links
                    .into_iter()
                    .map(|(id, code, name, status, version)| PhysicalLinkView {
                        id,
                        code,
                        name,
                        status,
                        version,
                    })
                    .collect()
            } else {
                Vec::new()
            },
        }
    });
    Workspace {
        company_id: company.to_owned(),
        available: true,
        context_key: auth.context_key.clone(),
        can_create: auth.can_create,
        can_read_physical_register: auth.can_read_physical_register,
        can_register_physical: auth.can_register_physical,
        assets: rows.assets.iter().map(summary).collect(),
        detail,
        accounts: rows
            .accounts
            .into_iter()
            .filter(|row| row.6 == "active")
            .map(|(id, chart_id, code, name, category, currency, _)| AccountOption {
                id,
                chart_id,
                code,
                name,
                category,
                currency,
            })
            .collect(),
        periods: rows
            .periods
            .into_iter()
            .filter(|row| row.4 == "open")
            .map(|(id, name, start_date, end_date, state)| PeriodOption {
                id,
                name,
                start_date: day(&start_date),
                end_date: day(&end_date),
                state,
            })
            .collect(),
        dimensions: rows
            .dimensions
            .into_iter()
            .map(|(id, code, name, required)| Dimension {
                id,
                code,
                name,
                required: required != 0,
            })
            .collect(),
        dimension_values: rows
            .dimension_values
            .into_iter()
            .map(|(dimension_id, code, name)| DimensionValue {
                dimension_id,
                code,
                name,
            })
            .collect(),
        unavailable_reason: None,
    }
}

pub fn assemble_prerequisites(company: &str, auth: PrerequisiteAuthority, rows: PrerequisiteRows) -> Prerequisites {
    if auth.reason.is_some() {
        return unavailable_prerequisites(company, &auth);
    }
    Prerequisites {
        company_id: company.to_owned(),
        available: true,
        context_key: auth.context_key.clone(),
        can_create_chart: auth.can_create_chart,
        can_create_account: auth.can_create_account,
        can_create_period: auth.can_create_period,
        can_confirm_rate: auth.can_confirm_rate,
        charts: rows
            .charts
            .into_iter()
            .filter(|row| row.3 == "active")
            .map(|(id, name, base_currency, _, _)| ChartOption {
                id,
                name,
                base_currency,
            })
            .collect(),
        accounts: rows
            .accounts
            .into_iter()
            .filter(|row| row.7 == "active")
            .map(|(id, chart_id, code, name, category, currency, control_kind, _, _)| PrereqAccount {
                id,
                chart_id,
                code,
                name,
                category,
                currency,
                control_kind,
            })
            .collect(),
        periods: rows
            .periods
            .into_iter()
            .map(|(id, name, start_date, end_date, state, version)| PrereqPeriod {
                id,
                name,
                start_date: day(&start_date),
                end_date: day(&end_date),
                state,
                version,
            })
            .collect(),
        rates: rows
            .rates
            .into_iter()
            .map(|(id, currency, effective_date, currency_per_usd, confirmed_at)| RateView {
                id,
                currency,
                effective_date: day(&effective_date),
                currency_per_usd: rate_text(&currency_per_usd),
                confirmed_at,
            })
            .collect(),
        unavailable_reason: None,
    }
}

// ---------------------------------------------------------------------------
// adapters: SQLx/Postgres execution
// ---------------------------------------------------------------------------

fn text(row: &sqlx::postgres::PgRow, key: &str) -> Result<String, BusinessError> {
    row.try_get::<String, _>(key).map_err(|_| unavailable())
}
fn opt_text(row: &sqlx::postgres::PgRow, key: &str) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(key).map_err(|_| unavailable())
}
fn integer(row: &sqlx::postgres::PgRow, key: &str) -> Result<i64, BusinessError> {
    row.try_get::<i64, _>(key).map_err(|_| unavailable())
}
fn summary_row(row: &sqlx::postgres::PgRow) -> Result<AssetRow, BusinessError> {
    Ok(AssetRow {
        id: text(row, "id")?,
        account_id: String::new(),
        asset_number: text(row, "asset_number")?,
        name: text(row, "name")?,
        status: text(row, "status")?,
        version: integer(row, "version")?,
        currency: text(row, "currency")?,
        cost_minor: integer(row, "cost_minor")?,
        accumulated_depreciation_minor: integer(row, "accumulated_depreciation_minor")?,
        ..AssetRow::default()
    })
}

fn detail_row(row: &sqlx::postgres::PgRow) -> Result<AssetRow, BusinessError> {
    Ok(AssetRow {
        account_id: text(row, "account_id")?,
        residual_minor: integer(row, "residual_minor")?,
        useful_life_months: integer(row, "useful_life_months")?,
        in_service_date: text(row, "in_service_date")?,
        depreciation_account_id: opt_text(row, "depreciation_account_id")?,
        depreciation_expense_account_id: opt_text(row, "depreciation_expense_account_id")?,
        activation_journal_id: opt_text(row, "activation_journal_id")?,
        chart_id: text(row, "chart_id")?,
        ..summary_row(row)?
    })
}

pub struct PgAssetsFinancialReadBackend {
    pool: DatabasePool,
    authority: Arc<dyn AssetsFinancialAuthority>,
}

impl PgAssetsFinancialReadBackend {
    pub fn new(pool: DatabasePool, authority: Arc<dyn AssetsFinancialAuthority>) -> Self {
        Self { pool, authority }
    }
}

impl AssetsFinancialReadBackend for PgAssetsFinancialReadBackend {
    fn workspace<'a>(
        &'a self,
        request: &'a ReadRequest<'a>,
        asset_id: Option<&'a str>,
    ) -> ReadFuture<'a, Workspace> {
        let pool = self.pool.clone();
        let authority = self.authority.clone();
        let company = request.company.to_owned();
        Box::pin(async move {
            let auth = authority.workspace(request, asset_id).await?;
            if auth.company_id != company {
                return Err(invalid_request());
            }
            if auth.reason.is_some() {
                return Ok(unavailable_workspace(&company, &auth));
            }
            let mut tx = pool.raw().begin().await.map_err(|_| unavailable())?;
            let mut rows = AssetRows::default();
            // The retained owner returns an empty list for a detail read and
            // only populates `assets` for the unqualified workspace.
            if asset_id.is_none() {
                let list = sqlx::query(ASSET_LIST_SQL)
                    .bind(company.as_str())
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|_| unavailable())?;
                for row in &list {
                    rows.assets.push(summary_row(row)?);
                }
            }
            if let Some(id) = asset_id {
                let detail = sqlx::query(ASSET_DETAIL_SQL)
                    .bind(id)
                    .bind(company.as_str())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|_| unavailable())?;
                if let Some(row) = detail {
                    rows.detail = Some(detail_row(&row)?);
                    let invalid = sqlx::query_scalar::<_, bool>(SOURCE_INVALID_SQL)
                        .bind(id)
                        .bind(company.as_str())
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(|_| unavailable())?;
                    if invalid {
                        return Err(source_changed());
                    }
                    for row in sqlx::query(DISPOSALS_SQL)
                        .bind(id)
                        .bind(company.as_str())
                        .fetch_all(&mut *tx)
                        .await
                        .map_err(|_| unavailable())?
                    {
                        rows.disposals.push((
                            text(&row, "id")?,
                            text(&row, "status")?,
                            integer(&row, "version")?,
                            integer(&row, "asset_version")?,
                            text(&row, "entry_date")?,
                            integer(&row, "proceeds_minor")?,
                            opt_text(&row, "journal_id")?,
                            text(&row, "created_by")?,
                        ));
                    }
                    for row in sqlx::query(EVENTS_SQL)
                        .bind(id)
                        .bind(company.as_str())
                        .fetch_all(&mut *tx)
                        .await
                        .map_err(|_| unavailable())?
                    {
                        rows.events.push((
                            text(&row, "id")?,
                            text(&row, "event_type")?,
                            integer(&row, "asset_version")?,
                            integer(&row, "created_at")?,
                        ));
                    }
                    for row in sqlx::query(DEPRECIATIONS_SQL)
                        .bind(id)
                        .bind(company.as_str())
                        .fetch_all(&mut *tx)
                        .await
                        .map_err(|_| unavailable())?
                    {
                        rows.depreciations.push((
                            text(&row, "id")?,
                            text(&row, "depreciation_month")?,
                            text(&row, "entry_date")?,
                            integer(&row, "amount_minor")?,
                            text(&row, "journal_id")?,
                        ));
                    }
                    for row in sqlx::query(PHYSICAL_LINKS_SQL)
                        .bind(company.as_str())
                        .bind(id)
                        .bind(None::<&str>)
                        .bind(None::<&str>)
                        .fetch_all(&mut *tx)
                        .await
                        .map_err(|_| unavailable())?
                    {
                        rows.physical_links.push((
                            text(&row, "id")?,
                            text(&row, "code")?,
                            text(&row, "name")?,
                            text(&row, "status")?,
                            integer(&row, "version")?,
                        ));
                    }
                }
            }
            for row in sqlx::query(ACCOUNT_OPTIONS_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.accounts.push((
                    text(&row, "id")?,
                    text(&row, "chart_id")?,
                    text(&row, "code")?,
                    text(&row, "name")?,
                    text(&row, "category")?,
                    opt_text(&row, "currency")?,
                    text(&row, "status")?,
                ));
            }
            for row in sqlx::query(PERIOD_OPTIONS_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.periods.push((
                    text(&row, "id")?,
                    text(&row, "name")?,
                    text(&row, "start_date")?,
                    text(&row, "end_date")?,
                    text(&row, "state")?,
                ));
            }
            for row in sqlx::query(DIMENSIONS_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.dimensions.push((
                    text(&row, "id")?,
                    text(&row, "code")?,
                    text(&row, "name")?,
                    integer(&row, "required")?,
                ));
            }
            for row in sqlx::query(DIMENSION_VALUES_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.dimension_values.push((
                    text(&row, "dimension_id")?,
                    text(&row, "code")?,
                    text(&row, "name")?,
                ));
            }
            tx.commit().await.map_err(|_| unavailable())?;
            let can_read_physical = auth.can_read_physical_register;
            Ok(assemble_workspace(&company, auth, rows, can_read_physical))
        })
    }

    fn prerequisites<'a>(
        &'a self,
        request: &'a ReadRequest<'a>,
    ) -> ReadFuture<'a, Prerequisites> {
        let pool = self.pool.clone();
        let authority = self.authority.clone();
        let company = request.company.to_owned();
        Box::pin(async move {
            let auth = authority.prerequisites(request).await?;
            if auth.company_id != company {
                return Err(invalid_request());
            }
            if auth.reason.is_some() {
                return Ok(unavailable_prerequisites(&company, &auth));
            }
            let mut tx = pool.raw().begin().await.map_err(|_| unavailable())?;
            let mut rows = PrerequisiteRows::default();
            for row in sqlx::query(PREREQ_CHARTS_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.charts.push((
                    text(&row, "id")?,
                    text(&row, "name")?,
                    text(&row, "base_currency")?,
                    text(&row, "status")?,
                    integer(&row, "version")?,
                ));
            }
            for row in sqlx::query(PREREQ_ACCOUNTS_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.accounts.push((
                    text(&row, "id")?,
                    text(&row, "chart_id")?,
                    text(&row, "code")?,
                    text(&row, "name")?,
                    text(&row, "category")?,
                    opt_text(&row, "currency")?,
                    text(&row, "control_kind")?,
                    text(&row, "status")?,
                    integer(&row, "version")?,
                ));
            }
            for row in sqlx::query(PREREQ_PERIODS_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.periods.push((
                    text(&row, "id")?,
                    text(&row, "name")?,
                    text(&row, "start_date")?,
                    text(&row, "end_date")?,
                    text(&row, "state")?,
                    integer(&row, "version")?,
                ));
            }
            for row in sqlx::query(PREREQ_RATES_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
            {
                rows.rates.push((
                    text(&row, "id")?,
                    text(&row, "currency")?,
                    text(&row, "effective_date")?,
                    text(&row, "currency_per_usd")?,
                    integer(&row, "confirmed_at")?,
                ));
            }
            rows.money_count = sqlx::query(PREREQ_MONEY_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| unavailable())?
                .len();
            tx.commit().await.map_err(|_| unavailable())?;
            Ok(assemble_prerequisites(&company, auth, rows))
        })
    }
}

// ---------------------------------------------------------------------------
// http: exact envelope, headers and bounded routing
// ---------------------------------------------------------------------------

pub struct AssetsFinancialTarget {
    pub host: String,
    pub holding_slug: String,
    pub company_id: String,
    pub backend: Arc<dyn AssetsFinancialReadBackend>,
}

struct AssetsFinancialState {
    targets: BTreeMap<(String, String), AssetsFinancialTarget>,
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
        .transpose()
}

fn target<'a>(
    state: &'a AssetsFinancialState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a AssetsFinancialTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(not_found)?;
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|company| !company.is_empty())
        .ok_or_else(|| {
            BusinessError::new(FailureKind::Unauthenticated, "UNAUTHENTICATED", "Sign in to continue.")
        })?;
    let target = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(not_found)?;
    if target.company_id != company {
        return Err(invalid_request());
    }
    unique_header(headers, CONTEXT_USER)?
        .filter(|user| !user.is_empty())
        .ok_or_else(|| {
            BusinessError::new(FailureKind::Unauthenticated, "UNAUTHENTICATED", "Sign in to continue.")
        })?;
    Ok(target)
}

async fn read_workspace(
    state: &AssetsFinancialState,
    slug: &str,
    headers: &HeaderMap,
    asset_id: Option<&str>,
) -> Result<serde_json::Value, BusinessError> {
    let target = target(state, headers, slug)?;
    let request = ReadRequest {
        host: target.host.as_str(),
        holding_slug: slug,
        company: target.company_id.as_str(),
        headers,
    };
    let workspace = target.backend.workspace(&request, asset_id).await?;
    let prerequisites = target.backend.prerequisites(&request).await?;
    Ok(serde_json::json!({"data":{"workspace":workspace,"prerequisites":prerequisites}}))
}

fn configuration(error: &'static str) -> BusinessError {
    BusinessError::invalid("ASSETS_FINANCIAL_CONFIG_INVALID", error)
}

/// Composition mounts this router on the private native service. No caller can
/// supply SQL, another Company or another Holding target.
pub fn router(
    targets: Vec<AssetsFinancialTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || target.company_id.is_empty()
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(configuration("Use exact distinct registered Holding Company targets."));
        }
    }
    if configured.is_empty() {
        return Err(configuration("Register at least one exact Holding Company target."));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/assets/financial-assets",
            get(root),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/assets/financial-assets/{*path}",
            get(wildcard),
        )
        .with_state(Arc::new(AssetsFinancialState { targets: configured })))
}

async fn root(
    State(state): State<Arc<AssetsFinancialState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    match read_workspace(&state, &slug, &headers, None).await {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

async fn wildcard(
    State(state): State<Arc<AssetsFinancialState>>,
    Path((slug, path)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let state = state.as_ref();
        match parse_subpath(&path).ok_or_else(not_found)? {
            Subpath::Root => read_workspace(state, &slug, &headers, None).await,
            Subpath::Prerequisites => {
                let target = target(state, &headers, &slug)?;
                let read = ReadRequest {
                    host: target.host.as_str(),
                    holding_slug: &slug,
                    company: target.company_id.as_str(),
                    headers: &headers,
                };
                let prerequisites = target.backend.prerequisites(&read).await?;
                Ok(serde_json::json!({"data":prerequisites}))
            }
            Subpath::Asset(id) => read_workspace(state, &slug, &headers, Some(&id)).await,
        }
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

// ---------------------------------------------------------------------------
// unit tests (no database, no server)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_subpaths_are_the_retained_get_shapes() {
        assert!(matches!(parse_subpath(""), Some(Subpath::Root)));
        assert!(matches!(parse_subpath("prerequisites"), Some(Subpath::Prerequisites)));
        assert!(matches!(parse_subpath("asset-42"), Some(Subpath::Asset(_))));
        assert!(parse_subpath("charts").is_none());
        assert!(parse_subpath("accounts").is_none());
        assert!(parse_subpath("asset-1/edit").is_none());
        assert!(parse_subpath("bad/id").is_none());
    }

    #[test]
    fn sql_is_company_scoped_and_executes_real_owner_tables() {
        assert!(ASSET_LIST_SQL.contains("company_id=$1"));
        assert!(ASSET_DETAIL_SQL.contains("a.id=$1 AND a.company_id=$2"));
        assert!(DISPOSALS_SQL.contains("finance_asset_disposals"));
        assert!(EVENTS_SQL.contains("finance_asset_events"));
        assert!(DEPRECIATIONS_SQL.contains("finance_asset_depreciations"));
        assert!(PHYSICAL_LINKS_SQL.contains("operational_assets"));
        assert!(ACCOUNT_OPTIONS_SQL.contains("secondary_chart_id=a.chart_id"));
        assert!(SOURCE_INVALID_SQL.contains("finance_asset_policy_runs"));
        assert!(PREREQ_RATES_SQL.contains("company_fx_rates"));
        assert!(PREREQ_MONEY_SQL.contains("company_currency_scales"));
    }

    #[test]
    fn summary_book_value_and_rate_text_match_the_owner_projection() {
        let asset = AssetRow {
            cost_minor: 5000,
            accumulated_depreciation_minor: 1200,
            ..AssetRow::default()
        };
        assert_eq!(summary(&asset).book_value_minor, 3800);
        assert_eq!(rate_text("1.250000"), "1.25");
        assert_eq!(rate_text("10"), "10");
        assert_eq!(day("2026-11-28T00:00:00"), "2026-11-28");
    }

    #[test]
    fn unavailable_workspace_is_exact() {
        let value = serde_json::to_value(unavailable_workspace(
            "acme:001",
            &WorkspaceAuthority {
                company_id: "acme:001".into(),
                user_id: "u-1".into(),
                reason: Some("purchase-required"),
                context_key: None,
                can_create: false,
                can_read_physical_register: false,
                can_register_physical: false,
                can_write: false,
                can_approve: false,
                independent: false,
                recognition_settled: false,
                disposal_settled: false,
            },
        ))
        .unwrap();
        let expected = serde_json::json!({
            "companyId":"acme:001","available":false,"contextKey":null,
            "canCreate":false,"canReadPhysicalRegister":false,"canRegisterPhysical":false,
            "assets":[],"detail":null,"accounts":[],"periods":[],"dimensions":[],
            "dimensionValues":[],"unavailableReason":"purchase-required"
        });
        assert_eq!(value, expected);
    }

    #[test]
    fn unavailable_prerequisites_is_exact() {
        let value = serde_json::to_value(unavailable_prerequisites(
            "acme:001",
            &PrerequisiteAuthority {
                company_id: "acme:001".into(),
                reason: Some("authority-required"),
                context_key: None,
                can_create_chart: false,
                can_create_account: false,
                can_create_period: false,
                can_confirm_rate: false,
            },
        ))
        .unwrap();
        assert_eq!(value["available"], false);
        assert_eq!(value["contextKey"], serde_json::Value::Null);
        assert_eq!(value["unavailableReason"], "authority-required");
        assert_eq!(value.as_object().unwrap().len(), 12);
    }

    #[test]
    fn workspace_projection_filters_and_flags_match_the_owner() {
        let asset = AssetRow {
            id: "asset-1".into(),
            asset_number: "A-1".into(),
            name: "Laptop".into(),
            status: "draft".into(),
            version: 2,
            currency: "USD".into(),
            cost_minor: 1000,
            accumulated_depreciation_minor: 100,
            residual_minor: 0,
            useful_life_months: 36,
            in_service_date: "2026-01-01".into(),
            depreciation_account_id: Some("dep-1".into()),
            depreciation_expense_account_id: Some("exp-1".into()),
            activation_journal_id: None,
            chart_id: "chart-1".into(),
            account_id: "acc-1".into(),
        };
        let rows = AssetRows {
            detail: Some(asset.clone()),
            disposals: vec![(
                "disposal-1".into(),
                "draft".into(),
                1,
                1,
                "2026-02-01T00:00:00".into(),
                500,
                None,
                "u-1".into(),
            )],
            events: vec![("event-1".into(), "created".into(), 1, 123)],
            depreciations: vec![(
                "dep-1".into(),
                "2026-01".into(),
                "2026-01-31T00:00:00".into(),
                25,
                "journal-1".into(),
            )],
            physical_links: vec![("p-1".into(), "PA-1".into(), "Laptop".into(), "active".into(), 1)],
            accounts: vec![(
                "acc-1".into(),
                "chart-1".into(),
                "1000".into(),
                "Cash".into(),
                "asset".into(),
                None,
                "active".into(),
            )],
            periods: vec![
                ("period-1".into(), "Jan".into(), "2026-01-01".into(), "2026-01-31".into(), "open".into()),
                ("period-2".into(), "Feb".into(), "2026-02-01".into(), "2026-02-28".into(), "closed".into()),
            ],
            dimensions: vec![("dim-1".into(), "CC".into(), "Cost centre".into(), 1)],
            dimension_values: vec![("dim-1".into(), "CC-1".into(), "IT".into())],
            ..AssetRows::default()
        };
        let auth = WorkspaceAuthority {
            company_id: "acme:001".into(),
            user_id: "u-1".into(),
            reason: None,
            context_key: Some("ctx".into()),
            can_create: true,
            can_read_physical_register: true,
            can_register_physical: true,
            can_write: true,
            can_approve: true,
            independent: true,
            recognition_settled: true,
            disposal_settled: true,
        };
        let value = serde_json::to_value(assemble_workspace("acme:001", auth, rows, true)).unwrap();
        assert_eq!(value["available"], true);
        assert_eq!(value["contextKey"], "ctx");
        assert_eq!(value["assets"].as_array().unwrap().len(), 0);
        assert_eq!(value["detail"]["asset"]["bookValueMinor"], 900);
        assert_eq!(value["detail"]["asset"]["canEdit"], true);
        assert_eq!(value["detail"]["asset"]["canRecognize"], true);
        assert_eq!(value["detail"]["asset"]["canPrepareDisposal"], false);
        assert_eq!(value["detail"]["disposals"][0]["entryDate"], "2026-02-01");
        assert_eq!(value["detail"]["disposals"][0]["canPost"], false);
        assert_eq!(value["detail"]["depreciations"][0]["month"], "2026-01");
        assert_eq!(value["periods"].as_array().unwrap().len(), 1);
        assert_eq!(value["dimensionValues"][0]["dimensionId"], "dim-1");
    }

    #[test]
    fn prerequisites_projection_exact_fields() {
        let rows = PrerequisiteRows {
            charts: vec![("chart-1".into(), "Main".into(), "USD".into(), "active".into(), 1), ("chart-2".into(), "Old".into(), "USD".into(), "archived".into(), 1)],
            accounts: vec![("acc-1".into(), "chart-1".into(), "1000".into(), "Cash".into(), "asset".into(), None, "none".into(), "active".into(), 1)],
            periods: vec![("p-1".into(), "Jan".into(), "2026-01-01".into(), "2026-01-31".into(), "open".into(), 3)],
            rates: vec![("r-1".into(), "EUR".into(), "2026-01-01T00:00:00".into(), "1.250000".into(), 100)],
            money_count: 2,
        };
        let auth = PrerequisiteAuthority {
            company_id: "acme:001".into(),
            reason: None,
            context_key: Some("pctx".into()),
            can_create_chart: true,
            can_create_account: true,
            can_create_period: true,
            can_confirm_rate: false,
        };
        let value = serde_json::to_value(assemble_prerequisites("acme:001", auth, rows)).unwrap();
        assert_eq!(value["charts"].as_array().unwrap().len(), 1);
        assert_eq!(value["periods"][0]["version"], 3);
        assert_eq!(value["rates"][0]["currencyPerUsd"], "1.25");
        assert_eq!(value["rates"][0]["effectiveDate"], "2026-01-01");
        assert_eq!(value["canConfirmRate"], false);
        assert_eq!(value.as_object().unwrap().len(), 12);
    }

    #[test]
    fn envelope_and_cache_header_match_the_retained_next_read() {
        let value = serde_json::json!({"data":{"workspace":{"available":true},"prerequisites":{"available":true}}});
        let response = bounded(value);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        struct Backend;
        impl AssetsFinancialReadBackend for Backend {
            fn workspace<'a>(&'a self, _: &'a ReadRequest<'a>, _: Option<&'a str>) -> ReadFuture<'a, Workspace> {
                Box::pin(async { Err(unavailable()) })
            }
            fn prerequisites<'a>(&'a self, _: &'a ReadRequest<'a>) -> ReadFuture<'a, Prerequisites> {
                Box::pin(async { Err(unavailable()) })
            }
        }
        let target = AssetsFinancialTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            company_id: "acme:001".into(),
            backend: Arc::new(Backend),
        };
        assert!(router(vec![target]).is_ok());
        assert!(router(vec![
            AssetsFinancialTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into(), company_id: "acme:001".into(), backend: Arc::new(Backend) },
            AssetsFinancialTarget { host: "acme.jesadakorn.com".into(), holding_slug: "acme".into(), company_id: "acme:001".into(), backend: Arc::new(Backend) },
        ])
        .is_err());
        assert!(router(Vec::new()).is_err());
    }

    #[test]
    fn company_scope_and_duplicate_headers_fail_closed() {
        let mut headers = HeaderMap::new();
        assert!(unique_header(&headers, CONTEXT_COMPANY).unwrap().is_none());
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert_eq!(unique_header(&headers, CONTEXT_COMPANY).unwrap(), Some("acme:001"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }
}
