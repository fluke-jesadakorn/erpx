//! PostgreSQL adapter and owner-local HTTP ingress for native Manufacturing
//! specification commands.
//!
//! SQL is Manufacturing-owned (`manufacturing_specifications`,
//! `manufacturing_specification_lines`, `manufacturing_specification_operations`,
//! `commerce_products`, `production_resources`) plus the shared command
//! receipt, audit and outbox tables. Cross-owner Commerce product and Planning
//! resource facts are read-only. The caller owns one serializable Holding
//! transaction; no commit is performed here.
//!
//! The HTTP section is transport facts only: a registered Host/Holding target,
//! one bounded specification path, an Idempotency-Key and business-error
//! mapping. Normal Account session resolution, selected Company/branch scope,
//! purchase admission and the serializable owner unit remain composition
//! responsibilities behind [`SpecificationHttpBackend`].
use crate::application::specification::{
    SpecificationCommandRepository, SpecificationFuture, create_result, review_result,
};
use crate::domain::bom::BomActor;
use crate::domain::specification::{
    CreatePlan, ReceiptSnapshot, ReviewPlan, SpecificationRow, idempotency_conflict,
    readback_invalid, review_conflict, source_invalid,
};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

type Error = PgTransactionError<BusinessError>;

/// PostgreSQL owner command adapter over the caller's serializable Holding
/// transaction.
pub struct PgSpecificationCommands<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgSpecificationCommands<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("MANUFACTURING_COMMAND_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(
                PersistenceError::new("MANUFACTURING_COMMAND_SERIALIZABLE_REQUIRED").into(),
            );
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("manufacturing-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn current(&mut self, actor: &BomActor, write: bool, now: i64) -> Result<bool, Error> {
        self.ready().await?;
        let permission = if write {
            "records.write"
        } else {
            "records.read"
        };
        let branch = actor.branch_id.as_ref().map(|value| value.as_str());
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 \
             AND a.revoked_at IS NULL AND iam_principal_active(a.user_id,a.company_id,$3) \
             AND iam_role_allows(a.role,a.company_id,$4) \
             AND ($5::text IS NULL OR a.scope_kind='holding' OR a.scope_node_id=$5))",
        )
        .bind(&actor.user_id)
        .bind(actor.company_id.as_str())
        .bind(now)
        .bind(permission)
        .bind(branch)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn active_product(&mut self, actor: &BomActor, product_id: &str) -> Result<bool, Error> {
        self.ready().await?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM commerce_products \
             WHERE id=$1 AND company_id=$2 AND status='active')",
        )
        .bind(product_id)
        .bind(actor.company_id.as_str())
        .fetch_one(&mut *self.connection)
        .await?)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &BomActor,
        key: &str,
        operation: &str,
        record_id: &str,
        request_hash: &str,
        result: &Value,
        now: i64,
    ) -> Result<(), Error> {
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(&actor.user_id)
        .bind(request_hash)
        .bind(record_id)
        .bind(result.to_string())
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn audit(
        &mut self,
        actor: &BomActor,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
        )
        .bind(key)
        .bind(&actor.user_id)
        .bind(&actor.user_id)
        .bind(operation)
        .bind("manufacturing_specification")
        .bind(record_id)
        .bind(version)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn outbox(
        &mut self,
        actor: &BomActor,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        state: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = serde_json::json!({
            "authority": authority,
            "id": record_id,
            "state": state,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(key)
        .bind(operation)
        .bind("manufacturing_specification")
        .bind(record_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn readback_specification(&mut self, plan: &CreatePlan) -> Result<(), Error> {
        let row = sqlx::query(
            "SELECT status,version,created_by,name,mode,output_product_id,\
             batch_quantity_milli,expected_yield_bps \
             FROM manufacturing_specifications WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.id)
        .bind(plan.actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        if row.try_get::<String, _>("status")? != "draft"
            || row.try_get::<i64, _>("version")? != 1
            || row.try_get::<String, _>("created_by")? != plan.actor.user_id
            || row.try_get::<String, _>("name")? != plan.name
            || row.try_get::<String, _>("mode")? != plan.mode
            || row.try_get::<String, _>("output_product_id")? != plan.output_product_id
            || row.try_get::<i64, _>("batch_quantity_milli")? != plan.batch_quantity_milli
            || row.try_get::<i64, _>("expected_yield_bps")? != plan.expected_yield_bps
        {
            return Err(readback_invalid().into());
        }
        let lines: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM manufacturing_specification_lines WHERE specification_id=$1",
        )
        .bind(&plan.id)
        .fetch_one(&mut *self.connection)
        .await?;
        let operations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM manufacturing_specification_operations WHERE specification_id=$1",
        )
        .bind(&plan.id)
        .fetch_one(&mut *self.connection)
        .await?;
        if lines != plan.lines.len() as i64 || operations != plan.operations.len() as i64 {
            return Err(readback_invalid().into());
        }
        Ok(())
    }
}

impl SpecificationCommandRepository for PgSpecificationCommands<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a BomActor,
        write: bool,
        now: i64,
    ) -> SpecificationFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> SpecificationFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SpecificationFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    authority_json: row.try_get("authority_json")?,
                })
            })
            .transpose()
        })
    }

    fn output_product<'a>(
        &'a mut self,
        actor: &'a BomActor,
        product_id: &'a str,
    ) -> SpecificationFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.active_product(actor, product_id).await })
    }

    fn material_product<'a>(
        &'a mut self,
        actor: &'a BomActor,
        product_id: &'a str,
    ) -> SpecificationFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.active_product(actor, product_id).await })
    }

    fn routing_resource<'a>(
        &'a mut self,
        actor: &'a BomActor,
        resource_id: &'a str,
    ) -> SpecificationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM production_resources \
                 WHERE id=$1 AND company_id=$2 AND status='active' \
                 AND ($3::text IS NULL OR branch_id=$3))",
            )
            .bind(resource_id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn specification<'a>(
        &'a mut self,
        actor: &'a BomActor,
        id: &'a str,
    ) -> SpecificationFuture<'a, Option<SpecificationRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,status,version,created_by FROM manufacturing_specifications \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(SpecificationRow {
                    id: row.try_get("id")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                })
            })
            .transpose()
        })
    }

    fn commit_create<'a>(
        &'a mut self,
        plan: CreatePlan,
    ) -> SpecificationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(&plan.key).await?;
            let changed = sqlx::query(
                "INSERT INTO manufacturing_specifications\
                 (id,company_id,name,mode,output_product_id,batch_quantity_milli,\
                 expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'draft',$8,NULL,1,$9,$9) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(plan.actor.company_id.as_str())
            .bind(&plan.name)
            .bind(&plan.mode)
            .bind(&plan.output_product_id)
            .bind(plan.batch_quantity_milli)
            .bind(plan.expected_yield_bps)
            .bind(&plan.actor.user_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(idempotency_conflict().into());
            }
            for line in &plan.lines {
                sqlx::query(
                    "INSERT INTO manufacturing_specification_lines\
                     (id,specification_id,product_id,quantity_milli,substitution_group,ordinal) \
                     VALUES($1,$2,$3,$4,$5,$6)",
                )
                .bind(&line.id)
                .bind(&plan.id)
                .bind(&line.product_id)
                .bind(line.quantity_milli)
                .bind(line.substitution_group.as_deref())
                .bind(line.ordinal)
                .execute(&mut *self.connection)
                .await?;
            }
            for operation in &plan.operations {
                sqlx::query(
                    "INSERT INTO manufacturing_specification_operations\
                     (id,specification_id,resource_id,name,setup_minutes,\
                     run_minutes_per_batch,ordinal) \
                     VALUES($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(&operation.id)
                .bind(&plan.id)
                .bind(&operation.resource_id)
                .bind(&operation.name)
                .bind(operation.setup_minutes)
                .bind(operation.run_minutes_per_batch)
                .bind(operation.ordinal)
                .execute(&mut *self.connection)
                .await?;
            }
            self.readback_specification(&plan).await?;
            let result =
                serde_json::to_value(create_result(&plan.id)).map_err(|_| readback_invalid())?;
            self.insert_receipt(
                &plan.actor,
                &plan.key,
                crate::domain::specification::CREATE_OPERATION,
                &plan.id,
                &plan.request_hash,
                &result,
                plan.now,
            )
            .await?;
            self.audit(
                &plan.actor,
                &plan.key,
                crate::domain::specification::CREATE_OPERATION,
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                &plan.actor,
                &plan.key,
                crate::domain::specification::CREATE_OPERATION,
                &plan.id,
                1,
                "created",
                plan.now,
            )
            .await?;
            Ok(result)
        })
    }

    fn commit_review<'a>(
        &'a mut self,
        plan: ReviewPlan,
    ) -> SpecificationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(&plan.key).await?;
            let changed = sqlx::query(
                "UPDATE manufacturing_specifications SET status=$1,reviewed_by=$2,\
                 version=version+1,updated_at=$3 \
                 WHERE id=$4 AND company_id=$5 AND version=$6 AND status=$7 \
                 AND ($8::boolean IS FALSE OR created_by<>$2)",
            )
            .bind(&plan.status)
            .bind(&plan.actor.user_id)
            .bind(plan.now)
            .bind(&plan.specification.id)
            .bind(plan.actor.company_id.as_str())
            .bind(plan.expected_version)
            .bind(plan.action.expected_status())
            .bind(plan.action.requires_independence())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(review_conflict().into());
            }
            let row = sqlx::query(
                "SELECT status,version,reviewed_by FROM manufacturing_specifications \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(&plan.specification.id)
            .bind(plan.actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
            if row.try_get::<String, _>("status")? != plan.status
                || row.try_get::<i64, _>("version")? != plan.expected_version + 1
                || row.try_get::<Option<String>, _>("reviewed_by")?.as_deref()
                    != Some(plan.actor.user_id.as_str())
            {
                return Err(readback_invalid().into());
            }
            let result = serde_json::to_value(review_result(
                &plan.specification.id,
                plan.action,
                plan.expected_version,
            ))
            .map_err(|_| readback_invalid())?;
            let operation = plan.action.operation();
            self.insert_receipt(
                &plan.actor,
                &plan.key,
                operation,
                &plan.specification.id,
                &plan.request_hash,
                &result,
                plan.now,
            )
            .await?;
            self.audit(
                &plan.actor,
                &plan.key,
                operation,
                &plan.specification.id,
                plan.expected_version + 1,
                plan.now,
            )
            .await?;
            self.outbox(
                &plan.actor,
                &plan.key,
                operation,
                &plan.specification.id,
                plan.expected_version + 1,
                &plan.status,
                plan.now,
            )
            .await?;
            Ok(result)
        })
    }
}

// ---------------------------------------------------------------------------
// HTTP ingress
// ---------------------------------------------------------------------------

const BODY_LIMIT: usize = 32 * 1024;

pub type SpecificationHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum SpecificationHttpCommand {
    Create {
        body: Value,
        idempotency_key: String,
    },
    Review {
        specification_id: String,
        body: Value,
        idempotency_key: String,
    },
}

pub struct SpecificationHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: SpecificationHttpCommand,
}

#[derive(Clone, Debug)]
pub struct SpecificationHttpResponse {
    /// Owner command data. The backend adds the validated current context.
    pub data: Value,
    pub context: Value,
}

impl SpecificationHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or purchase.
pub trait SpecificationHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: SpecificationHttpRequest<'a>,
    ) -> SpecificationHttpFuture<'a, SpecificationHttpResponse>;
}

pub struct SpecificationHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SpecificationHttpBackend>,
}

struct SpecificationHttpState {
    targets: BTreeMap<(String, String), SpecificationHttpTarget>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_REQUEST_INVALID",
        "Use one unambiguous Manufacturing specification command.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value.to_owned()).is_ok()
}

async fn strict_body(request: Request) -> Result<Value, BusinessError> {
    let content_type =
        unique_header(request.headers(), header::CONTENT_TYPE.as_str())?.ok_or_else(malformed)?;
    if !content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(malformed());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        return Err(malformed());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if !value.is_object() {
        return Err(malformed());
    }
    Ok(value)
}

fn strict_key(headers: &HeaderMap) -> Result<String, BusinessError> {
    let value = unique_header(headers, HeaderName::from_static("idempotency-key").as_str())?
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required for this Manufacturing command.",
            )
        })?;
    CommandKey::new(value.to_owned())?;
    Ok(value.to_owned())
}

fn ensure_no_query(request: &Request) -> Result<(), BusinessError> {
    if request.uri().query().is_some() {
        return Err(malformed());
    }
    Ok(())
}

fn ensure_review_action(body: &Value) -> Result<(), BusinessError> {
    match body.get("action").and_then(Value::as_str) {
        Some("approve" | "reject" | "retire") => Ok(()),
        _ => Err(malformed()),
    }
}

/// Owner-local router for the native specification create/review commands.
pub fn specification_router(
    targets: Vec<SpecificationHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/manufacturing/specifications",
            post(create_specification),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/manufacturing/specifications/{specification_id}",
            post(review_specification),
        )
        .with_state(Arc::new(SpecificationHttpState {
            targets: configured,
        })))
}

fn target<'a>(
    state: &'a SpecificationHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SpecificationHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST.as_str())?
        .filter(|value| !value.is_empty())
        .ok_or_else(not_found)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(not_found)
}

async fn execute(
    state: Arc<SpecificationHttpState>,
    slug: String,
    headers: HeaderMap,
    command: impl FnOnce(String) -> SpecificationHttpCommand,
) -> Response {
    let result = async move {
        let configured = target(&state, &headers, &slug)?;
        if unique_header(&headers, header::ORIGIN.as_str())?
            .is_some_and(|value| value != configured.origin)
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = strict_key(&headers)?;
        configured
            .backend
            .execute(SpecificationHttpRequest {
                host: &configured.host,
                holding_slug: &configured.holding_slug,
                headers: &headers,
                command: command(key),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) if reply.data.is_object() && reply.context.is_object() => bounded(reply.value()),
        Ok(_) => error_response(source_invalid()),
        Err(error) => error_response(error),
    }
}

async fn create_specification(
    State(state): State<Arc<SpecificationHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if let Err(error) = ensure_no_query(&request) {
        return error_response(error);
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    execute(state, slug, headers, move |idempotency_key| {
        SpecificationHttpCommand::Create {
            body,
            idempotency_key,
        }
    })
    .await
}

async fn review_specification(
    State(state): State<Arc<SpecificationHttpState>>,
    Path((slug, specification_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    if let Err(error) = ensure_no_query(&request) {
        return error_response(error);
    }
    if !valid_id(&specification_id) {
        return error_response(not_found());
    }
    let body = match strict_body(request).await {
        Ok(body) => body,
        Err(error) => return error_response(error),
    };
    if let Err(error) = ensure_review_action(&body) {
        return error_response(error);
    }
    execute(state, slug, headers, move |idempotency_key| {
        SpecificationHttpCommand::Review {
            specification_id,
            body,
            idempotency_key,
        }
    })
    .await
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

fn bounded(value: Value) -> Response {
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
