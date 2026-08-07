use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_integration_contracts::{customer, identity_access};
use erpx_kernel::TenantScope;
use erpx_support::ports::{
    CustomerTicketMessageView, IncidentList, IncidentListFilter, IncidentView,
    InternalTicketMessageView, PortError, ReadProjection, ReadRepository, SupportCapabilityView,
    SupportSessionView, SupportTicketList, SupportTicketListFilter, SupportTicketView,
    TicketReadAudience,
};
use erpx_support::{
    CapabilityId, IncidentId, MessageAudience, MessageId, SessionId, SupportTicketId,
};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    CAPABILITY_PROJECTION, INCIDENT_PROJECTION, SESSION_PROJECTION, TICKET_PROJECTION,
    capability_partition, incident_partition, session_partition, ticket_partition,
};

#[derive(Debug, Clone)]
pub struct SupportReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}
impl SupportReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }
    pub fn with_waiter_config(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
        })
    }
    async fn checkpoint(
        &self,
        projection: &str,
        consistency: &QueryConsistency,
        exact: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact {
                Some(partition) => self
                    .read_side
                    .checkpoint(projection, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != projection
                    || !valid_partition(projection, &requirement.partition)
                    || exact.is_some_and(|p| p != requirement.partition)
                {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: requirement.projection.clone(),
                        partition: requirement.partition.clone(),
                        target: requirement.target,
                        observed: None,
                    }));
                }
                self.waiter
                    .wait_until_checked(requirement)
                    .await
                    .map(Some)
                    .map_err(|e| match e {
                        WaitError::NotReady(v) => PortError::NotReady(v),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }
    async fn metadata(
        &self,
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status:Option<String>=sqlx::query_scalar("SELECT status FROM erpx_cqrs_read.projection_generations WHERE projection_name=$1 AND status IN ('active','building') ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1").bind(projection).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint
            .as_ref()
            .and_then(|v| Utc::now().signed_duration_since(v.updated_at).to_std().ok())
            .and_then(|v| u64::try_from(v.as_millis()).ok());
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }

    async fn ticket_messages(
        &self,
        generation: Uuid,
        partition: &str,
        id: SupportTicketId,
        audience: TicketReadAudience,
    ) -> Result<
        (
            Vec<CustomerTicketMessageView>,
            Option<Vec<InternalTicketMessageView>>,
        ),
        PortError,
    > {
        match audience {
            TicketReadAudience::Customer => {
                let rows = sqlx::query("SELECT message_id,author_ref,body,created_at FROM erpx_support_read.ticket_customer_message_projection WHERE projection_name=$1 AND generation=$2 AND partition_key=$3 AND ticket_id=$4 ORDER BY created_at,message_id")
                    .bind(TICKET_PROJECTION).bind(generation).bind(partition).bind(id.as_uuid()).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
                Ok((
                    rows.into_iter()
                        .map(decode_customer_message)
                        .collect::<Result<_, _>>()?,
                    None,
                ))
            }
            TicketReadAudience::Internal => {
                let rows = sqlx::query("SELECT message_id,author_ref,audience,body,created_at FROM erpx_support_read.ticket_internal_message_projection WHERE projection_name=$1 AND generation=$2 AND partition_key=$3 AND ticket_id=$4 ORDER BY created_at,message_id")
                    .bind(TICKET_PROJECTION).bind(generation).bind(partition).bind(id.as_uuid()).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
                let internal = rows
                    .into_iter()
                    .map(decode_internal_message)
                    .collect::<Result<Vec<_>, _>>()?;
                let customer = internal
                    .iter()
                    .filter(|v| v.audience == MessageAudience::CustomerVisible)
                    .map(|v| CustomerTicketMessageView {
                        message_id: v.message_id,
                        author_ref: v.author_ref.clone(),
                        body: v.body.clone(),
                        created_at: v.created_at,
                    })
                    .collect();
                Ok((customer, Some(internal)))
            }
        }
    }
}

#[async_trait]
impl ReadRepository for SupportReadRepository {
    async fn get_ticket(
        &self,
        authority: &TenantScope,
        id: SupportTicketId,
        audience: TicketReadAudience,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SupportTicketView>>, PortError> {
        let partition = ticket_partition(id);
        let checkpoint = self
            .checkpoint(TICKET_PROJECTION, consistency, Some(&partition))
            .await?;
        let filter = ScopeFilter::from(authority)?;
        let row=sqlx::query("SELECT item.*,item.generation FROM erpx_support_read.ticket_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.ticket_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(TICKET_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(filter.platform).bind(&filter.exact).bind(&filter.prefixes).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let value = if let Some(row) = row {
            let generation: Uuid = row.try_get("generation").map_err(|_| PortError::Internal)?;
            let (customer_messages, internal_messages) = self
                .ticket_messages(generation, &partition, id, audience)
                .await?;
            let view = decode_ticket(row, customer_messages, internal_messages)?;
            if view.ticket_id != id || !authority.contains(&view.scope) {
                return Err(PortError::Internal);
            }
            Some(view)
        } else {
            None
        };
        Ok(ReadProjection {
            value,
            metadata: self.metadata(TICKET_PROJECTION, checkpoint).await?,
        })
    }
    async fn list_tickets(
        &self,
        authority: &TenantScope,
        filter: &SupportTicketListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SupportTicketList>, PortError> {
        let checkpoint = self
            .checkpoint(TICKET_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(ticket_status_name);
        let customer = filter.customer.map(|v| v.as_uuid());
        let limit = i64::from(filter.page_size) + 1;
        let rows=sqlx::query("SELECT item.*,item.generation FROM erpx_support_read.ticket_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::text IS NULL OR item.ticket_status=$5) AND ($6::uuid IS NULL OR item.customer_id=$6) AND ($7::uuid IS NULL OR item.ticket_id>$7) ORDER BY item.ticket_id LIMIT $8")
            .bind(TICKET_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).bind(status).bind(customer).bind(after).bind(limit).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut values = Vec::with_capacity(rows.len());
        for row in rows {
            let id = SupportTicketId::try_from_uuid(
                row.try_get("ticket_id").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            let generation: Uuid = row.try_get("generation").map_err(|_| PortError::Internal)?;
            let partition = ticket_partition(id);
            let (messages, internal) = self
                .ticket_messages(generation, &partition, id, filter.audience)
                .await?;
            values.push(decode_ticket(row, messages, internal)?);
        }
        validate_scopes(authority, values.iter().map(|v| &v.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |v| v.ticket_id.as_uuid());
        Ok(ReadProjection {
            value: SupportTicketList {
                tickets: values,
                next_cursor,
            },
            metadata: self.metadata(TICKET_PROJECTION, checkpoint).await?,
        })
    }
    async fn get_incident(
        &self,
        authority: &TenantScope,
        id: IncidentId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<IncidentView>>, PortError> {
        let partition = incident_partition(id);
        let checkpoint = self
            .checkpoint(INCIDENT_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row=sqlx::query("SELECT item.state FROM erpx_support_read.incident_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.incident_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))").bind(INCIDENT_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value
            .as_ref()
            .is_some_and(|v: &IncidentView| v.incident_id != id || !authority.contains(&v.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(INCIDENT_PROJECTION, checkpoint).await?,
        })
    }
    async fn list_incidents(
        &self,
        authority: &TenantScope,
        filter: &IncidentListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<IncidentList>, PortError> {
        let checkpoint = self
            .checkpoint(INCIDENT_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(incident_status_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows=sqlx::query("SELECT item.state FROM erpx_support_read.incident_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::text IS NULL OR item.incident_status=$5) AND ($6::uuid IS NULL OR item.incident_id>$6) ORDER BY item.incident_id LIMIT $7").bind(INCIDENT_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).bind(status).bind(after).bind(limit).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut values = decode_rows::<IncidentView>(rows)?;
        validate_scopes(authority, values.iter().map(|v| &v.scope))?;
        let next_cursor =
            truncate_cursor(&mut values, filter.page_size, |v| v.incident_id.as_uuid());
        Ok(ReadProjection {
            value: IncidentList {
                incidents: values,
                next_cursor,
            },
            metadata: self.metadata(INCIDENT_PROJECTION, checkpoint).await?,
        })
    }
    async fn get_session(
        &self,
        authority: &TenantScope,
        id: SessionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SupportSessionView>>, PortError> {
        let partition = session_partition(id);
        let checkpoint = self
            .checkpoint(SESSION_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row=sqlx::query("SELECT item.* FROM erpx_support_read.session_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.session_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))").bind(SESSION_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode_session).transpose()?;
        if value.as_ref().is_some_and(|v: &SupportSessionView| {
            v.session_id != id || !authority.contains(&v.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(SESSION_PROJECTION, checkpoint).await?,
        })
    }
    async fn get_capability(
        &self,
        authority: &TenantScope,
        id: CapabilityId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SupportCapabilityView>>, PortError> {
        let partition = capability_partition(id);
        let checkpoint = self
            .checkpoint(CAPABILITY_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row=sqlx::query("SELECT item.* FROM erpx_support_read.capability_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.capability_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))").bind(CAPABILITY_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode_capability).transpose()?;
        if value.as_ref().is_some_and(|v: &SupportCapabilityView| {
            v.capability_id != id || !authority.contains(&v.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(CAPABILITY_PROJECTION, checkpoint).await?,
        })
    }
}

fn decode_ticket(
    row: PgRow,
    customer_messages: Vec<CustomerTicketMessageView>,
    internal_messages: Option<Vec<InternalTicketMessageView>>,
) -> Result<SupportTicketView, PortError> {
    Ok(SupportTicketView {
        ticket_id: SupportTicketId::try_from_uuid(get(&row, "ticket_id")?)
            .map_err(|_| PortError::Internal)?,
        version: to_u64(get::<i64>(&row, "source_aggregate_version")?)?,
        scope: json(&row, "tenant_scope")?,
        customer: customer::CustomerRef::try_from_uuid(get(&row, "customer_id")?)
            .map_err(|_| PortError::Internal)?,
        customer_account: customer::CustomerAccountRef::try_from_uuid(get(
            &row,
            "customer_account_id",
        )?)
        .map_err(|_| PortError::Internal)?,
        case_code: get(&row, "case_code")?,
        category: get(&row, "category")?,
        severity: get(&row, "severity")?,
        description: get(&row, "description")?,
        status: parse_ticket_status(&get::<String>(&row, "ticket_status")?)?,
        customer_messages,
        internal_messages,
        created_at: get(&row, "aggregate_created_at")?,
        updated_at: get(&row, "aggregate_updated_at")?,
    })
}
fn decode_customer_message(row: PgRow) -> Result<CustomerTicketMessageView, PortError> {
    Ok(CustomerTicketMessageView {
        message_id: MessageId::try_from_uuid(get(&row, "message_id")?)
            .map_err(|_| PortError::Internal)?,
        author_ref: get(&row, "author_ref")?,
        body: get(&row, "body")?,
        created_at: get(&row, "created_at")?,
    })
}
fn decode_internal_message(row: PgRow) -> Result<InternalTicketMessageView, PortError> {
    Ok(InternalTicketMessageView {
        message_id: MessageId::try_from_uuid(get(&row, "message_id")?)
            .map_err(|_| PortError::Internal)?,
        author_ref: get(&row, "author_ref")?,
        audience: parse_message_audience(&get::<String>(&row, "audience")?)?,
        body: get(&row, "body")?,
        created_at: get(&row, "created_at")?,
    })
}
fn decode_session(row: PgRow) -> Result<SupportSessionView, PortError> {
    Ok(SupportSessionView {
        session_id: SessionId::try_from_uuid(get(&row, "session_id")?)
            .map_err(|_| PortError::Internal)?,
        version: to_u64(get::<i64>(&row, "source_aggregate_version")?)?,
        scope: json(&row, "approved_scope")?,
        ticket_id: SupportTicketId::try_from_uuid(get(&row, "ticket_id")?)
            .map_err(|_| PortError::Internal)?,
        customer: customer::CustomerRef::try_from_uuid(get(&row, "customer_id")?)
            .map_err(|_| PortError::Internal)?,
        customer_account: customer::CustomerAccountRef::try_from_uuid(get(
            &row,
            "customer_account_id",
        )?)
        .map_err(|_| PortError::Internal)?,
        agent: identity_access::PrincipalRef::try_from_uuid(get(&row, "agent_principal_id")?)
            .map_err(|_| PortError::Internal)?,
        represented_principal: get::<Option<Uuid>>(&row, "represented_principal_id")?
            .map(identity_access::PrincipalRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        mode: parse_mode(&get::<String>(&row, "support_mode")?)?,
        permissions: json(&row, "permissions")?,
        module: get(&row, "module")?,
        reason: get(&row, "reason")?,
        status: parse_session_status(&get::<String>(&row, "session_status")?)?,
        requested_at: get(&row, "requested_at")?,
        expires_at: get(&row, "expires_at")?,
        approved_by: get(&row, "approved_by")?,
        approved_at: get(&row, "approved_at")?,
        revoked_at: get(&row, "revoked_at")?,
        revoked_reason: get(&row, "revoked_reason")?,
    })
}
fn decode_capability(row: PgRow) -> Result<SupportCapabilityView, PortError> {
    let reason = get::<Option<String>>(&row, "revocation_reason")?;
    let revoked_at = get::<Option<chrono::DateTime<chrono::Utc>>>(&row, "revoked_at")?;
    let revocation = match (reason, revoked_at) {
        (None, None) => None,
        (Some(r), Some(at)) => Some((parse_revocation(&r)?, at)),
        _ => return Err(PortError::Internal),
    };
    Ok(SupportCapabilityView {
        capability_id: CapabilityId::try_from_uuid(get(&row, "capability_id")?)
            .map_err(|_| PortError::Internal)?,
        version: to_u64(get::<i64>(&row, "source_aggregate_version")?)?,
        session_id: SessionId::try_from_uuid(get(&row, "session_id")?)
            .map_err(|_| PortError::Internal)?,
        ticket_id: SupportTicketId::try_from_uuid(get(&row, "ticket_id")?)
            .map_err(|_| PortError::Internal)?,
        customer: customer::CustomerRef::try_from_uuid(get(&row, "customer_id")?)
            .map_err(|_| PortError::Internal)?,
        customer_account: customer::CustomerAccountRef::try_from_uuid(get(
            &row,
            "customer_account_id",
        )?)
        .map_err(|_| PortError::Internal)?,
        agent: identity_access::PrincipalRef::try_from_uuid(get(&row, "agent_principal_id")?)
            .map_err(|_| PortError::Internal)?,
        device_ref: get(&row, "device_ref")?,
        mode: parse_mode(&get::<String>(&row, "support_mode")?)?,
        permissions: json(&row, "permissions")?,
        scope: json(&row, "approved_scope")?,
        module: get(&row, "module")?,
        issued_at: get(&row, "issued_at")?,
        expires_at: get(&row, "expires_at")?,
        revocation,
    })
}

fn get<'a, T: sqlx::Decode<'a, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>>(
    row: &'a PgRow,
    name: &str,
) -> Result<T, PortError> {
    row.try_get(name).map_err(|_| PortError::Internal)
}
fn json<T: serde::de::DeserializeOwned>(row: &PgRow, name: &str) -> Result<T, PortError> {
    let value: serde_json::Value = row.try_get(name).map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}
fn decode_state<T: serde::de::DeserializeOwned>(row: PgRow) -> Result<T, PortError> {
    json(&row, "state")
}
fn decode_rows<T: serde::de::DeserializeOwned>(rows: Vec<PgRow>) -> Result<Vec<T>, PortError> {
    rows.into_iter().map(decode_state).collect()
}
pub fn encode_cursor(id: Uuid) -> String {
    id.simple().to_string()
}
pub fn decode_cursor(value: &str) -> Result<Uuid, PortError> {
    if value.len() != 32 {
        return Err(PortError::Internal);
    }
    Uuid::parse_str(value).map_err(|_| PortError::Internal)
}
fn truncate_cursor<T>(
    values: &mut Vec<T>,
    page_size: u16,
    id: impl Fn(&T) -> Uuid,
) -> Option<String> {
    if values.len() > usize::from(page_size) {
        values.truncate(usize::from(page_size));
        values.last().map(|v| encode_cursor(id(v)))
    } else {
        None
    }
}
fn validate_scopes<'a>(
    authority: &TenantScope,
    scopes: impl Iterator<Item = &'a TenantScope>,
) -> Result<(), PortError> {
    if scopes.into_iter().all(|scope| authority.contains(scope)) {
        Ok(())
    } else {
        Err(PortError::Internal)
    }
}
fn valid_partition(projection: &str, partition: &str) -> bool {
    let prefix = match projection {
        TICKET_PROJECTION => "support.ticket:",
        INCIDENT_PROJECTION => "support.incident:",
        SESSION_PROJECTION => "support.session:",
        CAPABILITY_PROJECTION => "support.capability:",
        _ => return false,
    };
    partition
        .strip_prefix(prefix)
        .is_some_and(|v| v.len() == 32 && u128::from_str_radix(v, 16).is_ok())
}

struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
}
impl ScopeFilter {
    fn from(scope: &TenantScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            TenantScope::Platform => Self {
                platform: true,
                exact: vec![],
                prefixes: vec![],
            },
            TenantScope::Holding { holding_id } => Self {
                platform: false,
                exact: vec![],
                prefixes: vec![format!("h:{holding_id}/%")],
            },
            TenantScope::Company {
                holding_id,
                company_id,
            } => {
                let key = format!("h:{holding_id}/c:{company_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                }
            }
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}/c:{company_id}/b:{branch_id}")],
                prefixes: vec![],
            },
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Self {
                platform: false,
                exact: company_ids
                    .iter()
                    .flat_map(|company| {
                        if branch_ids.is_empty() {
                            vec![format!("h:{holding_id}/c:{company}")]
                        } else {
                            branch_ids
                                .iter()
                                .map(|branch| format!("h:{holding_id}/c:{company}/b:{branch}"))
                                .collect()
                        }
                    })
                    .collect(),
                prefixes: vec![],
            },
        })
    }
}
fn to_u64(v: i64) -> Result<u64, PortError> {
    u64::try_from(v).map_err(|_| PortError::Internal)
}
fn map_read_error(e: CqrsSqlxError) -> PortError {
    match e {
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}
fn ticket_status_name(v: erpx_support::TicketStatus) -> &'static str {
    crate::projection::ticket_status_name(v)
}
fn incident_status_name(v: erpx_support::IncidentStatus) -> &'static str {
    crate::projection::incident_status_name(v)
}
fn parse_ticket_status(v: &str) -> Result<erpx_support::TicketStatus, PortError> {
    match v {
        "new" => Ok(erpx_support::TicketStatus::New),
        "triaged" => Ok(erpx_support::TicketStatus::Triaged),
        "waiting_for_customer" => Ok(erpx_support::TicketStatus::WaitingForCustomer),
        "access_requested" => Ok(erpx_support::TicketStatus::AccessRequested),
        "approved" => Ok(erpx_support::TicketStatus::Approved),
        "in_progress" => Ok(erpx_support::TicketStatus::InProgress),
        "resolved" => Ok(erpx_support::TicketStatus::Resolved),
        "customer_confirmed" => Ok(erpx_support::TicketStatus::CustomerConfirmed),
        "closed" => Ok(erpx_support::TicketStatus::Closed),
        _ => Err(PortError::Internal),
    }
}
fn parse_message_audience(v: &str) -> Result<MessageAudience, PortError> {
    match v {
        "customer_visible" => Ok(MessageAudience::CustomerVisible),
        "internal" => Ok(MessageAudience::Internal),
        _ => Err(PortError::Internal),
    }
}
fn parse_mode(v: &str) -> Result<erpx_support::SupportMode, PortError> {
    match v {
        "diagnostic_read" => Ok(erpx_support::SupportMode::DiagnosticRead),
        "tenant_mutation" => Ok(erpx_support::SupportMode::TenantMutation),
        "impersonation" => Ok(erpx_support::SupportMode::Impersonation),
        _ => Err(PortError::Internal),
    }
}
fn parse_session_status(v: &str) -> Result<erpx_support::SessionStatus, PortError> {
    match v {
        "requested" => Ok(erpx_support::SessionStatus::Requested),
        "approved" => Ok(erpx_support::SessionStatus::Approved),
        "revoked" => Ok(erpx_support::SessionStatus::Revoked),
        _ => Err(PortError::Internal),
    }
}
fn parse_revocation(v: &str) -> Result<erpx_support::CapabilityRevocationReason, PortError> {
    match v {
        "customer_requested" => Ok(erpx_support::CapabilityRevocationReason::CustomerRequested),
        "agent_disabled" => Ok(erpx_support::CapabilityRevocationReason::AgentDisabled),
        "device_revoked" => Ok(erpx_support::CapabilityRevocationReason::DeviceRevoked),
        "session_revoked" => Ok(erpx_support::CapabilityRevocationReason::SessionRevoked),
        "ticket_closed" => Ok(erpx_support::CapabilityRevocationReason::TicketClosed),
        "administrative" => Ok(erpx_support::CapabilityRevocationReason::Administrative),
        _ => Err(PortError::Internal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursors_and_partitions_are_type_safe() {
        let id = Uuid::from_u128(8);
        assert_eq!(decode_cursor(&encode_cursor(id)).unwrap(), id);
        let ticket = ticket_partition(SupportTicketId::try_from_uuid(id).unwrap());
        assert!(valid_partition(TICKET_PROJECTION, &ticket));
        assert!(!valid_partition(CAPABILITY_PROJECTION, &ticket));
    }
    #[test]
    fn customer_query_source_never_names_internal_projection() {
        let source = include_str!("read.rs");
        let start = source.find("TicketReadAudience::Customer =>").unwrap();
        let end = source[start..]
            .find("TicketReadAudience::Internal =>")
            .unwrap()
            + start;
        assert!(!source[start..end].contains("ticket_internal_message_projection"));
        assert!(source[start..end].contains("ticket_customer_message_projection"));
    }
}
