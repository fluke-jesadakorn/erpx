//! Native R3 building block for `GET /api/v1/holdings/{holdingSlug}/pos/startup` (L10).
//!
//! The retained TypeScript GET runs three owner reads in parallel
//! (`posStartupPrerequisites`, `posPostingSetup`, `posStartupWorkspace`) and
//! derives authority/purchase/context-key facts across Finance, Commerce and
//! POS. This module is deliberately bounded to the **POS-owned** part of that
//! payload: settings prepared state and the startup register list. Both SQL
//! statements are the exact owner-registered literals
//! (`pos_core_keyed_settings_prepared_state`, `pos_core_startup_registers`)
//! that the TypeScript bridges currently bind and execute on the caller unit;
//! here the owner executes them itself with SQLx.
//!
//! It is additive and unique. It does not replace the TS GET route: the
//! prerequisites and posting-mapping reads still execute cross-owner SQL and
//! MAC/context-key derivation in TypeScript. See the worker report for the
//! exact blocker and the `pub mod`/mount lines for integration.

use erpx_platform_runtime_server::public::http_support::{
    header, routing::get, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request,
    Response, Router, State, StatusCode, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{PgPool, Row};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

/// `pos_core_keyed_settings_prepared_state`, `?` translated to `$1`.
pub const SETTINGS_SQL: &str =
    "SELECT version,accounting_prepared_by,accounting_approved_by FROM pos_settings WHERE company_id=$1";
/// `pos_core_startup_registers`, `?` translated to `$1`, `$2`, `$3`.
pub const REGISTERS_SQL: &str = "SELECT id,branch_id,warehouse_id,name,mode,active,version FROM pos_registers WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$3) ORDER BY id";

pub const FACTS_PATH: &str = "/api/v1/holdings/{holding_slug}/pos/startup/facts";

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Use one bounded native POS startup read.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_STARTUP_FACTS_UNAVAILABLE",
        "The native POS startup reader is unavailable.",
    )
}

/// Exact projection of the POS-owned startup workspace fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Register {
    pub id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub name: String,
    pub mode: String,
    pub active: bool,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    pub version: i64,
    pub accounting_prepared_by: Option<String>,
    pub accounting_approved_by: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupFacts {
    pub settings: Option<Settings>,
    pub registers: Vec<Register>,
}

impl StartupFacts {
    /// The `data.workspace` subset of the retained GET payload. Field names,
    /// casing and booleans match `posStartupWorkspace` exactly.
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({
            "data": {
                "workspace": {
                    "configured": self.settings.is_some(),
                    "settingsVersion": self.settings.as_ref().map(|s| s.version),
                    "accountingPrepared": self.settings.as_ref().and_then(|s| s.accounting_prepared_by.as_ref()).is_some(),
                    "accountingApproved": self.settings.as_ref().and_then(|s| s.accounting_approved_by.as_ref()).is_some(),
                    "registers": self.registers.iter().map(|r| serde_json::json!({
                        "id": r.id,
                        "branchId": r.branch_id,
                        "warehouseId": r.warehouse_id,
                        "name": r.name,
                        "mode": r.mode,
                        "active": r.active,
                        "version": r.version,
                    })).collect::<Vec<_>>(),
                }
            }
        })
    }
}

pub type FactsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait StartupFactsRead: Send + Sync {
    fn load<'a>(
        &'a self,
        company: &'a str,
        branch: &'a str,
        branch2: &'a str,
    ) -> FactsFuture<'a, StartupFacts>;
}

pub struct PgStartupFactsRead {
    pool: PgPool,
}

impl PgStartupFactsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool: pool.raw().clone(),
        }
    }
}

impl StartupFactsRead for PgStartupFactsRead {
    fn load<'a>(
        &'a self,
        company: &'a str,
        branch: &'a str,
        branch2: &'a str,
    ) -> FactsFuture<'a, StartupFacts> {
        let company = company.to_owned();
        let branch = branch.to_owned();
        let branch2 = branch2.to_owned();
        let pool = self.pool.clone();
        Box::pin(async move {
            let settings = sqlx::query(SETTINGS_SQL)
                .bind(&company)
                .fetch_optional(&pool)
                .await
                .map_err(|_| unavailable())?
                .map(|row| {
                    Ok::<_, BusinessError>(Settings {
                        version: row.try_get("version").map_err(|_| unavailable())?,
                        accounting_prepared_by: row
                            .try_get("accounting_prepared_by")
                            .map_err(|_| unavailable())?,
                        accounting_approved_by: row
                            .try_get("accounting_approved_by")
                            .map_err(|_| unavailable())?,
                    })
                })
                .transpose()?;
            let rows = sqlx::query(REGISTERS_SQL)
                .bind(&company)
                .bind(&branch)
                .bind(&branch2)
                .fetch_all(&pool)
                .await
                .map_err(|_| unavailable())?;
            let registers = rows
                .into_iter()
                .map(|row| {
                    Ok::<_, BusinessError>(Register {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                        warehouse_id: row.try_get("warehouse_id").map_err(|_| unavailable())?,
                        name: row.try_get("name").map_err(|_| unavailable())?,
                        mode: row.try_get("mode").map_err(|_| unavailable())?,
                        active: row.try_get("active").map_err(|_| unavailable())?,
                        version: row.try_get("version").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(StartupFacts {
                settings,
                registers,
            })
        })
    }
}

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const CONTEXT_BRANCH2: HeaderName = HeaderName::from_static("x-erpx-context-branch");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactsTarget {
    pub host: String,
    pub holding_slug: String,
}

struct FactsState {
    targets: BTreeMap<(String, String), Arc<dyn StartupFactsRead>>,
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

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("POS_STARTUP_FACTS_REQUEST_INVALID"));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("POS_STARTUP_FACTS_REQUEST_INVALID"))
        })
        .transpose()
}

fn state(targets: Vec<(FactsTarget, Arc<dyn StartupFactsRead>)>) -> Result<FactsState, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(invalid("POS_STARTUP_FACTS_CONFIG_INVALID"));
        }
    }
    if configured.is_empty() {
        return Err(invalid("POS_STARTUP_FACTS_CONFIG_INVALID"));
    }
    Ok(FactsState { targets: configured })
}

pub fn router(pool: DatabasePool, targets: Vec<FactsTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

pub fn router_for_holdings(
    targets: Vec<(FactsTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            (
                target,
                Arc::new(PgStartupFactsRead::new(pool)) as Arc<dyn StartupFactsRead>,
            )
        })
        .collect();
    Ok(Router::new()
        .route(FACTS_PATH, get(read))
        .with_state(Arc::new(state(readers)?)))
}

pub fn router_with_readers(
    targets: Vec<(FactsTarget, Arc<dyn StartupFactsRead>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(FACTS_PATH, get(read))
        .with_state(Arc::new(state(targets)?)))
}

async fn read(
    State(state): State<Arc<FactsState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) {
            return Err(invalid("POS_STARTUP_FACTS_REQUEST_INVALID"));
        }
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(|| invalid("POS_STARTUP_FACTS_REQUEST_INVALID"))?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(|| invalid("POS_STARTUP_FACTS_COMPANY_INVALID"))?
            .to_owned();
        let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|value| valid_id(value))
            .unwrap_or("")
            .to_owned();
        let branch2 = unique_header(request.headers(), CONTEXT_BRANCH2)?
            .filter(|value| valid_id(value))
            .unwrap_or("")
            .to_owned();
        Ok::<_, BusinessError>(reader.load(&company, &branch, &branch2).await?.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_matches_owner_registered_literals() {
        assert_eq!(
            SETTINGS_SQL,
            "SELECT version,accounting_prepared_by,accounting_approved_by FROM pos_settings WHERE company_id=$1"
        );
        assert_eq!(
            REGISTERS_SQL,
            "SELECT id,branch_id,warehouse_id,name,mode,active,version FROM pos_registers WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$3) ORDER BY id"
        );
        assert!(SETTINGS_SQL.contains("pos_settings"));
        assert!(REGISTERS_SQL.contains("pos_registers"));
    }

    #[test]
    fn envelope_matches_pos_startup_workspace_subset() {
        let facts = StartupFacts {
            settings: Some(Settings {
                version: 3,
                accounting_prepared_by: Some("maker".to_owned()),
                accounting_approved_by: None,
            }),
            registers: vec![Register {
                id: "reg-1".to_owned(),
                branch_id: "branch-1".to_owned(),
                warehouse_id: "warehouse-1".to_owned(),
                name: "Front".to_owned(),
                mode: "restaurant".to_owned(),
                active: true,
                version: 2,
            }],
        };
        let value = facts.value();
        assert_eq!(value["data"]["workspace"]["configured"], true);
        assert_eq!(value["data"]["workspace"]["settingsVersion"], 3);
        assert_eq!(value["data"]["workspace"]["accountingPrepared"], true);
        assert_eq!(value["data"]["workspace"]["accountingApproved"], false);
        assert_eq!(value["data"]["workspace"]["registers"][0]["id"], "reg-1");
        assert_eq!(value["data"]["workspace"]["registers"][0]["branchId"], "branch-1");
        assert_eq!(value["data"]["workspace"]["registers"][0]["warehouseId"], "warehouse-1");
        assert_eq!(value["data"]["workspace"]["registers"][0]["active"], true);
        assert_eq!(value["data"]["workspace"]["registers"][0]["version"], 2);
    }

    #[test]
    fn rejects_duplicate_context_headers() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
        assert_eq!(unique_header(&headers, CONTEXT_BRANCH).unwrap(), None);
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        let reader = Arc::new(ProbeRead) as Arc<dyn StartupFactsRead>;
        let target = || FactsTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(state(vec![(target(), reader.clone())]).is_ok());
        assert!(state(vec![(target(), reader.clone()), (target(), reader.clone())]).is_err());
        assert!(state(vec![]).is_err());
    }

    struct ProbeRead;

    impl StartupFactsRead for ProbeRead {
        fn load<'a>(
            &'a self,
            _company: &'a str,
            _branch: &'a str,
            _branch2: &'a str,
        ) -> FactsFuture<'a, StartupFacts> {
            Box::pin(async { Ok(StartupFacts { settings: None, registers: vec![] }) })
        }
    }
}
