//! PostgreSQL adapter for the native Organization position-save command.
//!
//! The caller owns the Holding transaction, current session proof and Company
//! lease. This adapter re-checks the current IAM source, the organization graph
//! snapshot, the position version, writes the position row and stores the
//! command receipt, audit and outbox rows in the same unit. Roster-exit effects
//! remain behind the composition-owned `RosterExitPort`; no other owner's
//! tables are written here.
use crate::application::{
    organization_nodes::{OrganizationCommandActor, OrganizationFuture},
    position::OrganizationPositionRepository,
};
use crate::domain::position::{
    PositionGraphGuard, PositionInput, PositionRoster, PositionRosterPerson,
    PositionRosterPosition, position_not_found,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandEventScope, CommandField, CommandJson, WorkAuthority, command_fingerprint,
};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Retained `organizationGuard` graph snapshot. `to_jsonb` keeps the same row
/// images as the TypeScript guard; `people` and `engagements` are read through
/// the existing people/payroll relations selected by that SQL.
const GRAPH_SNAPSHOT_SQL: &str = "SELECT jsonb_build_object(
 'people',(SELECT jsonb_agg(p.snapshot ORDER BY p.id) FROM (SELECT person.id,person.company_id,to_jsonb(person) snapshot FROM hr_people person) p WHERE p.company_id=$1),
 'engagements',(SELECT jsonb_agg(e.snapshot ORDER BY e.id) FROM (SELECT engagement.id,engagement.company_id,to_jsonb(engagement) snapshot FROM hr_engagements engagement) e WHERE e.company_id=$2),
 'positions',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM organization_positions p WHERE p.company_id=$3),
 'levels',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM staff_levels l WHERE l.company_id=$4),
 'nodes',(SELECT jsonb_agg(to_jsonb(n) ORDER BY n.id) FROM organization_nodes n WHERE n.company_id=$5)
)::text";

fn command_actor(actor: &OrganizationCommandActor) -> Result<CommandActor, BusinessError> {
    let command = &actor.command;
    Ok(CommandActor {
        user_id: command.user_id.clone(),
        assignment_id: command.assignment_id.clone(),
        role: command.role.clone(),
        company_id: command.company_id.clone(),
        branch_id: command
            .branch_id
            .as_ref()
            .map(|value| CommandField::Value(value.clone()))
            .unwrap_or(CommandField::Null),
        department_id: command
            .department_id
            .as_ref()
            .map(|value| CommandField::Value(value.clone()))
            .unwrap_or(CommandField::Null),
        authority: command
            .authority
            .as_ref()
            .map(|authority| {
                CommandField::Value(WorkAuthority {
                    session_id: authority.session_id.clone(),
                    revision: authority.revision,
                    assignment_version: authority.assignment_version,
                })
            })
            .unwrap_or(CommandField::Null),
        iam: CommandField::Value(CommandJson::parse(&command.iam_json)?),
        assignment_scope_kind: command.assignment_scope_kind.clone(),
        assignment_scope_node_id: command.assignment_scope_node_id.clone(),
    })
}

fn fingerprint(
    actor: &OrganizationCommandActor,
    operation: &str,
    record_id: &str,
    input: &PositionInput,
) -> Result<String, BusinessError> {
    command_fingerprint(
        &command_actor(actor)?,
        operation,
        record_id,
        &CommandJson::from_serializable(input)?,
    )
}

fn conflict(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, "CONFLICT", message)
}

/// The retained `executeCommand` maps a failed guard/assertion (check
/// constraint) to this generic 409.
fn record_changed() -> BusinessError {
    conflict(
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

fn conflict_code(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| matches!(code.as_ref(), "23505" | "23503" | "23514" | "23P01"))
    {
        return record_changed().into();
    }
    error.into()
}

fn aggregate(operation: &str) -> String {
    operation.split('.').take(2).collect::<Vec<_>>().join("_")
}

pub struct PgOrganizationPosition<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOrganizationPosition<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn transaction(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn authorize(&mut self, actor: &OrganizationCommandActor, now: i64) -> Result<(), Error> {
        self.transaction()?;
        let command = &actor.command;
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments g \
             JOIN organization_nodes c ON c.id=g.company_id AND c.kind='company' AND c.status='active' \
             WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
             AND g.revoked_at IS NULL \
             AND iam_assignment_active(g.id,$5) AND iam_principal_active(g.user_id,g.company_id,$5) \
             AND iam_role_allows(g.role,g.company_id,'organization.manage'))",
        )
        .bind(command.assignment_id.as_str())
        .bind(command.user_id.as_str())
        .bind(&command.role)
        .bind(command.company_id.as_str())
        .bind(now)
        .fetch_one(&mut *self.connection)
        .await?;
        if !allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORGANIZATION_AUTHORITY_CHANGED",
                "Your current organization authority is unavailable.",
            )
            .into());
        }
        Ok(())
    }

    async fn receipt(&mut self, key: &str, hash: &str) -> Result<Option<Value>, Error> {
        let row = sqlx::query(
            "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let saved: String = row.try_get("request_hash")?;
        if saved != hash {
            return Err(conflict_code(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json")?;
        raw.and_then(|value| serde_json::from_str(&value).ok())
            .map(Some)
            .ok_or_else(|| {
                conflict_code(
                    "IDEMPOTENCY_CONFLICT",
                    "The retained result is unavailable.",
                )
                .into()
            })
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_receipt(
        &mut self,
        actor: &OrganizationCommandActor,
        key: &str,
        operation: &str,
        record_id: &str,
        hash: &str,
        result: &Value,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        let command_actor = command_actor(actor)?;
        let authority = command_actor.receipt_authority()?;
        let inserted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(actor.command.user_id.as_str())
        .bind(hash)
        .bind(record_id)
        .bind(result.to_string())
        .bind(now)
        .bind(authority.as_str())
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?
        .rows_affected()
            == 1;
        if !inserted {
            return Err(conflict_code(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let aggregate = aggregate(operation);
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,(SELECT authority_json FROM command_receipts WHERE command_id=$2))",
        )
        .bind(format!("audit-{key}"))
        .bind(key)
        .bind(actor.command.user_id.as_str())
        .bind(actor.command.assignment_id.as_str())
        .bind(operation)
        .bind(&aggregate)
        .bind(record_id)
        .bind(version)
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        let scope = CommandEventScope::from(&command_actor);
        let payload = command_actor.outbox_payload(record_id, version, None, &scope)?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("outbox-{key}"))
        .bind(key)
        .bind(operation)
        .bind(&aggregate)
        .bind(record_id)
        .bind(version)
        .bind(payload.as_str())
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        Ok(())
    }

    async fn people(&mut self, company: &CompanyId) -> Result<Vec<PositionRosterPerson>, Error> {
        let rows = sqlx::query(
            "SELECT p.id person_id,p.user_id,p.legal_name,p.email,p.version::bigint person_version,\
             p.status person_status,e.id engagement_id,e.version::bigint engagement_version,\
             e.company_id,e.branch_id,e.department_id,e.position_id,e.manager_person_id,e.level_id,\
             l.level_number::bigint level_number,l.name level_name,e.status,e.start_date,e.end_date,e.job_title \
             FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
             LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 \
             WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND p.status!='archived' \
             AND e.worker_type='employee' AND e.status!='ended' ORDER BY p.legal_name,e.id",
        )
        .bind(company.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PositionRosterPerson {
                    person_id: row.try_get("person_id")?,
                    user_id: row.try_get("user_id")?,
                    legal_name: row.try_get("legal_name")?,
                    email: row.try_get("email")?,
                    engagement_id: row.try_get("engagement_id")?,
                    engagement_version: row.try_get("engagement_version")?,
                    person_version: row.try_get("person_version")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    position_id: row.try_get("position_id")?,
                    manager_person_id: row.try_get("manager_person_id")?,
                    level_id: row.try_get("level_id")?,
                    level_number: row.try_get("level_number")?,
                    level_name: row.try_get("level_name")?,
                    status: row.try_get("status")?,
                    person_status: row.try_get("person_status")?,
                    start_date: row.try_get("start_date")?,
                    end_date: row.try_get("end_date")?,
                    job_title: row.try_get("job_title")?,
                    issues: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }

    async fn positions(
        &mut self,
        company: &CompanyId,
    ) -> Result<Vec<PositionRosterPosition>, Error> {
        let rows = sqlx::query(
            "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id,\
             p.reports_to_position_id,p.requirements,p.is_ceo::bigint is_ceo,\
             p.roster_required::bigint roster_required,p.active::bigint active,\
             p.version::bigint version,l.level_number::bigint level_number \
             FROM organization_positions p \
             JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id \
             WHERE p.company_id=$1 ORDER BY l.level_number,p.title,p.id",
        )
        .bind(company.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PositionRosterPosition {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    title: row.try_get("title")?,
                    level_id: row.try_get("level_id")?,
                    level_number: row.try_get("level_number")?,
                    reports_to_position_id: row.try_get("reports_to_position_id")?,
                    requirements: Some(row.try_get("requirements")?),
                    is_ceo: row.try_get("is_ceo")?,
                    roster_required: row.try_get("roster_required")?,
                    active: row.try_get("active")?,
                    version: row.try_get("version")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }

    async fn graph_guard(&mut self, company: &CompanyId) -> Result<PositionGraphGuard, Error> {
        let snapshot: String = sqlx::query_scalar(GRAPH_SNAPSHOT_SQL)
            .bind(company.as_str())
            .bind(company.as_str())
            .bind(company.as_str())
            .bind(company.as_str())
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
        Ok(PositionGraphGuard { snapshot })
    }

    async fn require_graph(
        &mut self,
        company: &CompanyId,
        guard: &PositionGraphGuard,
    ) -> Result<(), Error> {
        let query = format!("SELECT (({GRAPH_SNAPSHOT_SQL})::jsonb = $6::jsonb)");
        let current: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .bind(company.as_str())
            .bind(company.as_str())
            .bind(company.as_str())
            .bind(company.as_str())
            .bind(company.as_str())
            .bind(&guard.snapshot)
            .fetch_one(&mut *self.connection)
            .await?;
        if !current {
            return Err(record_changed().into());
        }
        Ok(())
    }

    async fn require_version(
        &mut self,
        company: &CompanyId,
        id: &str,
        expected_version: i64,
    ) -> Result<(), Error> {
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT version::bigint FROM organization_positions WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        match version {
            None => Err(position_not_found().into()),
            Some(version) if version != expected_version => Err(record_changed().into()),
            Some(_) => Ok(()),
        }
    }
}

impl OrganizationPositionRepository for PgOrganizationPosition<'_> {
    type Error = Error;

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a PositionInput,
    ) -> OrganizationFuture<'a, Option<Value>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let hash = fingerprint(actor, operation, record_id, input)?;
            self.receipt(key, &hash).await
        })
    }

    fn roster<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> OrganizationFuture<'a, PositionRoster, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let people = self.people(company).await?;
            let positions = self.positions(company).await?;
            Ok(PositionRoster { people, positions })
        })
    }

    fn branch_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: &'a str,
    ) -> OrganizationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active')",
            )
            .bind(branch)
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn department_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        department: &'a str,
        branch: Option<&'a str>,
    ) -> OrganizationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 \
                 AND kind='department' AND status='active' AND branch_id IS NOT DISTINCT FROM $3)",
            )
            .bind(company.as_str())
            .bind(department)
            .bind(branch)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn position_level<'a>(
        &'a mut self,
        company: &'a CompanyId,
        level: &'a str,
    ) -> OrganizationFuture<'a, Option<i64>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            Ok(sqlx::query_scalar::<_, i64>(
                "SELECT level_number::bigint FROM staff_levels WHERE id=$1 AND company_id=$2 \
                 AND active=1 AND level_number IS NOT NULL",
            )
            .bind(level)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn graph_guard<'a>(
        &'a mut self,
        company: &'a CompanyId,
        _key: &'a str,
    ) -> OrganizationFuture<'a, PositionGraphGuard, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            self.graph_guard(company).await
        })
    }

    fn exiting_engagements<'a>(
        &'a mut self,
        company: &'a CompanyId,
        position: &'a str,
    ) -> OrganizationFuture<'a, Vec<String>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT id FROM hr_engagements WHERE company_id=$1 AND position_id=$2 \
                 AND status='active' ORDER BY id",
            )
            .bind(company.as_str())
            .bind(position)
            .fetch_all(&mut *self.connection)
            .await?)
        })
    }

    fn persist<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        id: Option<&'a str>,
        input: &'a PositionInput,
        key: &'a str,
        guard: &'a PositionGraphGuard,
        roster_required: bool,
        _exiting: &'a [String],
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            self.authorize(actor, now).await?;
            let company = actor.command.company_id.clone();
            self.require_graph(&company, guard).await?;
            if let Some(id) = id {
                self.require_version(&company, id, input.expected_version)
                    .await?;
            }
            let record_id = match id {
                Some(id) => id.to_owned(),
                None => format!("position-{key}"),
            };
            let version = input.expected_version + 1;
            let rows = sqlx::query(
                "INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,\
                 reports_to_position_id,is_ceo,roster_required,active,version,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$13) \
                 ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,\
                 title=excluded.title,level_id=excluded.level_id,reports_to_position_id=excluded.reports_to_position_id,\
                 is_ceo=excluded.is_ceo,roster_required=excluded.roster_required,active=excluded.active,\
                 version=excluded.version,updated_at=excluded.updated_at \
                 WHERE organization_positions.company_id=excluded.company_id",
            )
            .bind(&record_id)
            .bind(company.as_str())
            .bind(&input.branch_id)
            .bind(&input.department_id)
            .bind(&input.title)
            .bind(&input.level_id)
            .bind(&input.reports_to_position_id)
            .bind(if input.is_ceo { 1_i64 } else { 0_i64 })
            .bind(if roster_required { 1_i64 } else { 0_i64 })
            .bind(if input.active { 1_i64 } else { 0_i64 })
            .bind(version)
            .bind(actor.command.user_id.as_str())
            .bind(now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if rows != 1 {
                return Err(record_changed().into());
            }
            let updated = sqlx::query(
                "UPDATE organization_positions SET requirements=$1 WHERE id=$2 AND company_id=$3",
            )
            .bind(&input.requirements)
            .bind(&record_id)
            .bind(company.as_str())
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated != 1 {
                return Err(record_changed().into());
            }
            let result = json!({"id": record_id, "version": version});
            let hash = fingerprint(actor, "organization.position.save", &record_id, input)?;
            self.save_receipt(
                actor,
                key,
                "organization.position.save",
                &record_id,
                &hash,
                &result,
                version,
                now,
            )
            .await?;
            Ok(result)
        })
    }
}
