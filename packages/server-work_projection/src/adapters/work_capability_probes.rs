//! PostgreSQL Work Projection capability probes.
//!
//! These are the owner reads behind `getWorkCapabilities`: active staff
//! roster membership, service-team coverage, inventory preparation and the
//! declared Expense purchase gate. The caller supplies the already-authorized
//! scope and inventory booleans; this module performs no HTTP or session work.
use crate::domain::work_capabilities::{CapabilityProbe, WorkCapabilityFacts, work_capabilities};
use crate::domain::work_card_availability::WorkCapabilitiesFact;
use sqlx::{PgConnection, Row};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkCapabilityScope<'a> {
    pub company_id: &'a str,
    pub user_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub beta: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InventoryPreparation {
    pub can_prepare_counts: bool,
    pub can_prepare_transfers: bool,
}

/// The single active employee/contractor engagement wins. Zero or multiple
/// active engagements map to the TypeScript 409 and therefore to `None` here.
pub async fn active_staff_roster_required(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    now: i64,
) -> Result<Option<bool>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT COALESCE(pos.roster_required,0)=1 roster_required \
         FROM hr_people p \
         JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
         LEFT JOIN organization_positions pos ON pos.id=e.position_id \
           AND pos.company_id=e.company_id AND pos.active=1 \
         WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL \
         AND p.user_id=$2 AND e.status='active' \
         AND e.start_date::date<=current_date \
         AND (e.end_date IS NULL OR e.end_date::date>=current_date) \
         AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=p.user_id \
           AND a.company_id=p.company_id AND a.revoked_at IS NULL \
           AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND a.valid_from<=$3 AND a.valid_until>$4) \
         LIMIT 2",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(now)
    .bind(now)
    .fetch_all(&mut *connection)
    .await?;
    if rows.len() != 1 {
        return Ok(None);
    }
    Ok(Some(rows[0].try_get::<bool, _>("roster_required")?))
}

/// A member is covered when their active team covers the exact workspace.
pub async fn covered_team(
    connection: &mut PgConnection,
    scope: &WorkCapabilityScope<'_>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_team_members m \
         JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id AND t.active=1 \
         JOIN service_team_coverage c ON c.team_id=t.id AND c.company_id=t.company_id \
         JOIN organization_nodes n ON n.id=c.scope_node_id AND n.company_id=c.company_id AND n.status='active' \
         LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.kind='branch' \
         WHERE m.company_id=$1 AND m.user_id=$2 AND ( \
           (c.scope_kind='holding' AND n.kind='company' AND n.id=m.company_id) OR \
           (c.scope_kind='branch' AND n.kind='branch' AND ($3::text IS NULL OR n.id=$3)) OR \
           (c.scope_kind='department' AND n.kind='department' \
             AND (n.branch_id IS NULL OR b.status='active') \
             AND ($4::text IS NULL OR n.branch_id=$4) \
             AND ($5::text IS NULL OR n.id=$5)) \
         ))",
    )
    .bind(scope.company_id)
    .bind(scope.user_id)
    .bind(scope.branch_id)
    .bind(scope.branch_id)
    .bind(scope.department_id)
    .fetch_one(&mut *connection)
    .await
}

/// Reads the probes and applies the exact capability ordering. The Expense
/// gate is the caller's purchased `people.expenses` new-work decision.
pub async fn read_work_capabilities(
    connection: &mut PgConnection,
    scope: &WorkCapabilityScope<'_>,
    now: i64,
    inventory: InventoryPreparation,
    expense_available: bool,
) -> Result<WorkCapabilitiesFact, sqlx::Error> {
    let active_staff =
        active_staff_roster_required(connection, scope.company_id, scope.user_id, now).await?;
    let team = covered_team(connection, scope).await?;
    let facts = WorkCapabilityFacts {
        beta: scope.beta,
        active_staff: CapabilityProbe::Value(active_staff),
        covered_team: CapabilityProbe::Value(team),
        can_prepare_counts: inventory.can_prepare_counts,
        can_prepare_transfers: inventory.can_prepare_transfers,
    };
    let mut result = work_capabilities(&facts);
    if !expense_available {
        result.creation.retain(|kind| kind != "expense");
    }
    Ok(result)
}
