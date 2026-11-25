//! Native Support incident by-id read adapter. Support owns the incident row;
//! this port resolves exactly one scoped current record into the bounded
//! projection used by the SELECT-only read tail. It opens no transaction and
//! never introduces a writer or a generic Support command.
use crate::application::incident::{IncidentRead, IncidentReadPorts};
use crate::domain::incident::IncidentActor;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::PgPool;

const MAX_ID: usize = 200;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPORT_INCIDENT_READ_UNAVAILABLE",
        "The native Support incident reader is unavailable.",
    )
}

/// Reject an unbounded or malformed id before it reaches SQL. Identifiers are
/// opaque ASCII tokens; callers never supply SQL syntax or a second selector.
pub fn scoped_incident_id(id: &str) -> Result<&str, BusinessError> {
    if id.is_empty()
        || id.len() > MAX_ID
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid(
            "SUPPORT_INCIDENT_ID_INVALID",
            "Provide a valid incident id.",
        ));
    }
    Ok(id)
}

/// The selected current Company/branch/department. `None` on branch or
/// department means the whole Company; scope filtering stays in SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncidentReadScope {
    pub company: CompanyId,
    pub branch: Option<String>,
    pub department: Option<String>,
}

/// Native by-id read. The caller's actor is checked against the reader's bound
/// scope, then one row is selected under that scope and projected to five
/// fields. Absence is `Ok(None)` so the use case owns the not-found error.
pub async fn read_incident(
    pool: &PgPool,
    scope: &IncidentReadScope,
    actor: &IncidentActor,
    id: &str,
) -> Result<Option<IncidentRead>, BusinessError> {
    let id = scoped_incident_id(id)?;
    if actor.company_id != scope.company.as_str()
        || actor.branch_id.as_deref() != scope.branch.as_deref()
        || actor.department_id.as_deref() != scope.department.as_deref()
    {
        return Ok(None);
    }
    let raw: Option<(String, i64, String, String)> = sqlx::query_as(
        "SELECT id,version,status,title FROM support_incidents \
         WHERE id=$1 AND company_id=$2 \
         AND ($3::text IS NULL OR branch_id=$3) \
         AND ($4::text IS NULL OR department_id=$4)",
    )
    .bind(id)
    .bind(scope.company.as_str())
    .bind(&scope.branch)
    .bind(&scope.department)
    .fetch_optional(pool)
    .await
    .map_err(|_| unavailable())?;
    let Some((id, version, status, title)) = raw else {
        return Ok(None);
    };
    Ok(Some(IncidentRead {
        href: format!("/support/incidents/{id}"),
        id,
        version,
        status,
        title,
    }))
}

/// Reader bound to one selected context, ready as an `IncidentReadPorts`.
#[derive(Clone, Debug)]
pub struct IncidentReader {
    pool: PgPool,
    scope: IncidentReadScope,
}

impl IncidentReader {
    pub fn new(pool: PgPool, scope: IncidentReadScope) -> Self {
        Self { pool, scope }
    }
}

impl IncidentReadPorts for IncidentReader {
    async fn authorize(&self, actor: &IncidentActor) -> Result<(), BusinessError> {
        if !actor.permissions.can_read {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "SUPPORT_INCIDENT_READ_REQUIRED",
                "Choose a role that can read Support incidents.",
            ));
        }
        if actor.company_id != self.scope.company.as_str() {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "SUPPORT_INCIDENT_NOT_FOUND",
                "Incident not found in this scope.",
            ));
        }
        Ok(())
    }

    async fn incident(
        &self,
        actor: &IncidentActor,
        id: &str,
    ) -> Result<Option<IncidentRead>, BusinessError> {
        read_incident(&self.pool, &self.scope, actor, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::incident::IncidentPermissions;

    fn actor(company: &str, can_read: bool) -> IncidentActor {
        IncidentActor {
            user_id: "manager".into(),
            company_id: company.into(),
            branch_id: None,
            department_id: None,
            permissions: IncidentPermissions {
                can_read,
                can_work: false,
                can_manage: false,
            },
            same_person_ids: vec![],
        }
    }

    fn reader() -> IncidentReader {
        use sqlx::postgres::PgPoolOptions;
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgresql://erpx:erpx@127.0.0.1:5432/erpx_support_test")
            .expect("lazy pool for authority-only test");
        IncidentReader::new(
            pool,
            IncidentReadScope {
                company: CompanyId::new("company").expect("company id"),
                branch: None,
                department: None,
            },
        )
    }

    #[test]
    fn accepts_only_bounded_opaque_identifiers() {
        assert_eq!(scoped_incident_id("incident-1").unwrap(), "incident-1");
        assert_eq!(scoped_incident_id("a_b-2").unwrap(), "a_b-2");
        for bad in ["", "has space", "a/b", "a'b", "x;drop", &"x".repeat(201)] {
            assert_eq!(
                scoped_incident_id(bad).unwrap_err().code,
                "SUPPORT_INCIDENT_ID_INVALID"
            );
        }
    }

    #[tokio::test]
    async fn reader_authorize_fails_closed_on_permission_and_company() {
        let reader = reader();
        reader.authorize(&actor("company", true)).await.unwrap();
        assert_eq!(
            reader
                .authorize(&actor("company", false))
                .await
                .unwrap_err()
                .code,
            "SUPPORT_INCIDENT_READ_REQUIRED"
        );
        assert_eq!(
            reader
                .authorize(&actor("other", true))
                .await
                .unwrap_err()
                .code,
            "SUPPORT_INCIDENT_NOT_FOUND"
        );
    }
}
