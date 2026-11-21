//! Native Support customer ticket-portal owner facts.
//!
//! Every query runs on the caller's already-authorized Holding transaction. The
//! caller supplies current staff/customer authority, purchase/release purpose,
//! replay lookup and command receipt/audit/outbox. This adapter never opens or
//! commits a transaction, retries or calls a provider.
use crate::adapters::support_ticket_read::{SupportReadScope, read_predicate};
use crate::domain::ticket_portal::{
    AcknowledgementInsert, BindingStatus, PortalActor, PortalBindingRow, PortalCustomer,
    PortalPlan, PortalProjection, PortalProjectionItem, PortalResponse, PortalRights, PortalTicket,
    PortalWorkspace, ResponseInsert, binding_view, can_prepare, digest,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn not_found() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_PORTAL_NOT_FOUND",
        "Support customer access is unavailable.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPORT_PORTAL_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("SUPPORT_PORTAL_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Serialize Support ticket/portal writers before their first source snapshot.
pub async fn lock_ticket_portal_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE support_tickets,support_ticket_events,support_ticket_portal_bindings,\
         support_ticket_portal_events,support_ticket_portal_identities,\
         support_ticket_portal_reads,support_ticket_portal_responses,\
         support_ticket_portal_acknowledgements,command_receipts,audit_events,outbox_events \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

pub async fn portal_installed(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL \
         AND to_regclass('support_ticket_portal_acknowledgements') IS NOT NULL",
    )
    .fetch_one(&mut *c)
    .await?)
}

/// Identity retained with every binding. Only the reviewed summary is public.
fn identity_sql(alias: &str) -> String {
    format!(
        "jsonb_build_object('companyId',{alias}.company_id,'branchId',{alias}.branch_id,\
         'departmentId',{alias}.department_id,'sourceKind',{alias}.source_kind,\
         'orderId',{alias}.order_id,'customerId',{alias}.customer_id,\
         'sourceRecord',{alias}.source_record_json)"
    )
}

fn valid_bindings() -> String {
    format!(
        "SELECT binding.* FROM support_ticket_portal_bindings binding \
         JOIN support_tickets ticket ON ticket.id=binding.ticket_id \
           AND ticket.company_id=binding.company_id AND ticket.source_kind<>'field_service' \
           AND binding.source_identity={} \
         JOIN command_receipts birth ON birth.command_id=binding.prepare_command_id \
           AND birth.operation='support.ticket-portal.prepare' \
           AND birth.record_id=binding.id AND birth.actor_id=binding.prepared_by \
           AND birth.authority_json::jsonb->>'companyId'=binding.company_id \
           AND birth.result_json::jsonb->>'id'=binding.id \
           AND birth.result_json::jsonb->>'status'='draft' \
           AND birth.result_json::jsonb->>'version'='1'",
        identity_sql("ticket")
    )
}

pub async fn ticket_image(c: &mut PgConnection, ticket_id: &str) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(ticket)::text FROM support_tickets ticket WHERE ticket.id=$1",
    )
    .bind(ticket_id)
    .fetch_optional(&mut *c)
    .await?)
}

/// Exact binding/ticket/event/identity/read/response/acknowledgement image used
/// by the TypeScript source-changed guard.
pub async fn binding_image(c: &mut PgConnection, ids: &[String]) -> Result<String, Error> {
    if ids.is_empty() {
        return Ok("[]".into());
    }
    let placeholders = (1..=ids.len())
        .map(|index| format!("${index}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "WITH selected AS(SELECT * FROM support_ticket_portal_bindings WHERE id IN ({placeholders}))
         SELECT jsonb_build_array(
           (SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.id) FROM selected binding),
           (SELECT jsonb_agg(to_jsonb(ticket) ORDER BY ticket.id) FROM support_tickets ticket
             WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.ticket_id=ticket.id)),
           (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_ticket_portal_events event
             WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=event.binding_id)),
           (SELECT jsonb_agg(to_jsonb(identity) ORDER BY identity.binding_id)
             FROM support_ticket_portal_identities identity
             WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=identity.binding_id)),
           (SELECT jsonb_agg(to_jsonb(read) ORDER BY read.id) FROM support_ticket_portal_reads read
             WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=read.binding_id)),
           (SELECT jsonb_agg(to_jsonb(response) ORDER BY response.id)
             FROM support_ticket_portal_responses response
             WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=response.binding_id)),
           (SELECT jsonb_agg(to_jsonb(ack) ORDER BY ack.id)
             FROM support_ticket_portal_acknowledgements ack
             WHERE EXISTS(SELECT 1 FROM selected binding WHERE binding.id=ack.binding_id))
         )::text"
    );
    let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql));
    for id in ids {
        query = query.bind(id);
    }
    Ok(query.fetch_one(&mut *c).await?)
}

fn row_to_binding(row: &sqlx::postgres::PgRow) -> Result<PortalBindingRow, Error> {
    let status: String = row.try_get("status")?;
    Ok(PortalBindingRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        ticket_id: row.try_get("ticket_id")?,
        ticket_version: row.try_get("ticket_version")?,
        customer_name: row.try_get("customer_name")?,
        email: row.try_get("email")?,
        subject: row.try_get("subject")?,
        summary: row.try_get("summary")?,
        expires_at: row.try_get("expires_at")?,
        status: BindingStatus::parse(&status).ok_or_else(|| {
            conflict(
                "SUPPORT_PORTAL_SOURCE_CHANGED",
                "The verified customer access changed before completion.",
            )
        })?,
        version: row.try_get("version")?,
        prepared_by: row.try_get("prepared_by")?,
    })
}

async fn raw_binding(c: &mut PgConnection, id: &str) -> Result<Option<PortalBindingRow>, Error> {
    let sql = format!(
        "SELECT * FROM ({}) binding WHERE binding.id=$1",
        valid_bindings()
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut *c)
        .await?;
    row.as_ref().map(row_to_binding).transpose()
}

/// Staff binding lookup. The ticket must remain visible under the selected
/// Company/scope predicate; otherwise the binding is indistinguishable from
/// absent. Field Service tickets never enter this port.
pub async fn binding_for_staff(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<PortalBindingRow>, Error> {
    serializable(c).await?;
    let Some(binding) = raw_binding(c, id).await? else {
        return Ok(None);
    };
    if binding.company_id != scope.company_id.as_str() {
        return Ok(None);
    }
    let permission = read_predicate("$4", "$5", "$6");
    let sql = format!(
        "SELECT 1 FROM support_tickets t \
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) \
           AND t.source_kind<>'field_service' AND {permission}"
    );
    let visible = sqlx::query_scalar::<_, i32>(sqlx::AssertSqlSafe(sql))
        .bind(&binding.ticket_id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_optional(&mut *c)
        .await?
        .is_some();
    Ok(visible.then_some(binding))
}

pub async fn ticket_for_staff(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<PortalTicket>, Error> {
    serializable(c).await?;
    let permission = read_predicate("$5", "$6", "$7");
    let sql = format!(
        "SELECT t.id,t.version,t.title,t.status,t.source_kind FROM support_tickets t \
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) \
           AND ($4::text IS NULL OR t.department_id=$4) AND {permission}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_optional(&mut *c)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let source_kind: String = row.try_get("source_kind")?;
    if source_kind == "field_service" {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SOURCE_WORKSPACE_REQUIRED",
            "Field Service owns access to this ticket.",
        )
        .into());
    }
    Ok(Some(PortalTicket {
        id: row.try_get("id")?,
        version: row.try_get("version")?,
        title: row.try_get("title")?,
        status: row.try_get("status")?,
        source_kind,
    }))
}

pub async fn current_response(
    c: &mut PgConnection,
    binding_id: &str,
) -> Result<Option<PortalResponse>, Error> {
    let row = sqlx::query(
        "SELECT id,response_version,message,created_at FROM support_ticket_portal_responses \
         WHERE binding_id=$1 ORDER BY response_version DESC LIMIT 1",
    )
    .bind(binding_id)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, Error>(PortalResponse {
            id: row.try_get("id")?,
            version: row.try_get("response_version")?,
            message: row.try_get("message")?,
            published_at: row.try_get("created_at")?,
        })
    })
    .transpose()
}

pub async fn acknowledged_at(
    c: &mut PgConnection,
    binding_id: &str,
    response_id: Option<&str>,
) -> Result<Option<i64>, Error> {
    let Some(response_id) = response_id else {
        return Ok(None);
    };
    Ok(sqlx::query_scalar(
        "SELECT created_at FROM support_ticket_portal_acknowledgements \
         WHERE binding_id=$1 AND response_id=$2",
    )
    .bind(binding_id)
    .bind(response_id)
    .fetch_optional(&mut *c)
    .await?)
}

/// Staff workspace. `rights` is the caller's already-verified purchase result.
pub async fn staff_workspace(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    actor: &PortalActor,
    ticket_id: Option<&str>,
    binding_id: Option<&str>,
    rights: &PortalRights,
    now: i64,
) -> Result<PortalWorkspace, Error> {
    serializable(c).await?;
    let empty = PortalWorkspace {
        company_id: actor.company_id.clone(),
        schema_available: false,
        available: false,
        context_key: None,
        can_prepare: false,
        ticket: None,
        bindings: vec![],
    };
    if !portal_installed(c).await? {
        return Ok(empty);
    }
    let selected = if let Some(binding_id) = binding_id {
        binding_for_staff(c, scope, binding_id).await?
    } else {
        None
    };
    let ticket_id = selected
        .as_ref()
        .map(|binding| binding.ticket_id.clone())
        .or_else(|| ticket_id.map(str::to_owned));
    let ticket = if let Some(ticket_id) = ticket_id.as_deref() {
        ticket_for_staff(c, scope, ticket_id).await?
    } else {
        None
    };
    let sql = format!(
        "SELECT binding.* FROM ({}) binding JOIN support_tickets ticket ON ticket.id=binding.ticket_id \
         WHERE binding.company_id=$1 AND ($2::text IS NULL OR ticket.branch_id=$2) \
           AND ($3::text IS NULL OR ticket.department_id=$3) \
           AND ($4::text IS NULL OR binding.ticket_id=$4) \
           AND ($5::text IS NULL OR binding.id=$5) \
         ORDER BY binding.updated_at DESC,binding.id LIMIT 100",
        valid_bindings()
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(actor.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(ticket_id.as_deref())
        .bind(binding_id)
        .fetch_all(&mut *c)
        .await?;
    let mut visible = vec![];
    for row in rows {
        let binding = row_to_binding(&row)?;
        if ticket_for_staff(c, scope, &binding.ticket_id)
            .await?
            .is_some()
        {
            visible.push(binding);
        }
    }
    let ids = visible
        .iter()
        .map(|binding| binding.id.clone())
        .collect::<Vec<_>>();
    let image = binding_image(c, &ids).await?;
    let ticket_image = match ticket_id.as_deref() {
        Some(ticket_id) => ticket_image(c, ticket_id).await?,
        None => None,
    };
    let mut bindings = Vec::with_capacity(visible.len());
    for binding in visible {
        let response = current_response(c, &binding.id).await?;
        let acknowledged = acknowledged_at(
            c,
            &binding.id,
            response.as_ref().map(|value| value.id.as_str()),
        )
        .await?;
        bindings.push(binding_view(
            actor,
            &binding,
            rights,
            response,
            acknowledged,
            now,
        ));
    }
    Ok(PortalWorkspace {
        company_id: actor.company_id.clone(),
        schema_available: true,
        available: true,
        context_key: Some(digest(&serde_json::json!({
            "assignmentId": actor.assignment_id,
            "companyId": actor.company_id,
            "images": image,
            "ticketImage": ticket_image,
        }))),
        can_prepare: can_prepare(actor, rights, ticket.as_ref()),
        ticket,
        bindings,
    })
}

/// Customer projection over already-verified email bindings. `company_allowed`
/// is the composition's current purchase/release decision for each Company; no
/// client-supplied Company scope is accepted. The read records the exact
/// customer identity and read event used by the TS projection.
pub async fn record_customer_projection<F>(
    c: &mut PgConnection,
    customer: &PortalCustomer,
    now: i64,
    company_allowed: F,
) -> Result<PortalProjection, Error>
where
    F: Fn(&str) -> bool,
{
    serializable(c).await?;
    let context_key = customer.identity_key();
    let empty = PortalProjection {
        schema_available: false,
        context_key: context_key.clone(),
        items: vec![],
    };
    if !portal_installed(c).await? {
        return Ok(empty);
    }
    let sql = format!(
        "SELECT binding.* FROM ({}) binding \
         WHERE binding.email=$1 AND binding.status='approved' AND binding.expires_at>$2 \
           AND NOT EXISTS(SELECT 1 FROM support_ticket_portal_identities identity \
             WHERE identity.binding_id=binding.id \
               AND (identity.user_id<>$3 OR identity.email<>binding.email)) \
         ORDER BY binding.updated_at DESC,binding.id",
        valid_bindings()
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(customer.normalized_email())
        .bind(now)
        .bind(customer.user_id.as_str())
        .fetch_all(&mut *c)
        .await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let binding = row_to_binding(&row)?;
        if !company_allowed(&binding.company_id) {
            continue;
        }
        sqlx::query(
            "INSERT INTO support_ticket_portal_identities(binding_id,company_id,user_id,email,created_at) \
             VALUES($1,$2,$3,$4,$5) ON CONFLICT(binding_id) DO NOTHING",
        )
        .bind(&binding.id)
        .bind(&binding.company_id)
        .bind(customer.user_id.as_str())
        .bind(customer.normalized_email())
        .bind(now)
        .execute(&mut *c)
        .await?;
        let response = current_response(c, &binding.id).await?;
        let acknowledged = acknowledged_at(
            c,
            &binding.id,
            response.as_ref().map(|response| response.id.as_str()),
        )
        .await?;
        items.push(PortalProjectionItem {
            id: binding.id.clone(),
            ticket_id: binding.ticket_id.clone(),
            subject: binding.subject.clone(),
            summary: binding.summary.clone(),
            customer_name: binding.customer_name.clone(),
            version: binding.version,
            expires_at: binding.expires_at,
            response,
            acknowledged_at: acknowledged,
        });
        sqlx::query(
            "INSERT INTO support_ticket_portal_reads(id,binding_id,company_id,user_id,binding_version,created_at) \
             VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(format!("support-portal-read-{}", digest(&serde_json::json!({
            "bindingId": binding.id,
            "userId": customer.user_id,
            "version": binding.version,
            "at": now,
        }))))
        .bind(&binding.id)
        .bind(&binding.company_id)
        .bind(customer.user_id.as_str())
        .bind(binding.version)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    Ok(PortalProjection {
        schema_available: true,
        context_key,
        items,
    })
}

/// Approved, unexpired binding for one verified customer and exact email.
pub async fn customer_binding(
    c: &mut PgConnection,
    customer: &PortalCustomer,
    id: &str,
    now: i64,
) -> Result<PortalBindingRow, Error> {
    serializable(c).await?;
    let sql = format!(
        "SELECT binding.* FROM ({}) binding \
         JOIN support_ticket_portal_identities identity \
           ON identity.binding_id=binding.id AND identity.user_id=$1 \
           AND identity.email=binding.email \
         WHERE binding.id=$2 AND binding.email=$3 AND binding.status='approved' \
           AND binding.expires_at>$4",
        valid_bindings()
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(customer.user_id.as_str())
        .bind(id)
        .bind(customer.normalized_email())
        .bind(now)
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(not_found)?;
    row_to_binding(&row)
}

/// Immutable decision replay proof. A valid request hash alone cannot recover an
/// orphan receipt; the original event and receipt result must still agree.
pub async fn decision_replay_proof(
    c: &mut PgConnection,
    actor: &PortalActor,
    binding_id: &str,
    key: &str,
    action: &str,
    result: &serde_json::Value,
) -> Result<(), Error> {
    let result_text = serde_json::to_string(result)
        .map_err(|_| conflict("SUPPORT_PORTAL_SOURCE_CHANGED", "Invalid replay result."))?;
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM support_ticket_portal_events event \
         JOIN command_receipts receipt ON receipt.command_id=event.command_id \
           AND receipt.record_id=event.binding_id AND receipt.actor_id=event.actor_id \
           AND receipt.operation='support.ticket-portal.'||event.action \
           AND receipt.authority_json::jsonb->>'companyId'=event.company_id \
         WHERE event.command_id=$1 AND event.binding_id=$2 AND event.company_id=$3 \
           AND event.actor_id=$4 AND event.action=$5 \
           AND receipt.result_json::jsonb=$6::jsonb \
           AND receipt.result_json::jsonb=jsonb_build_object(\
             'id',event.binding_id,\
             'status',CASE event.action WHEN 'prepare' THEN 'draft' WHEN 'approve' THEN 'approved' \
               WHEN 'reject' THEN 'rejected' ELSE 'revoked' END,\
             'version',event.version)",
    )
    .bind(key)
    .bind(binding_id)
    .bind(actor.company_id.as_str())
    .bind(actor.user_id.as_str())
    .bind(action)
    .bind(result_text)
    .fetch_optional(&mut *c)
    .await?;
    if found.is_none() {
        return Err(conflict(
            "SUPPORT_PORTAL_SOURCE_CHANGED",
            "The verified customer access changed before completion.",
        ));
    }
    Ok(())
}

pub async fn response_replay_proof(
    c: &mut PgConnection,
    actor: &PortalActor,
    binding_id: &str,
    key: &str,
    result: &serde_json::Value,
) -> Result<(), Error> {
    let result_text = serde_json::to_string(result)
        .map_err(|_| conflict("SUPPORT_PORTAL_SOURCE_CHANGED", "Invalid replay result."))?;
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM support_ticket_portal_responses response \
         JOIN command_receipts receipt ON receipt.command_id=response.command_id \
           AND receipt.record_id=response.binding_id AND receipt.actor_id=response.actor_id \
           AND receipt.operation='support.ticket-portal.respond' \
           AND receipt.authority_json::jsonb->>'companyId'=response.company_id \
         WHERE response.command_id=$1 AND response.binding_id=$2 AND response.company_id=$3 \
           AND response.actor_id=$4 AND receipt.result_json::jsonb=$5::jsonb \
           AND receipt.result_json::jsonb=jsonb_build_object(\
             'id',response.id,'bindingId',response.binding_id,'version',response.response_version,\
             'status','published','message',response.message,'publishedAt',response.created_at)",
    )
    .bind(key)
    .bind(binding_id)
    .bind(actor.company_id.as_str())
    .bind(actor.user_id.as_str())
    .bind(result_text)
    .fetch_optional(&mut *c)
    .await?;
    if found.is_none() {
        return Err(conflict(
            "SUPPORT_PORTAL_SOURCE_CHANGED",
            "The verified customer access changed before completion.",
        ));
    }
    Ok(())
}

/// Apply owner rows only. Command receipt/audit/outbox remain the composition's
/// responsibility on this same connection.
pub async fn apply_plan(
    c: &mut PgConnection,
    _actor: &PortalActor,
    plan: &PortalPlan,
) -> Result<(), Error> {
    serializable(c).await?;
    match plan {
        PortalPlan::Prepare { binding, event, .. } => {
            let sql = format!(
                "INSERT INTO support_ticket_portal_bindings(\
                   id,company_id,ticket_id,ticket_version,source_identity,customer_name,email,\
                   subject,summary,expires_at,status,version,prepared_by,prepare_command_id,\
                   created_at,updated_at) \
                 SELECT $1,company_id,id,version,{},$2,$3,$4,$5,$6,'draft',1,$7,$8,$9,$9 \
                 FROM support_tickets ticket \
                 WHERE ticket.id=$10 AND ticket.company_id=$11 AND ticket.version=$12 \
                   AND ticket.source_kind<>'field_service'",
                identity_sql("ticket")
            );
            let inserted = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&binding.id)
                .bind(&binding.customer_name)
                .bind(&binding.email)
                .bind(&binding.subject)
                .bind(&binding.summary)
                .bind(binding.expires_at)
                .bind(&binding.prepared_by)
                .bind(&binding.command_id)
                .bind(binding.created_at)
                .bind(&binding.ticket_id)
                .bind(&binding.company_id)
                .bind(binding.ticket_version)
                .execute(&mut *c)
                .await?
                .rows_affected();
            if inserted != 1 {
                return Err(conflict(
                    "SUPPORT_PORTAL_TICKET_CHANGED",
                    "Reload the actual ticket before preparing access.",
                ));
            }
            insert_event(c, event).await?;
        }
        PortalPlan::Decide {
            status,
            version,
            event,
            ..
        } => {
            let updated = sqlx::query(
                "UPDATE support_ticket_portal_bindings \
                 SET status=$1,version=version+1,updated_at=$2 \
                 WHERE id=$3 AND company_id=$4 AND version=$5",
            )
            .bind(status.as_str())
            .bind(event.created_at)
            .bind(&event.binding_id)
            .bind(&event.company_id)
            .bind(version - 1)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(conflict(
                    "SUPPORT_PORTAL_VERSION_CHANGED",
                    "Reload the current customer access.",
                ));
            }
            insert_event(c, event).await?;
        }
        PortalPlan::Respond { response, .. } => {
            insert_response(c, response).await?;
        }
        PortalPlan::Acknowledge {
            acknowledgement, ..
        } => {
            insert_acknowledgement(c, acknowledgement).await?;
        }
    }
    Ok(())
}

async fn insert_event(
    c: &mut PgConnection,
    event: &crate::domain::ticket_portal::EventInsert,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO support_ticket_portal_events(\
           id,binding_id,company_id,version,action,actor_id,command_id,reason,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(&event.id)
    .bind(&event.binding_id)
    .bind(&event.company_id)
    .bind(event.version)
    .bind(event.action)
    .bind(&event.actor_id)
    .bind(&event.command_id)
    .bind(&event.reason)
    .bind(event.created_at)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn insert_response(c: &mut PgConnection, response: &ResponseInsert) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO support_ticket_portal_responses(\
           id,company_id,binding_id,binding_version,response_version,message,reason,actor_id,\
           command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(&response.id)
    .bind(&response.company_id)
    .bind(&response.binding_id)
    .bind(response.binding_version)
    .bind(response.response_version)
    .bind(&response.message)
    .bind(&response.reason)
    .bind(&response.actor_id)
    .bind(&response.command_id)
    .bind(response.created_at)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn insert_acknowledgement(
    c: &mut PgConnection,
    acknowledgment: &AcknowledgementInsert,
) -> Result<(), Error> {
    let duplicate: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM support_ticket_portal_acknowledgements WHERE response_id=$1",
    )
    .bind(&acknowledgment.response_id)
    .fetch_optional(&mut *c)
    .await?;
    if duplicate.is_some() {
        return Err(conflict(
            "SUPPORT_PORTAL_ALREADY_ACKNOWLEDGED",
            "This response has already been acknowledged.",
        ));
    }
    sqlx::query(
        "INSERT INTO support_ticket_portal_acknowledgements(\
           id,company_id,binding_id,binding_version,response_id,response_version,user_id,email,\
           request_key,request_hash,note,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(&acknowledgment.id)
    .bind(&acknowledgment.company_id)
    .bind(&acknowledgment.binding_id)
    .bind(acknowledgment.binding_version)
    .bind(&acknowledgment.response_id)
    .bind(acknowledgment.response_version)
    .bind(&acknowledgment.user_id)
    .bind(&acknowledgment.email)
    .bind(&acknowledgment.request_key)
    .bind(&acknowledgment.request_hash)
    .bind(&acknowledgment.note)
    .bind(acknowledgment.created_at)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Verified-customer acknowledgement effects: immutable row plus the exact
/// audit/outbox event. No command receipt is created for an external customer.
pub async fn apply_customer_acknowledgement(
    c: &mut PgConnection,
    plan: &PortalPlan,
) -> Result<(), Error> {
    let PortalPlan::Acknowledge {
        acknowledgement,
        binding_id,
        response_id,
        user_id,
        company_id,
        email,
        ..
    } = plan
    else {
        return Err(BusinessError::invalid(
            "SUPPORT_PORTAL_INPUT_INVALID",
            "Invalid Support Portal acknowledgement.",
        )
        .into());
    };
    serializable(c).await?;
    insert_acknowledgement(c, acknowledgement).await?;
    let audit_id = format!("support-portal-ack-audit-{}", acknowledgement.request_key);
    sqlx::query(
        "INSERT INTO audit_events(\
           id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
           purpose,created_at,authority_json) \
         VALUES($1,$2,$3,'external-customer','support.ticket-portal.acknowledge',\
           'support_ticket_portal',$4,$5,\
           'Verified customer acknowledged an exact published Support response',$6,$7)",
    )
    .bind(&audit_id)
    .bind(&acknowledgement.request_key)
    .bind(user_id)
    .bind(binding_id)
    .bind(acknowledgement.response_version)
    .bind(acknowledgement.created_at)
    .bind(
        serde_json::to_string(&serde_json::json!({
            "kind": "external-customer",
            "bindingId": binding_id,
            "email": email,
        }))
        .unwrap_or_default(),
    )
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "INSERT INTO outbox_events(\
           id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,\
           created_at) VALUES($1,$2,'support.ticket-portal.acknowledge','support_ticket_portal',\
           $3,$4,$5,$6)",
    )
    .bind(format!(
        "support-portal-ack-event-{}",
        acknowledgement.request_key
    ))
    .bind(&acknowledgement.request_key)
    .bind(binding_id)
    .bind(acknowledgement.response_version)
    .bind(
        serde_json::to_string(&serde_json::json!({
            "bindingId": binding_id,
            "responseId": response_id,
            "userId": user_id,
            "companyId": company_id,
        }))
        .unwrap_or_default(),
    )
    .bind(acknowledgement.created_at)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Prior acknowledgement by exact request key. The composite caller compares
/// the binding, user and request hash before returning the retained result.
pub async fn acknowledgement_prior(
    c: &mut PgConnection,
    key: &str,
) -> Result<
    Option<(
        String,
        String,
        String,
        crate::domain::ticket_portal::AcknowledgementResult,
    )>,
    Error,
> {
    let row = sqlx::query(
        "SELECT id,binding_id,user_id,request_hash,response_id,response_version,created_at \
         FROM support_ticket_portal_acknowledgements WHERE request_key=$1",
    )
    .bind(key)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        let binding_id: String = row.try_get("binding_id")?;
        let user_id: String = row.try_get("user_id")?;
        let request_hash: String = row.try_get("request_hash")?;
        let response_id: String = row.try_get("response_id")?;
        let response_version: i64 = row.try_get("response_version")?;
        let created_at: i64 = row.try_get("created_at")?;
        Ok::<_, Error>((
            binding_id.clone(),
            user_id,
            request_hash,
            crate::domain::ticket_portal::AcknowledgementResult {
                id: row.try_get("id")?,
                binding_id,
                response_id,
                response_version,
                status: "acknowledged",
                created_at,
            },
        ))
    })
    .transpose()
}

pub async fn guard_customer_binding(
    c: &mut PgConnection,
    customer: &PortalCustomer,
    binding_id: &str,
    response_id: &str,
    now: i64,
) -> Result<(), Error> {
    let _binding = customer_binding(c, customer, binding_id, now).await?;
    let response = current_response(c, binding_id)
        .await?
        .ok_or_else(not_found)?;
    if response.id != response_id {
        return Err(conflict(
            "SUPPORT_PORTAL_SOURCE_CHANGED",
            "The verified customer access changed before completion.",
        ));
    }
    Ok(())
}

/// Guard that a retained staff binding and ticket image did not change.
pub async fn guard_binding_image(
    c: &mut PgConnection,
    binding_ids: &[String],
    ticket_id: Option<&str>,
    expected: &(String, Option<String>),
) -> Result<(), Error> {
    let current = (
        binding_image(c, binding_ids).await?,
        match ticket_id {
            Some(ticket_id) => ticket_image(c, ticket_id).await?,
            None => None,
        },
    );
    if current != *expected {
        return Err(conflict(
            "SUPPORT_PORTAL_SOURCE_CHANGED",
            "The original ticket, verified customer access or public response changed before completion.",
        ));
    }
    Ok(())
}
