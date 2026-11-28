//! Composition backend for the native POS day-close command HTTP route.
//!
//! Session, current IAM authority, the serializable Holding unit and the
//! cross-owner day-report facts are resolved here under one transaction. The
//! owner crate supplies the planner, receipt identity and SQL only. No business
//! rule is re-implemented here and no foreign fact is synthesized from a
//! client payload.
use crate::business_http::BusinessScopeSlugs;
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_pos_server::public::commands::PosActor;
use erpx_pos_server::public::day_close::{DayReportFacts, PgPosDayClose};
use erpx_pos_server::public::day_close_http::{
    DayCloseCommand, DayCloseHttpBackend, DayCloseHttpFuture, DayCloseHttpRequest,
    DayCloseHttpResponse, DayCloseHttpTarget, DayCloseReport, DayCloseWitness, execute_day_close,
};
use erpx_pos_server::public::day_receipt::PosDayReceiptIdentity;
use erpx_pos_server::public::day_report::{
    PgPosDayReportSources, PosDayForeignFacts, PosDayOwnedSources, PosDayPmsSaleFact,
    assemble_day_report,
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, Router};
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;
/// Transparent alias of the owner crate's future type (kept private there).
type DayFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "POS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_DAY_UNAVAILABLE",
        "The native POS day-close service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct DayCloseSources;

impl HoldingWorkSourcePreparation for DayCloseSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct DayCloseConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl DayCloseConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
}

/// Asia/Bangkok business date for the supplied epoch millis, matching the
/// retained `businessDate()` clock port (UTC+07:00, no DST).
fn business_today(now_ms: i64) -> String {
    let days = (now_ms / 1000 + 7 * 3600).div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02}")
}

fn json_string(value: Option<&str>) -> Result<String, Error> {
    serde_json::to_string(&value).map_err(|_| BusinessError::from(unavailable()).into())
}

/// Foreign facts are read from the same caller-owned transaction, using the
/// retained SQL. No fact is taken from the browser command.
async fn foreign_facts(
    connection: &mut PgConnection,
    company: &str,
    branch: &str,
    date: &str,
    owned: &PosDayOwnedSources,
) -> Result<PosDayForeignFacts, Error> {
    let branch_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch')",
    )
    .bind(branch)
    .bind(company)
    .fetch_one(&mut *connection)
    .await?;

    let mut pms_by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let rows =
        sqlx::query("SELECT source_key,record_id FROM pms_sources WHERE source_key LIKE 'pos:%'")
            .fetch_all(&mut *connection)
            .await?;
    for row in rows {
        let key: String = row.try_get("source_key")?;
        let record_id: String = row.try_get("record_id")?;
        pms_by_key.entry(key).or_default().push(record_id);
    }
    let mut pms_sales = Vec::with_capacity(owned.sales.len());
    for sale in &owned.sales {
        let value: Value =
            serde_json::from_str(&sale.row_json).map_err(|_| BusinessError::from(unavailable()))?;
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| BusinessError::from(unavailable()))?;
        let key = format!("pos:{id}:{id}");
        let mut ids = pms_by_key.remove(&key).unwrap_or_default();
        ids.sort();
        pms_sales.push(PosDayPmsSaleFact {
            sale_id: id.to_owned(),
            room_charge_ids: ids,
        });
    }
    if !pms_by_key.is_empty() {
        // A stale PMS binding for a sale outside this report cannot be hashed
        // into the snapshot; fail closed instead of dropping it silently.
        return Err(BusinessError::from(unavailable()).into());
    }

    let commerce_warehouse_ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM commerce_warehouses WHERE company_id=$1 AND branch_id=$2",
    )
    .bind(company)
    .bind(branch)
    .fetch_all(&mut *connection)
    .await?;

    let journal_rows = sqlx::query(
        "SELECT j.id,j.status,j.entry_date,j.source_id,j.memo,j.version \
         FROM finance_journals j WHERE j.company_id=$1 AND j.entry_date=$2 AND j.source_type='pos' \
         AND EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=j.id \
         AND l.dimension_json::jsonb->>'branchId'=$3) ORDER BY j.id",
    )
    .bind(company)
    .bind(date)
    .bind(branch)
    .fetch_all(&mut *connection)
    .await?;
    let mut finance_journals = Vec::with_capacity(journal_rows.len());
    for row in journal_rows {
        let id: String = row.try_get("id")?;
        let status: String = row.try_get("status")?;
        let entry_date: String = row.try_get("entry_date")?;
        let source_id: Option<String> = row.try_get("source_id")?;
        let memo: Option<String> = row.try_get("memo")?;
        let version: i64 = row.try_get("version")?;
        // Preserve the retained JS property order for byte-stable hashing.
        finance_journals.push(format!(
            "{{\"id\":{},\"status\":{},\"entry_date\":{},\"source_id\":{},\"memo\":{},\"version\":{}}}",
            json_string(Some(id.as_str()))?,
            json_string(Some(status.as_str()))?,
            json_string(Some(entry_date.as_str()))?,
            json_string(source_id.as_deref())?,
            json_string(memo.as_deref())?,
            version
        ));
    }

    let settings_join = "JOIN pos_settings c ON c.company_id=j.company_id";
    let sum_sql = |account: &'static str, company_clause: bool| {
        let clause = if company_clause {
            "j.company_id=$1 AND j.entry_date=$2 AND j.status='posted' AND j.source_type='pos' AND l.dimension_json::jsonb->>'branchId'=$3"
        } else {
            "j.company_id=$1 AND j.status='posted'"
        };
        format!(
            "SELECT COALESCE(SUM(l.{}-l.{}),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id {} WHERE {} AND l.account_id=c.accounts_json::jsonb->>'{}'",
            if company_clause {
                "debit_minor"
            } else {
                "base_debit_minor"
            },
            if company_clause {
                "credit_minor"
            } else {
                "base_credit_minor"
            },
            settings_join,
            clause,
            account
        )
    };
    // The dynamic fragment is assembled only from fixed account keys and fixed
    // boolean flags; every user-supplied value stays a bound parameter.
    let cost_sql = sum_sql("cogs", true);
    let cost_minor = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(cost_sql))
        .bind(company)
        .bind(date)
        .bind(branch)
        .fetch_one(&mut *connection)
        .await?;
    let waste_sql = sum_sql("waste", true);
    let waste_minor = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(waste_sql))
        .bind(company)
        .bind(date)
        .bind(branch)
        .fetch_one(&mut *connection)
        .await?;
    let stripe_sql = sum_sql("stripe", false);
    let stripe_clearing_minor = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(stripe_sql))
        .bind(company)
        .fetch_one(&mut *connection)
        .await?;

    Ok(PosDayForeignFacts {
        company_id: company.to_owned(),
        branch_id: branch.to_owned(),
        business_date: date.to_owned(),
        branch_exists,
        pms_sales,
        commerce_warehouse_ids,
        finance_journals,
        cost_minor,
        waste_minor,
        stripe_clearing_minor,
    })
}

fn report_facts(
    report: &erpx_pos_server::public::day_report::DayReport,
) -> Result<DayCloseReport, Error> {
    let blockers = report
        .value
        .get("blockers")
        .and_then(Value::as_array)
        .ok_or_else(|| BusinessError::from(unavailable()))?
        .iter()
        .map(|item| {
            item.get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let shift_count = report
        .value
        .get("sources")
        .and_then(|value| value.get("shifts"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let company_id = report
        .value
        .get("companyId")
        .and_then(Value::as_str)
        .ok_or_else(|| BusinessError::from(unavailable()))?
        .to_owned();
    let branch_id = report
        .value
        .get("branchId")
        .and_then(Value::as_str)
        .ok_or_else(|| BusinessError::from(unavailable()))?
        .to_owned();
    let business_date = report
        .value
        .get("businessDate")
        .and_then(Value::as_str)
        .ok_or_else(|| BusinessError::from(unavailable()))?
        .to_owned();
    Ok(DayCloseReport {
        facts: DayReportFacts {
            company_id,
            branch_id,
            business_date,
            digest: report.digest.clone(),
            blockers,
            shift_count,
        },
        currency: report
            .value
            .get("currency")
            .and_then(Value::as_str)
            .ok_or_else(|| BusinessError::from(unavailable()))?
            .to_owned(),
        snapshot_json: serde_json::to_string(&report.value)
            .map_err(|_| BusinessError::from(unavailable()))?,
    })
}

/// Day-close witness bound to the current authority and the pre-read report.
struct CurrentDayWitness<'a> {
    current: &'a AuthorizedWorkContext,
    report: DayCloseReport,
    today: String,
}

impl DayCloseWitness for CurrentDayWitness<'_> {
    type Error = PgTransactionError<BusinessError>;

    fn report<'a>(
        &'a self,
        company_id: &'a str,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> DayFuture<'a, DayCloseReport, Error> {
        Box::pin(async move {
            let facts = &self.report.facts;
            if facts.company_id != company_id
                || facts.branch_id != branch_id
                || facts.business_date != business_date
            {
                return Err(BusinessError::from(unavailable()).into());
            }
            Ok(self.report.clone())
        })
    }

    fn same_person<'a>(
        &'a self,
        left: &'a str,
        right: &'a str,
    ) -> DayFuture<'a, bool, Error> {
        Box::pin(async move { Ok(left == right || self.current.same_person_user_raw(right)) })
    }

    fn new_id<'a>(&'a self) -> DayFuture<'a, String, Error> {
        Box::pin(async { Ok(uuid::Uuid::new_v4().to_string()) })
    }

    fn today<'a>(&'a self) -> DayFuture<'a, String, Error> {
        Box::pin(async move { Ok(self.today.clone()) })
    }
}

pub struct DayCloseService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: DayCloseConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> DayCloseService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: DayCloseConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        &self.configuration.company
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "pos.reports".into(),
            module: Some("pos".into()),
            destination: "pos".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        }
    }

    pub async fn execute<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: String,
        command: DayCloseCommand,
    ) -> Result<DayCloseHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                DayCloseSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pos.reports")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let actor = PosActor {
                            user_id: current.user_id().clone(),
                            holding_id: current.holding_id().clone(),
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                            assignment_id: selected.assignment().id.clone(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                        };
                        let raw_input = match &command {
                            DayCloseCommand::Prepare { branch_id, business_date } => {
                                serde_json::json!({"branchId": branch_id, "businessDate": business_date}).to_string()
                            }
                            DayCloseCommand::Confirm { id, expected_version } => {
                                serde_json::json!({"id": id, "expectedVersion": expected_version}).to_string()
                            }
                        };
                        let identity = PosDayReceiptIdentity::parse(
                            &actor,
                            command.operation(),
                            &key,
                            &raw_input,
                        )?;
                        let now = clock.now();
                        let branch_and_date = match &command {
                            DayCloseCommand::Prepare { branch_id, business_date } => {
                                (branch_id.clone(), business_date.clone())
                            }
                            DayCloseCommand::Confirm { id, .. } => {
                                let row = sqlx::query(
                                    "SELECT branch_id,business_date FROM pos_day_closes WHERE company_id=$1 AND id=$2",
                                )
                                .bind(company.as_str())
                                .bind(id)
                                .fetch_optional(&mut *connection)
                                .await?
                                .ok_or_else(|| BusinessError::new(
                                    FailureKind::NotFound,
                                    "POS_NOT_FOUND",
                                    "Day close not found.",
                                ))?;
                                (row.try_get("branch_id")?, row.try_get("business_date")?)
                            }
                        };
                        let (branch_id, business_date) = branch_and_date;
                        {
                            let mut lock = PgPosDayClose::new(connection);
                            lock.lock_company(company.as_str()).await?;
                        }
                        let owned = PgPosDayReportSources::new(connection)
                            .load(company.as_str(), &branch_id, &business_date)
                            .await?;
                        let foreign = foreign_facts(
                            connection,
                            company.as_str(),
                            &branch_id,
                            &business_date,
                            &owned,
                        )
                        .await?;
                        let report = assemble_day_report(
                            company.as_str(),
                            &branch_id,
                            &business_date,
                            owned,
                            foreign,
                            |value| hex::encode(Sha256::digest(value.as_bytes())),
                        )?;
                        let report = report_facts(&report)?;
                        let witness = CurrentDayWitness {
                            current,
                            report,
                            today: business_today(now.get()),
                        };
                        let mut store = PgPosDayClose::new(connection);
                        let (id, state) =
                            execute_day_close(&actor, &identity, &command, &mut store, &witness, now)
                                .await?;
                        drop(store);
                        Ok(DayCloseHttpResponse {
                            data: serde_json::json!({"id": id, "state": state}),
                            context: serde_json::json!({
                                "userId": current.user_id().as_str(),
                                "companyId": workspace.company_id.as_str(),
                            }),
                        })
                    })
                },
            )
            .await
    }
}

pub struct DayCloseBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<DayCloseService<C>>,
}

impl<C: Clock + Send + Sync + 'static> DayCloseBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<DayCloseService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> DayCloseHttpBackend for DayCloseBackend<C> {
    fn execute<'a>(
        &'a self,
        request: DayCloseHttpRequest<'a>,
    ) -> DayCloseHttpFuture<'a, DayCloseHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .execute(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key.as_str().to_owned(),
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

/// Build the owner router bound to a backend; the lead mounts the result.
pub fn pos_day_close_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn DayCloseHttpBackend>,
) -> Result<Router, BusinessError> {
    erpx_pos_server::public::day_close_http::pos_day_close_router(vec![DayCloseHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
