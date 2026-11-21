//! PostgreSQL adapter for Executive external connections.
//!
//! Token rows, the final create audit and the shared request budget live on the
//! Control database; current assignment, organization, role, engagement and
//! membership witnesses live on the selected Holding write database. The two
//! units are explicit and never substituted for one another.
use crate::application::executive_connections::{ExecutiveConnectionsStore, StoreFuture};
use crate::domain::executive_connections::*;
use crate::public::postgres::{DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

fn persistence() -> BusinessError {
    unavailable()
}

fn from_transaction(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => persistence(),
    }
}

#[derive(Clone)]
pub struct PgExecutiveConnections {
    control: DatabasePool,
    holding: DatabasePool,
}

/// Consume one shared MCP request from the retained per-user/company minute
/// window. Rejects unknown, revoked and expired secrets without revealing
/// which fact failed. Only the Control unit is read.
pub async fn consume_rate(
    control: &DatabasePool,
    secret_hash: &str,
    now: i64,
) -> Result<i64, BusinessError> {
    let token = sqlx::query(
        "SELECT user_id,company_id FROM executive_external_tokens \
         WHERE secret_hash=$1 AND revoked_at IS NULL AND expires_at>$2",
    )
    .bind(secret_hash)
    .bind(now)
    .fetch_optional(control.raw())
    .await
    .map_err(|_| persistence())?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unauthenticated,
            "INVALID_TOKEN",
            "This Executive token is invalid, expired or revoked.",
        )
    })?;
    let user_id: String = token.get("user_id");
    let company_id: String = token.get("company_id");
    let requests: i32 = sqlx::query_scalar(
        "INSERT INTO executive_mcp_rate_limits(user_id,company_id,window_start,requests) \
         VALUES($1,$2,$3,1) \
         ON CONFLICT(user_id,company_id) DO UPDATE SET window_start=excluded.window_start, \
         requests=CASE WHEN executive_mcp_rate_limits.window_start=excluded.window_start \
         THEN executive_mcp_rate_limits.requests+1 ELSE 1 END \
         RETURNING requests",
    )
    .bind(user_id)
    .bind(company_id)
    .bind(rate_window(now))
    .fetch_one(control.raw())
    .await
    .map_err(|_| persistence())?;
    if i64::from(requests) > 60 {
        return Err(rate_limited());
    }
    Ok(i64::from(requests))
}

impl PgExecutiveConnections {
    /// Explicit Control and Holding units. The caller must pass the Control
    /// database that owns the token schema and the selected Holding write unit.
    pub fn new(control: DatabasePool, holding: DatabasePool) -> Self {
        Self { control, holding }
    }

    pub fn control(&self) -> &DatabasePool {
        &self.control
    }

    pub fn holding(&self) -> &DatabasePool {
        &self.holding
    }

    async fn scopes_on(
        connection: &mut PgConnection,
        actor: &ConnectionActor,
    ) -> Result<Vec<ConnectionScope>, BusinessError> {
        let rows = sqlx::query(
            "SELECT n.id,n.kind,n.display_name,n.branch_id,b.display_name branch_name \
             FROM organization_nodes n \
             LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.status='active' \
             WHERE n.company_id=$1 AND n.status='active' AND n.kind IN ('company','branch','department') \
             AND (n.branch_id IS NULL OR b.id IS NOT NULL) \
             AND ($2::text IS NULL OR n.id=$3 OR n.branch_id=$4) \
             AND ($5::text IS NULL OR n.id=$6) \
             ORDER BY n.kind,n.display_name,n.id",
        )
        .bind(&actor.company_id)
        .bind(&actor.branch_id)
        .bind(&actor.branch_id)
        .bind(&actor.branch_id)
        .bind(&actor.department_id)
        .bind(&actor.department_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| persistence())?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let kind: String = row.get("kind");
                let id: String = row.get("id");
                let display_name: String = row.get("display_name");
                let branch_id: Option<String> = row.get("branch_id");
                let branch_name: Option<String> = row.get("branch_name");
                let department = kind == "department";
                ConnectionScope {
                    label: scope_label(&kind, &display_name, branch_name.as_deref()),
                    branch_id: if kind == "branch" {
                        Some(id.clone())
                    } else {
                        branch_id
                    },
                    department_id: department.then_some(id.clone()),
                    id,
                }
            })
            .collect())
    }

    async fn eligible_on(
        connection: &mut PgConnection,
        actor: &ConnectionActor,
    ) -> Result<bool, BusinessError> {
        let eligible: bool = sqlx::query_scalar(
            "SELECT (\
               ($1='holding_owner' AND iam_role_allows($1,$2,'company.owner')) \
               OR EXISTS(\
                 SELECT 1 FROM hr_people p \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id \
                 WHERE p.company_id=$2 AND p.user_id=$3 AND p.status='active' AND p.merged_into_id IS NULL \
                 AND e.status='active' \
                 AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
                 AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
                 AND l.active=1 AND l.level_number IN (1,2))\
             )",
        )
        .bind(&actor.role)
        .bind(&actor.company_id)
        .bind(&actor.user_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| persistence())?;
        Ok(eligible)
    }

    async fn access_current_on(
        connection: &mut PgConnection,
        token: &TokenRow,
        now: i64,
    ) -> Result<bool, BusinessError> {
        let current: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM assignments a \
             JOIN organization_nodes c ON c.id=a.company_id AND c.status='active' AND c.kind='company' \
             WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND a.role=$4 AND a.version=$5 \
             AND a.scope_kind=$6 AND a.scope_node_id IS NOT DISTINCT FROM $7::text \
             AND a.revoked_at IS NULL AND a.valid_from<=$8 AND a.valid_until>$8 \
             AND iam_assignment_active(a.id,$8) AND iam_principal_active(a.user_id,a.company_id,$8) \
             AND iam_role_allows(a.role,$2,'executive.access') \
             AND ($9::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b \
                  WHERE b.id=$9 AND b.company_id=$2 AND b.status='active' AND b.kind='branch')) \
             AND ($10::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes d \
                  WHERE d.id=$10 AND d.company_id=$2 AND d.status='active' AND d.kind='department' \
                  AND d.branch_id IS NOT DISTINCT FROM $9::text)) \
             AND (\
               (a.role='holding_owner' AND iam_role_allows(a.role,$2,'company.owner')) \
               OR EXISTS(\
                 SELECT 1 FROM hr_people p \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id \
                 WHERE p.company_id=$2 AND p.user_id=$3 AND p.status='active' AND p.merged_into_id IS NULL \
                 AND e.status='active' \
                 AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
                 AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
                 AND l.active=1 AND l.level_number IN (1,2))\
             ) LIMIT 1",
        )
        .bind(&token.assignment_id)
        .bind(&token.company_id)
        .bind(&token.user_id)
        .bind(&token.role)
        .bind(token.assignment_version)
        .bind(&token.assignment_scope_kind)
        .bind(&token.assignment_scope_node_id)
        .bind(now)
        .bind(&token.branch_id)
        .bind(&token.department_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| persistence())?;
        Ok(current.is_some())
    }
}

fn token_row(row: sqlx::postgres::PgRow) -> TokenRow {
    TokenRow {
        id: row.get("id"),
        name: row.get("name"),
        user_id: row.get("user_id"),
        holding_id: row.get("holding_id"),
        company_id: row.get("company_id"),
        assignment_id: row.get("assignment_id"),
        assignment_version: i64::from(row.get::<i32, _>("assignment_version")),
        role: row.get("role"),
        assignment_scope_kind: row.get("assignment_scope_kind"),
        assignment_scope_node_id: row.get("assignment_scope_node_id"),
        branch_id: row.get("branch_id"),
        department_id: row.get("department_id"),
        scope_label: row.get("scope_label"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
        last_used_at: row.get("last_used_at"),
    }
}

impl ExecutiveConnectionsStore for PgExecutiveConnections {
    fn list_tokens<'a>(&'a self, actor: &'a ConnectionActor) -> StoreFuture<'a, Vec<TokenRow>> {
        Box::pin(async move {
            let sql = "SELECT id,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,\
assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,created_at,expires_at,\
revoked_at,last_used_at FROM executive_external_tokens \
WHERE user_id=$1 AND holding_id=$2 AND company_id=$3 \
ORDER BY created_at DESC LIMIT 100";
            let rows = sqlx::query(sql)
                .bind(&actor.user_id)
                .bind(&actor.holding_id)
                .bind(&actor.company_id)
                .fetch_all(self.control.raw())
                .await
                .map_err(|_| persistence())?;
            Ok(rows.into_iter().map(token_row).collect())
        })
    }

    fn scopes<'a>(&'a self, actor: &'a ConnectionActor) -> StoreFuture<'a, Vec<ConnectionScope>> {
        Box::pin(async move {
            let mut connection = self
                .holding
                .raw()
                .acquire()
                .await
                .map_err(|_| persistence())?;
            Self::scopes_on(&mut connection, actor).await
        })
    }

    fn eligible<'a>(&'a self, actor: &'a ConnectionActor) -> StoreFuture<'a, bool> {
        Box::pin(async move {
            let mut connection = self
                .holding
                .raw()
                .acquire()
                .await
                .map_err(|_| persistence())?;
            Self::eligible_on(&mut connection, actor).await
        })
    }

    fn selected_grant<'a>(
        &'a self,
        actor: &'a ConnectionActor,
    ) -> StoreFuture<'a, Option<GrantRow>> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT role,valid_until FROM assignments WHERE id=$1 AND company_id=$2 AND user_id=$3",
            )
            .bind(&actor.assignment_id)
            .bind(&actor.company_id)
            .bind(&actor.user_id)
            .fetch_optional(self.holding.raw())
            .await
            .map_err(|_| persistence())?;
            Ok(row.map(|row| GrantRow {
                role: row.get("role"),
                valid_until: row.get("valid_until"),
            }))
        })
    }

    fn access_current<'a>(&'a self, token: &'a TokenRow, now: i64) -> StoreFuture<'a, bool> {
        Box::pin(async move {
            let mut connection = self
                .holding
                .raw()
                .acquire()
                .await
                .map_err(|_| persistence())?;
            Self::access_current_on(&mut connection, token, now).await
        })
    }

    fn create<'a>(&'a self, connection: &'a NewConnection) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let actor = connection.actor.clone();
            let id = connection.id.clone();
            let secret_hash = connection.secret_hash.clone();
            let name = connection.name.clone();
            let scope_label = connection.scope_label.clone();
            let created_at = connection.created_at;
            let expires_at = connection.expires_at;
            PgUnitOfWork::new(self.control.clone())
                .run_once(TransactionOptions::default(), move |tx| {
                    let actor = actor.clone();
                    let id = id.clone();
                    let secret_hash = secret_hash.clone();
                    let name = name.clone();
                    let scope_label = scope_label.clone();
                    Box::pin(async move {
                        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,7092))")
                            .bind(actor.principal_key())
                            .execute(&mut *tx)
                            .await?;
                        let active: i32 = sqlx::query_scalar(
                            "SELECT count(*)::integer FROM executive_external_tokens \
                             WHERE user_id=$1 AND company_id=$2 AND revoked_at IS NULL AND expires_at>$3",
                        )
                        .bind(&actor.user_id)
                        .bind(&actor.company_id)
                        .bind(created_at)
                        .fetch_one(&mut *tx)
                        .await?;
                        if i64::from(active) >= EXECUTIVE_CONNECTION_LIMIT {
                            return Err(connection_limit().into());
                        }
                        let inserted: Option<String> = sqlx::query_scalar(
                            "INSERT INTO executive_external_tokens(\
                              id,secret_hash,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,\
                              assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,created_at,expires_at\
                             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) \
                             ON CONFLICT(id) DO NOTHING RETURNING id",
                        )
                        .bind(&id)
                        .bind(&secret_hash)
                        .bind(&name)
                        .bind(&actor.user_id)
                        .bind(&actor.holding_id)
                        .bind(&actor.company_id)
                        .bind(&actor.assignment_id)
                        .bind(actor.assignment_version)
                        .bind(&actor.role)
                        .bind(&actor.scope_kind)
                        .bind(&actor.scope_node_id)
                        .bind(&actor.branch_id)
                        .bind(&actor.department_id)
                        .bind(&scope_label)
                        .bind(created_at)
                        .bind(expires_at)
                        .fetch_optional(&mut *tx)
                        .await?;
                        if inserted.is_none() {
                            return Err(already_created().into());
                        }
                        sqlx::query(
                            "INSERT INTO executive_mcp_audit(\
                              id,token_id,user_id,company_id,assignment_id,branch_id,department_id,action,outcome,code,started_at,duration_ms\
                             ) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,'success','OK',$8,0)",
                        )
                        .bind(&id)
                        .bind(&actor.user_id)
                        .bind(&actor.company_id)
                        .bind(&actor.assignment_id)
                        .bind(&actor.branch_id)
                        .bind(&actor.department_id)
                        .bind(EXECUTIVE_CONNECTION_AUDIT_ACTION)
                        .bind(created_at)
                        .execute(&mut *tx)
                        .await?;
                        Ok(())
                    })
                })
                .await
                .map_err(from_transaction)
        })
    }

    fn revoke<'a>(
        &'a self,
        actor: &'a ConnectionActor,
        id: &'a str,
        now: i64,
    ) -> StoreFuture<'a, bool> {
        Box::pin(async move {
            let revoked: Option<String> = sqlx::query_scalar(
                "UPDATE executive_external_tokens SET revoked_at=COALESCE(revoked_at,$1) \
                 WHERE id=$2 AND user_id=$3 AND holding_id=$4 AND company_id=$5 RETURNING id",
            )
            .bind(now)
            .bind(id)
            .bind(&actor.user_id)
            .bind(&actor.holding_id)
            .bind(&actor.company_id)
            .fetch_optional(self.control.raw())
            .await
            .map_err(|_| persistence())?;
            Ok(revoked.is_some())
        })
    }

    fn mark_revoked<'a>(&'a self, id: &'a str, now: i64) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            sqlx::query("UPDATE executive_external_tokens SET revoked_at=$1 WHERE id=$2")
                .bind(now)
                .bind(id)
                .execute(self.control.raw())
                .await
                .map_err(|_| persistence())?;
            Ok(())
        })
    }
}
