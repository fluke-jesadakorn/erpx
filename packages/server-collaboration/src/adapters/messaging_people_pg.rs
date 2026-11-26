//! sqlx execution for the Messaging people picker. This is the only module in
//! the slice that owns SQL text or touches a pool. The Company, clock and
//! viewer come from composition's authenticated Holding selection; the HTTP
//! caller can supply only a bounded search string and an optional team
//! identifier.
//!
//! A team-filtered read re-applies the retained team visibility rule: a
//! non-managed team is visible to its members, a managed team to current
//! Company-node members. The viewer predicate is the same owner fragment the
//! native room read uses, so the picker cannot enumerate a team the caller
//! cannot see.

use crate::application::messaging_people::{MessagingPeopleFuture, MessagingPeopleRead};
use crate::domain::messaging_people::PersonRow;
use erpx_server_organization::public::organization_nodes::messaging_team_organization_sql;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

/// Exact active-account predicate used by the retained people picker: Company
/// fence, active assignment at the caller's clock, optional team membership and
/// the retained team-visibility guard for the authenticated viewer.
pub fn sql_active_people() -> String {
    format!(
        "SELECT p.user_id,COALESCE(NULLIF(p.preferred_name,''),p.legal_name) AS name, \
            COALESCE(p.email,'') AS email \
         FROM hr_people p \
         WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL \
           AND p.user_id IS NOT NULL \
           AND EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=p.company_id AND a.user_id=p.user_id \
             AND a.revoked_at IS NULL AND a.valid_from<=$2 AND a.valid_until>$2 \
             AND iam_assignment_active(a.id,$2) AND iam_principal_active(a.user_id,a.company_id,$2)) \
           AND ($3::text IS NULL OR ( \
             EXISTS(SELECT 1 FROM messaging_teams t WHERE t.id=$3 AND t.company_id=$1 AND ( \
               (NOT t.organization_managed AND EXISTS(SELECT 1 FROM messaging_team_members viewer \
                 WHERE viewer.team_id=t.id AND viewer.user_id=$4)) \
               OR (t.organization_managed AND {}) \
             )) \
             AND EXISTS(SELECT 1 FROM messaging_team_members tm WHERE tm.team_id=$3 AND tm.user_id=p.user_id) \
           )) \
         ORDER BY name,p.user_id",
        messaging_team_organization_sql("t", "$4")
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MESSAGING_PEOPLE_READ_UNAVAILABLE",
        "The native Messaging people reader is unavailable.",
    )
}

/// Execute the active-people query on the caller's open Holding unit. The
/// caller owns the transaction and passes its authenticated Company/viewer.
pub async fn fetch_active_people(
    connection: &mut PgConnection,
    company_id: &str,
    now: i64,
    viewer_id: &str,
    team_id: Option<&str>,
) -> Result<Vec<PersonRow>, BusinessError> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql_active_people()))
        .bind(company_id)
        .bind(now)
        .bind(team_id)
        .bind(viewer_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    rows.into_iter()
        .map(|row| {
            Ok(PersonRow {
                user_id: row.try_get("user_id").map_err(|_| unavailable())?,
                name: row.try_get("name").map_err(|_| unavailable())?,
                email: row.try_get("email").map_err(|_| unavailable())?,
            })
        })
        .collect()
}

/// Postgres-backed owner reader. Composition supplies the Holding pool.
pub struct PgMessagingPeopleRead {
    pool: DatabasePool,
}

impl PgMessagingPeopleRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl MessagingPeopleRead for PgMessagingPeopleRead {
    fn active<'a>(
        &'a self,
        company_id: &'a str,
        now: i64,
        viewer_id: &'a str,
        team_id: Option<&'a str>,
    ) -> MessagingPeopleFuture<'a, Vec<PersonRow>> {
        let pool = self.pool.clone();
        let company_id = company_id.to_owned();
        let viewer_id = viewer_id.to_owned();
        let team_id = team_id.map(str::to_owned);
        Box::pin(async move {
            let mut connection = pool.raw().acquire().await.map_err(|_| unavailable())?;
            fetch_active_people(
                &mut connection,
                &company_id,
                now,
                &viewer_id,
                team_id.as_deref(),
            )
            .await
        })
    }
}
