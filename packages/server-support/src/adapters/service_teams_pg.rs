//! Production service-team owner adapter.
//!
//! Organization/People/IAM facts and the Support owner rows are read and written
//! on the caller's serializable Holding connection. No composition module or
//! second team writer is used.
use crate::adapters::service_teams::{
    apply_plan, authority_snapshot, company_member, current_team, organization_node,
    overlapping_teams, service_member, service_team_options, validate_service_scope,
};
use crate::adapters::support_command_receipt::{
    SupportCommandReceipt, replay_support_receipt, retain_support_receipt,
};
use crate::application::service_teams::{
    OrgNode, RecordScope, ServiceTeamActor, ServiceTeamPlan, ServiceTeamPorts, ServiceTeamRow,
    in_service_workspace,
};
use crate::domain::service_teams::Coverage;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const CATEGORIES: &[&str] = &[
    "it",
    "engineering",
    "maintenance",
    "facilities",
    "housekeeping",
    "people",
    "finance",
    "purchasing",
    "operations",
    "sales",
];

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "SERVICE_TEAMS_UNAVAILABLE",
            "Service team storage is temporarily unavailable.",
        ),
    }
}

pub struct PgServiceTeams<'a> {
    connection: &'a mut PgConnection,
    authority_json: String,
    now: i64,
}

impl<'a> PgServiceTeams<'a> {
    pub fn new(
        connection: &'a mut PgConnection,
        authority_json: impl Into<String>,
        now: i64,
    ) -> Self {
        Self {
            connection,
            authority_json: authority_json.into(),
            now,
        }
    }

    pub async fn lock_sources(&mut self) -> Result<(), BusinessError> {
        crate::adapters::service_teams::lock_service_team_sources(self.connection)
            .await
            .map_err(business)
    }

    pub async fn options(&mut self, actor: &ServiceTeamActor) -> Result<Value, BusinessError> {
        self.require_owner(actor).await?;
        let members = self.member_directory(&actor.company_id).await?;
        service_team_options(self.connection, actor, members)
            .await
            .map_err(business)
    }

    pub async fn list(&mut self, actor: &ServiceTeamActor) -> Result<Value, BusinessError> {
        self.require_owner(actor).await?;
        let rows = sqlx::query(
            "SELECT id,name,active,version,configuration_json FROM service_teams \
             WHERE company_id=$1 ORDER BY name,id",
        )
        .bind(&actor.company_id)
        .fetch_all(&mut *self.connection)
        .await
        .map_err(Error::from)
        .map_err(business)?;
        let teams = rows
            .into_iter()
            .map(|row| {
                Ok::<_, Error>(json!({
                    "id": row.try_get::<String, _>("id")?,
                    "name": row.try_get::<String, _>("name")?,
                    "active": row.try_get::<i64, _>("active")? == 1,
                    "version": row.try_get::<i64, _>("version")?,
                    "configuration_json": row.try_get::<String, _>("configuration_json")?,
                }))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(business)?;
        Ok(json!({"teams": teams}))
    }

    async fn member_directory(&mut self, company_id: &str) -> Result<Value, BusinessError> {
        let raw: String = sqlx::query_scalar(
            "SELECT coalesce(jsonb_agg(jsonb_build_object(\
               'id',m.user_id,'name',coalesce(p.preferred_name,p.legal_name),\
               'legalName',p.legal_name) ORDER BY p.legal_name,m.user_id),'[]'::jsonb)::text \
             FROM (SELECT DISTINCT user_id FROM assignments WHERE company_id=$1 \
               AND revoked_at IS NULL \
               AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
               AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)) m \
             JOIN hr_people p ON p.user_id=m.user_id AND p.company_id=$1 \
               AND p.status='active' AND p.merged_into_id IS NULL \
             WHERE EXISTS(SELECT 1 FROM hr_engagements e \
               WHERE e.person_id=p.id AND e.company_id=p.company_id \
                 AND e.status='active' \
                 AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
                 AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))",
        )
        .bind(company_id)
        .fetch_one(&mut *self.connection)
        .await
        .map_err(Error::from)
        .map_err(business)?;
        serde_json::from_str(&raw).map_err(|_| {
            BusinessError::invalid(
                "SERVICE_TEAM_MEMBERS_INVALID",
                "The People member directory is unavailable.",
            )
        })
    }

    async fn require_owner(&mut self, actor: &ServiceTeamActor) -> Result<(), BusinessError> {
        let owner = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM assignments a WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 \
             AND (a.role='holding_owner' OR iam_role_allows(a.role,a.company_id,'company.owner')) \
             AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND (a.scope_kind='holding' OR a.scope_kind='branch' OR a.scope_kind='department')",
        )
        .bind(&actor.assignment_id)
        .bind(&actor.user_id)
        .bind(&actor.company_id)
        .fetch_optional(&mut *self.connection)
        .await
        .map_err(Error::from)
        .map_err(business)?;
        // The caller supplies the selected current assignment in composition;
        // this owner adapter does not trust a role supplied on the wire.
        if owner.is_none() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "OWNER_REQUIRED",
                "A company owner must configure service teams and handbook editors.",
            ));
        }
        Ok(())
    }
}

impl ServiceTeamPorts for PgServiceTeams<'_> {
    fn now(&self) -> i64 {
        self.now
    }

    fn categories(&self) -> &[&'static str] {
        CATEGORIES
    }

    async fn authorize_owner(&mut self, actor: &ServiceTeamActor) -> Result<(), BusinessError> {
        if !actor.holding_owner {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "OWNER_REQUIRED",
                "A company owner must configure service teams and handbook editors.",
            ));
        }
        self.require_owner(actor).await
    }

    async fn current(
        &mut self,
        actor: &ServiceTeamActor,
        id: &str,
    ) -> Result<Option<ServiceTeamRow>, BusinessError> {
        current_team(self.connection, actor, id)
            .await
            .map_err(business)
    }

    async fn company_member(
        &mut self,
        company_id: &str,
        user_id: &str,
    ) -> Result<bool, BusinessError> {
        company_member(self.connection, company_id, user_id)
            .await
            .map_err(business)
    }

    async fn node(&mut self, company_id: &str, id: &str) -> Result<Option<OrgNode>, BusinessError> {
        organization_node(self.connection, company_id, id)
            .await
            .map_err(business)
    }

    async fn validate_scope(
        &mut self,
        actor: &ServiceTeamActor,
        scope: &RecordScope,
    ) -> Result<(), BusinessError> {
        if !in_service_workspace(actor, scope) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "REQUEST_SCOPE_FORBIDDEN",
                "Choose a location within your authorized workspace.",
            ));
        }
        validate_service_scope(self.connection, actor, scope)
            .await
            .map_err(business)
    }

    async fn scope_member(
        &mut self,
        scope: &RecordScope,
        user_id: &str,
    ) -> Result<bool, BusinessError> {
        service_member(self.connection, scope, user_id)
            .await
            .map_err(business)
    }

    async fn authority_snapshot(&mut self, company_id: &str) -> Result<String, BusinessError> {
        authority_snapshot(self.connection, company_id)
            .await
            .map_err(business)
    }

    async fn replay(
        &mut self,
        actor: &ServiceTeamActor,
        key: &str,
        operation: &str,
        id: &str,
        input: &Value,
    ) -> Result<Option<Value>, BusinessError> {
        let hash = crate::adapters::support_command_receipt::support_request_hash(
            &actor.user_id,
            &actor.assignment_id,
            &actor.role,
            &actor.company_id,
            actor.branch_id.as_deref(),
            actor.department_id.as_deref(),
            operation,
            id,
            input,
        );
        replay_support_receipt(self.connection, key, operation, &actor.user_id, id, &hash)
            .await
            .map_err(business)
    }

    async fn overlaps(
        &mut self,
        actor: &ServiceTeamActor,
        id: Option<&str>,
        coverage: &Coverage,
    ) -> Result<Vec<String>, BusinessError> {
        overlapping_teams(self.connection, actor, id, coverage)
            .await
            .map_err(business)
    }

    async fn commit(
        &mut self,
        actor: &ServiceTeamActor,
        key: &str,
        plan: &ServiceTeamPlan,
        input: &Value,
        result: &Value,
    ) -> Result<(), BusinessError> {
        apply_plan(self.connection, actor, plan, self.now)
            .await
            .map_err(business)?;
        let receipt = SupportCommandReceipt {
            key,
            actor_id: &actor.user_id,
            assignment_id: &actor.assignment_id,
            role: &actor.role,
            company_id: &actor.company_id,
            branch_id: actor.branch_id.as_deref(),
            department_id: actor.department_id.as_deref(),
            operation: plan.operation,
            record_id: &plan.id,
            record_type: "service_team",
            aggregate_type: "service_team",
            version: plan.version,
            state: if plan.input.active {
                "active"
            } else {
                "inactive"
            },
            input,
            result,
            authority_json: &self.authority_json,
            created_at: self.now,
        };
        retain_support_receipt(self.connection, &receipt)
            .await
            .map_err(business)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_match_the_employee_service_catalogue() {
        assert_eq!(
            CATEGORIES,
            &[
                "it",
                "engineering",
                "maintenance",
                "facilities",
                "housekeeping",
                "people",
                "finance",
                "purchasing",
                "operations",
                "sales"
            ]
        );
    }
}
