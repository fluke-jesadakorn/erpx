//! sqlx execution for the Messaging people picker. This is the only module in
//! the slice that owns SQL text or touches a pool. The Company and clock come
//! from composition's authenticated Holding selection; the HTTP caller can
//! supply only a bounded search string and an optional team identifier.

use crate::application::messaging_people::{MessagingPeopleFuture, MessagingPeopleRead};
use crate::domain::messaging_people::PersonRow;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Exact active-account predicate used by the retained people picker: Company
/// fence, active assignment at the caller's clock and optional team membership.
pub const SQL_ACTIVE_PEOPLE: &str =
    "SELECT p.user_id,COALESCE(NULLIF(p.preferred_name,''),p.legal_name) AS name, \
        COALESCE(p.email,'') AS email \
     FROM hr_people p \
     WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL \
       AND p.user_id IS NOT NULL \
       AND EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=p.company_id AND a.user_id=p.user_id \
         AND a.revoked_at IS NULL AND a.valid_from<=$2 AND a.valid_until>$2 \
         AND iam_assignment_active(a.id,$2) AND iam_principal_active(a.user_id,a.company_id,$2)) \
       AND ($3::text IS NULL OR EXISTS(SELECT 1 FROM messaging_team_members tm \
         WHERE tm.team_id=$3 AND tm.user_id=p.user_id)) \
     ORDER BY name,p.user_id";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MESSAGING_PEOPLE_READ_UNAVAILABLE",
        "The native Messaging people reader is unavailable.",
    )
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
        team_id: Option<&'a str>,
    ) -> MessagingPeopleFuture<'a, Vec<PersonRow>> {
        let pool = self.pool.clone();
        let company_id = company_id.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(SQL_ACTIVE_PEOPLE.to_owned()))
                .bind(company_id.as_str())
                .bind(now)
                .bind(team_id)
                .fetch_all(pool.raw())
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
        })
    }
}
