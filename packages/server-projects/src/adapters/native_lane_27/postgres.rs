//! sqlx execution for the lane 27 Projects actuals read. The statements start
//! from the byte-identical TS statements (`actuals.ts` plus the Finance-owned
//! fixed join fragments); only the projection is made explicit and the money
//! calculation is moved into the Rust domain.

use super::application::{ActualsFuture, ProjectActualsRead};
use super::domain::{
    ActualsScope, CostAllocationRow, HandoffProject, HandoffRow, HandoffScope, HandoffWorkRow,
    ProjectRow, RoutineProjectOption, WorkLogRow, minor_to_decimal, unavailable,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};

/// The exact work-log statement. Bind order stays user,user,user,company,
/// project,user,user as in the replaced TypeScript reader.
const WORK_LOGS: &str = "SELECT l.id,l.project_id,l.user_id,l.work_date::text work_date,l.minutes,l.state,l.version,l.reviewed_by,\
     l.rejection_reason,l.created_at,l.updated_at,\
     iam_same_person(l.user_id,$1,p.company_id) is_actor,\
     (iam_same_person(p.owner_user_id,$2,p.company_id) AND NOT iam_same_person(l.user_id,$3,p.company_id)) can_review\
     FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id\
     WHERE l.company_id=$4 AND l.project_id=$5 AND (iam_same_person(p.owner_user_id,$6,p.company_id) OR iam_same_person(l.user_id,$7,p.company_id))\
     ORDER BY l.work_date DESC,l.id";

/// The exact cost-allocation statement. The original Finance join fragments
/// are inlined; company isolation is added on the two joined owner tables using
/// the same company bind so no new bind is introduced.
const COST_ALLOCATIONS: &str = "SELECT a.id,a.project_id,a.journal_line_id,a.amount_minor::text amount_minor,a.state,a.version,\
     a.created_by,a.approved_by,a.rejection_reason,a.created_at,a.updated_at,j.id journal_id,j.journal_number,\
     NOT iam_same_person(a.created_by,$1,a.company_id) can_review,\
     iam_same_person(a.created_by,$2,a.company_id) is_creator,\
     j.entry_date::text entry_date,j.currency,j.status journal_status,ac.code account_code,ac.name account_name\
     FROM staff_project_cost_allocations a JOIN finance_journal_lines l ON l.id=a.journal_line_id\
     JOIN finance_journals j ON j.id=l.journal_id AND j.company_id=$3\
     JOIN finance_accounts ac ON ac.id=l.account_id AND ac.company_id=$3\
     WHERE a.company_id=$3 AND a.project_id=$4 ORDER BY j.entry_date DESC,j.journal_number DESC,a.id";

/// Byte-identical handoff list statement with the `SELECT *` projection made
/// explicit so `snapshot_json` is transported as text.
const HANDOFFS: &str = "SELECT id,company_id,project_id,source_project_version,recipient_id,status,version,snapshot_json::text snapshot_json,snapshot_sha256,evidence,prepared_by,prepared_at,decided_by,decided_at,decision_reason\
     FROM staff_project_handoffs WHERE company_id=$1 AND project_id=$2 ORDER BY prepared_at DESC,id";

/// The closed r33 point-read head with `project_id` as its only projection.
const HANDOFF_PROJECT: &str = "SELECT project_id FROM staff_project_handoffs WHERE id=$1 AND company_id=$2";

/// Byte-identical handoff work statement; the owner-or-member scope expands the
/// shared Project fragment with the exact bind order company,user,user,user,user.
const HANDOFF_WORK: &str = "SELECT h.id,h.version,h.project_id,h.recipient_id,h.prepared_by,h.status,h.prepared_at,h.decided_at,p.title,p.lifecycle_status \
     FROM staff_project_handoffs h JOIN staff_projects p ON p.id=h.project_id AND p.company_id=h.company_id \
     WHERE h.company_id=$1 AND (iam_same_person(h.recipient_id,$2,h.company_id) OR iam_same_person(h.prepared_by,$3,h.company_id)) \
       AND (p.owner_user_id=$4 OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=$5)) \
     ORDER BY COALESCE(h.decided_at,h.prepared_at) DESC,h.id LIMIT 200";

/// The former byte-identical visible-row heads (`read-tail-r30.ts` plus the
/// owner-or-member scope from `project-read-projections.ts`) with `?` bound as
/// `$n` and an explicit fixed projection. Bind order stays id,company,user,user
/// for the point read and company,user,user for the list.
const VISIBLE_PROJECT: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.description,p.owner_user_id,p.lifecycle_status,p.version,p.updated_at\
     FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2\
       AND (p.owner_user_id=$3 OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=$4))";

const VISIBLE_PROJECTS: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.description,p.owner_user_id,p.lifecycle_status,p.version,p.updated_at\
     FROM staff_projects p WHERE p.company_id=$1\
       AND (p.owner_user_id=$2 OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=$3))\
     ORDER BY p.updated_at DESC";

/// The narrow routine picker statement: the exact `listProjects`
/// Company/owner-or-member predicate and `updated_at DESC` order, projecting
/// only `{id,title}`. It adds no privilege, ordering or cross-owner policy.
const ROUTINE_PROJECT_OPTIONS: &str = "SELECT p.id,p.title \
      FROM staff_projects p WHERE p.company_id=$1 \
        AND (p.owner_user_id=$2 OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=$3)) \
      ORDER BY p.updated_at DESC";

pub struct PgProjectActualsRead {
    pool: DatabasePool,
}

impl PgProjectActualsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn routine_project_options(
        connection: &mut PgConnection,
        company_id: &str,
        user_id: &str,
    ) -> Result<Vec<RoutineProjectOption>, BusinessError> {
        let rows = sqlx::query_as::<_, (String, String)>(ROUTINE_PROJECT_OPTIONS)
            .bind(company_id)
            .bind(user_id)
            .bind(user_id)
            .fetch_all(connection)
            .await
            .map_err(|_| unavailable())?;
        Ok(rows
            .into_iter()
            .map(|(id, title)| RoutineProjectOption { id, title })
            .collect())
    }
}

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, sqlx::Error> {
    row.try_get::<String, _>(column)
}

fn opt_text(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, sqlx::Error> {
    row.try_get::<Option<String>, _>(column)
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, sqlx::Error> {
    row.try_get::<i64, _>(column)
}

fn boolean(row: &sqlx::postgres::PgRow, column: &str) -> Result<bool, sqlx::Error> {
    row.try_get::<bool, _>(column)
}

impl ProjectActualsRead for PgProjectActualsRead {
    fn work_logs<'a>(&'a self, scope: &'a ActualsScope) -> ActualsFuture<'a, Vec<WorkLogRow>> {
        Box::pin(async move {
            let rows = sqlx::query(WORK_LOGS)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.company)
                .bind(&scope.project)
                .bind(&scope.user)
                .bind(&scope.user)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(WorkLogRow {
                        id: text(&row, "id")?,
                        project_id: text(&row, "project_id")?,
                        user_id: text(&row, "user_id")?,
                        work_date: text(&row, "work_date")?,
                        minutes: integer(&row, "minutes")?,
                        state: text(&row, "state")?,
                        version: integer(&row, "version")?,
                        reviewed_by: opt_text(&row, "reviewed_by")?,
                        rejection_reason: text(&row, "rejection_reason")?,
                        created_at: integer(&row, "created_at")?,
                        updated_at: integer(&row, "updated_at")?,
                        is_actor: boolean(&row, "is_actor")?,
                        can_review: boolean(&row, "can_review")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(|_| unavailable())
        })
    }

    fn cost_allocations<'a>(
        &'a self,
        scope: &'a ActualsScope,
    ) -> ActualsFuture<'a, Vec<CostAllocationRow>> {
        Box::pin(async move {
            let rows = sqlx::query(COST_ALLOCATIONS)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.company)
                .bind(&scope.project)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    let amount_minor = text(&row, "amount_minor")?;
                    let amount = minor_to_decimal(&amount_minor).map_err(|_| {
                        sqlx::Error::Decode("PROJECT_ACTUALS_AMOUNT_INVALID".into())
                    })?;
                    Ok(CostAllocationRow {
                        id: text(&row, "id")?,
                        project_id: text(&row, "project_id")?,
                        journal_line_id: text(&row, "journal_line_id")?,
                        amount_minor,
                        amount,
                        state: text(&row, "state")?,
                        version: integer(&row, "version")?,
                        created_by: text(&row, "created_by")?,
                        approved_by: opt_text(&row, "approved_by")?,
                        rejection_reason: text(&row, "rejection_reason")?,
                        created_at: integer(&row, "created_at")?,
                        updated_at: integer(&row, "updated_at")?,
                        journal_id: text(&row, "journal_id")?,
                        journal_number: text(&row, "journal_number")?,
                        entry_date: text(&row, "entry_date")?,
                        currency: text(&row, "currency")?,
                        journal_status: text(&row, "journal_status")?,
                        account_code: text(&row, "account_code")?,
                        account_name: text(&row, "account_name")?,
                        can_review: boolean(&row, "can_review")?,
                        is_creator: boolean(&row, "is_creator")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(|_| unavailable())
        })
    }

    fn handoffs<'a>(&'a self, scope: &'a HandoffScope) -> ActualsFuture<'a, Vec<HandoffRow>> {
        Box::pin(async move {
            let rows = sqlx::query(HANDOFFS)
                .bind(&scope.company)
                .bind(&scope.project)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(HandoffRow {
                        id: text(&row, "id")?,
                        company_id: text(&row, "company_id")?,
                        project_id: text(&row, "project_id")?,
                        source_project_version: integer(&row, "source_project_version")?,
                        recipient_id: text(&row, "recipient_id")?,
                        status: text(&row, "status")?,
                        version: integer(&row, "version")?,
                        snapshot_json: text(&row, "snapshot_json")?,
                        snapshot_sha256: text(&row, "snapshot_sha256")?,
                        evidence: text(&row, "evidence")?,
                        prepared_by: text(&row, "prepared_by")?,
                        prepared_at: integer(&row, "prepared_at")?,
                        decided_by: opt_text(&row, "decided_by")?,
                        decided_at: row.try_get::<Option<i64>, _>("decided_at")?,
                        decision_reason: opt_text(&row, "decision_reason")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(|_| unavailable())
        })
    }

    fn handoff_project<'a>(
        &'a self,
        scope: &'a HandoffScope,
    ) -> ActualsFuture<'a, Option<HandoffProject>> {
        Box::pin(async move {
            let row = sqlx::query(HANDOFF_PROJECT)
                .bind(&scope.handoff)
                .bind(&scope.company)
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| -> Result<HandoffProject, sqlx::Error> {
                Ok(HandoffProject {
                    project_id: text(&row, "project_id")?,
                })
            })
            .transpose()
            .map_err(|_| unavailable())
        })
    }

    fn handoff_work<'a>(
        &'a self,
        scope: &'a HandoffScope,
    ) -> ActualsFuture<'a, Vec<HandoffWorkRow>> {
        Box::pin(async move {
            let rows = sqlx::query(HANDOFF_WORK)
                .bind(&scope.company)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.user)
                .bind(&scope.user)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(HandoffWorkRow {
                        id: text(&row, "id")?,
                        version: integer(&row, "version")?,
                        project_id: text(&row, "project_id")?,
                        recipient_id: text(&row, "recipient_id")?,
                        prepared_by: text(&row, "prepared_by")?,
                        status: text(&row, "status")?,
                        prepared_at: integer(&row, "prepared_at")?,
                        decided_at: row.try_get::<Option<i64>, _>("decided_at")?,
                        title: text(&row, "title")?,
                        lifecycle_status: text(&row, "lifecycle_status")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(|_| unavailable())
        })
    }

    fn visible_project<'a>(
        &'a self,
        scope: &'a ActualsScope,
    ) -> ActualsFuture<'a, Option<ProjectRow>> {
        Box::pin(async move {
            let row = sqlx::query(VISIBLE_PROJECT)
                .bind(&scope.project)
                .bind(&scope.company)
                .bind(&scope.user)
                .bind(&scope.user)
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| -> Result<ProjectRow, sqlx::Error> {
                Ok(ProjectRow {
                    id: text(&row, "id")?,
                    company_id: text(&row, "company_id")?,
                    branch_id: opt_text(&row, "branch_id")?,
                    department_id: opt_text(&row, "department_id")?,
                    title: text(&row, "title")?,
                    description: text(&row, "description")?,
                    owner_user_id: text(&row, "owner_user_id")?,
                    lifecycle_status: text(&row, "lifecycle_status")?,
                    version: integer(&row, "version")?,
                    updated_at: integer(&row, "updated_at")?,
                })
            })
            .transpose()
            .map_err(|_| unavailable())
        })
    }

    fn visible_projects<'a>(
        &'a self,
        scope: &'a ActualsScope,
    ) -> ActualsFuture<'a, Vec<ProjectRow>> {
        Box::pin(async move {
            let rows = sqlx::query(VISIBLE_PROJECTS)
                .bind(&scope.company)
                .bind(&scope.user)
                .bind(&scope.user)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(ProjectRow {
                        id: text(&row, "id")?,
                        company_id: text(&row, "company_id")?,
                        branch_id: opt_text(&row, "branch_id")?,
                        department_id: opt_text(&row, "department_id")?,
                        title: text(&row, "title")?,
                        description: text(&row, "description")?,
                        owner_user_id: text(&row, "owner_user_id")?,
                        lifecycle_status: text(&row, "lifecycle_status")?,
                        version: integer(&row, "version")?,
                        updated_at: integer(&row, "updated_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(|_| unavailable())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_are_owner_local_and_company_scoped() {
        assert!(WORK_LOGS.contains("FROM staff_project_work_logs l JOIN staff_projects p"));
        assert!(WORK_LOGS.contains("l.company_id=$4"));
        assert!(COST_ALLOCATIONS.contains("FROM staff_project_cost_allocations a"));
        assert!(COST_ALLOCATIONS.contains("a.company_id=$3"));
        assert!(COST_ALLOCATIONS.contains("j.company_id=$3"));
        assert!(COST_ALLOCATIONS.contains("ac.company_id=$3"));
    }

    #[test]
    fn sql_keeps_the_original_order_and_bind_arity() {
        assert!(WORK_LOGS.ends_with("ORDER BY l.work_date DESC,l.id"));
        assert!(WORK_LOGS.contains("$7"));
        assert!(!WORK_LOGS.contains("$8"));
        assert!(COST_ALLOCATIONS.ends_with("ORDER BY j.entry_date DESC,j.journal_number DESC,a.id"));
        assert!(COST_ALLOCATIONS.contains("$4"));
        assert!(!COST_ALLOCATIONS.contains("$5"));
    }

    #[test]
    fn handoff_statements_keep_relations_order_and_bind_arity() {
        assert!(HANDOFFS.ends_with("ORDER BY prepared_at DESC,id"));
        assert!(HANDOFFS.contains("snapshot_json::text snapshot_json"));
        assert!(HANDOFFS.contains("company_id=$1 AND project_id=$2"));
        assert!(HANDOFF_PROJECT.ends_with("WHERE id=$1 AND company_id=$2"));
        assert!(HANDOFF_WORK.ends_with("ORDER BY COALESCE(h.decided_at,h.prepared_at) DESC,h.id LIMIT 200"));
        assert!(HANDOFF_WORK.contains("p.owner_user_id=$4"));
        assert!(HANDOFF_WORK.contains("m.user_id=$5"));
        assert!(!HANDOFF_WORK.contains("$6"));
    }

    #[test]
    fn visible_project_statements_keep_owner_member_scope_and_bind_order() {
        assert!(VISIBLE_PROJECT.contains("p.id=$1 AND p.company_id=$2"));
        assert!(VISIBLE_PROJECT.contains("p.owner_user_id=$3"));
        assert!(VISIBLE_PROJECT.contains("m.user_id=$4"));
        assert!(!VISIBLE_PROJECT.contains("$5"));
        assert!(VISIBLE_PROJECTS.contains("p.company_id=$1"));
        assert!(VISIBLE_PROJECTS.contains("p.owner_user_id=$2"));
        assert!(VISIBLE_PROJECTS.contains("m.user_id=$3"));
        assert!(VISIBLE_PROJECTS.ends_with("ORDER BY p.updated_at DESC"));
        for sql in [VISIBLE_PROJECT, VISIBLE_PROJECTS] {
            assert!(sql.contains("p.lifecycle_status"));
            assert!(!sql.contains("SELECT p.*"));
        }
    }

    #[test]
    fn routine_picker_statement_keeps_the_list_projects_visibility_and_order() {
        let predicate = "AND (p.owner_user_id=$2 OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=$3))";
        assert!(
            ROUTINE_PROJECT_OPTIONS
                .starts_with("SELECT p.id,p.title FROM staff_projects p WHERE p.company_id=$1")
        );
        assert!(ROUTINE_PROJECT_OPTIONS.contains(predicate));
        assert!(ROUTINE_PROJECT_OPTIONS.ends_with("ORDER BY p.updated_at DESC"));
        assert!(!ROUTINE_PROJECT_OPTIONS.contains("$4"));
        let normalized = VISIBLE_PROJECTS
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(normalized.contains(predicate));
        assert!(normalized.ends_with("ORDER BY p.updated_at DESC"));
        let upper = ROUTINE_PROJECT_OPTIONS.to_ascii_uppercase();
        for forbidden in ["INSERT ", "UPDATE ", "DELETE ", "LOCK "] {
            assert!(!upper.contains(forbidden));
        }
    }
}
