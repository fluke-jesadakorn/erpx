//! Field visit evidence operation. Native source/Records/current-IAM reads and
//! the Runtime-owned access audit share one selected Holding transaction.
use crate::{
    field_collaboration_command::FieldCollaborationTarget,
    field_service_records_source::FieldCollaborationSources,
};
use base64::Engine;
use erpx_server_field_service::public::executive::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::{executive_access::*, postgres::PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock};
use serde::Serialize;
use serde_json::json;
type Error = PgTransactionError<BusinessError>;
use erpx_server_field_service::public::executive_evidence::*;
pub async fn read_selected_field_evidence<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    request: FieldEvidenceRequest,
    action: EvidenceAction,
) -> Result<crate::field_executive_read::FieldExecutiveReadResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let company = target.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut w = workspace()?;
                if w.company_id != company {
                    return Err(missing());
                }
                w.mutation = false;
                Ok(w)
            },
            WorkAuthorizationRequest {
                permission: "records.read".into(),
                module: Some("field_service".into()),
                destination: "executive/field-service".into(),
                api: true,
            },
            FieldCollaborationSources,
            move |c, current| {
                let target = target.clone();
                let request = request.clone();
                Box::pin(async move { read(c, current, &target, request, action).await })
            },
        )
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error)
                if matches!(error.code, "PERMISSION_REQUIRED" | "MODULE_FORBIDDEN") =>
            {
                BusinessError::new(
                    erpx_server_shared::public::FailureKind::Forbidden,
                    "EVIDENCE_FORBIDDEN",
                    "This source record is not available in the current role or scope.",
                )
                .into()
            }
            other => other,
        })
}
async fn read<C: Clock + Send + Sync>(
    c: &mut sqlx::PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    mut input: FieldEvidenceRequest,
    action: EvidenceAction,
) -> Result<crate::field_executive_read::FieldExecutiveReadResponse, Error> {
    if input.metric != METRIC {
        return Err(invalid().into());
    }
    input.query = trim(&input.query).into();
    if input.query.encode_utf16().count() > 100
        || input.filters.currency.len() != 3
        || !input
            .filters
            .currency
            .bytes()
            .all(|b| b.is_ascii_uppercase())
    {
        return Err(invalid().into());
    }
    let from = date(&input.filters.from)?;
    let to = date(&input.filters.to)?;
    if to < from || to - from > 366 * 86_400_000 {
        return Err(invalid().into());
    }
    let page_limit = limit(&input.limit)?;
    let seek = cursor(input.cursor.as_deref())?;
    let record = match action {
        EvidenceAction::Search => {
            if input.record_id.is_some() {
                return Err(invalid().into());
            }
            None
        }
        EvidenceAction::Open => Some(
            input
                .record_id
                .as_deref()
                .filter(|v| !v.is_empty())
                .ok_or_else(invalid)?,
        ),
    };
    let query = FieldExecutiveQuery {
        access_only: false,
        metric: FieldMetric::Completion,
        breakdown: FieldBreakdown::Default,
        from_at: from,
        to_at: to,
        query: if record.is_some() {
            String::new()
        } else {
            input.query.clone()
        },
        before_at: seek.as_ref().map(|v| v.0),
        before_id: seek.as_ref().map(|v| v.1.clone()),
        evidence_limit: Some(if record.is_some() { 1 } else { page_limit }),
    };
    let prepared =
        crate::field_executive_read::prepare_read(c, current, target, query, record).await?;
    let rows = prepared.response.data["evidence"]
        .as_array()
        .ok_or_else(invalid)?;
    let visible = rows
        .iter()
        .take(if record.is_some() { 1 } else { page_limit })
        .collect::<Vec<_>>();
    if record.is_some() && visible.is_empty() {
        return Err(missing().into());
    }
    let query_hash = match record {
        Some(id) => hash(
            serde_json::to_string(&OpenHash {
                provider_id: PROVIDER,
                metric: METRIC,
                filters: &input.filters,
                record_id: id,
            })
            .map_err(|_| invalid())?,
        ),
        None => hash(
            serde_json::to_string(&SearchHash {
                provider_id: PROVIDER,
                metric: METRIC,
                filters: &input.filters,
                query: &input.query,
                cursor: input.cursor.as_deref(),
            })
            .map_err(|_| invalid())?,
        ),
    };
    let mut audits = Vec::new();
    let mut items = Vec::new();
    for row in &visible {
        let id = row["id"].as_str().ok_or_else(invalid)?;
        let version = row["version"].as_i64().ok_or_else(invalid)?;
        audits.push(EvidenceAccessRow {
            id: id.into(),
            version,
        });
        let at = row["scheduled_at"].as_i64().ok_or_else(invalid)?;
        let timestamp = time::OffsetDateTime::from_unix_timestamp_nanos(at as i128 * 1_000_000)
            .map_err(|_| invalid())?
            .format(time::macros::format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
            ))
            .map_err(|_| invalid())?;
        items.push(json!({"id":id,"label":id,"summary":format!("{} · {timestamp} UTC",row["status"].as_str().ok_or_else(invalid)?),"status":row["status"],"version":version,"href":format!("/field-service/{}",encode(id))}));
    }
    let data = if let Some(record) = record {
        let mut item = visible[0].clone();
        item.as_object_mut().ok_or_else(invalid)?.insert(
            "href".into(),
            json!(format!("/field-service/{}", encode(record))),
        );
        json!({"provider":PROVIDER,"metric":METRIC,"fields":FIELDS,"record":item,"queryHash":query_hash})
    } else {
        let more = rows.len() > page_limit;
        let next = if more {
            let row = visible.last().ok_or_else(invalid)?;
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Cursor<'a> {
                scheduled_at: i64,
                id: &'a str,
            }
            Some(
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&Cursor {
                        scheduled_at: row["scheduled_at"].as_i64().ok_or_else(invalid)?,
                        id: row["id"].as_str().ok_or_else(invalid)?,
                    })
                    .map_err(|_| invalid())?,
                ),
            )
        } else {
            None
        };
        json!({"provider":PROVIDER,"metric":METRIC,"fields":FIELDS,"items":items,"hasMore":more,"nextCursor":next,"queryHash":query_hash})
    };
    record_executive_evidence_access(
        c,
        ExecutiveEvidenceAccess {
            company: current.selection().workspace().company_id.as_str(),
            user: current.user_id().as_str(),
            assignment: current.selection().assignment().id.as_str(),
            provider: PROVIDER,
            metric: METRIC,
            action: if record.is_some() {
                EvidenceAccessAction::Open
            } else {
                EvidenceAccessAction::Search
            },
            fields: FIELDS,
            query_hash: &query_hash,
            at: target.clock.now().get(),
            rows: &audits,
        },
    )
    .await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    prepared.require_current(c, current, target).await?;
    Ok(crate::field_executive_read::FieldExecutiveReadResponse {
        data,
        context: prepared.response.context,
    })
}
