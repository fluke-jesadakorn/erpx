//! Application port and handler for the native lane 11 person-identity reads.
//!
//! No SQL lives here. Composition injects the owner reader; the handlers only
//! validate the Company boundary, pin the projection and enforce fail-closed
//! invariants before the transport serialises the response.

use crate::adapters::native_lane_11::domain::{
    DirectoryConfigResponse, IdentityIdsResponse, PrincipalsResponse, RoleIdsResponse,
    RoleResponse, RoleRevision, RolesResponse, SuccessorRow, identity_ids, invalid, principals,
    valid_company, valid_directory_kind, valid_id, validate_ids,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type PersonIdentityFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner read port. `successors` receives `None` for the policy-snapshot
/// form (all active successors in the Company) and `Some(ids)` for the filtered
/// form. The sqlx adapter owns every predicate and the bind order.
pub trait PersonIdentityQuery: Send + Sync {
    fn aliases<'a>(
        &'a self,
        company: &'a str,
        ids: &'a [String],
    ) -> PersonIdentityFuture<'a, Vec<String>>;

    fn successors<'a>(
        &'a self,
        company: &'a str,
        ids: Option<&'a [String]>,
    ) -> PersonIdentityFuture<'a, Vec<SuccessorRow>>;

    /// Owner-local directory configuration read. `kind` is one of the fixed
    /// `DIRECTORY_KINDS`; `can_manage` selects the unrestricted connection
    /// projection. No caller SQL or table name reaches this port.
    fn directory_rows<'a>(
        &'a self,
        company: &'a str,
        kind: &'a str,
        can_manage: bool,
    ) -> PersonIdentityFuture<'a, Vec<Value>>;

    /// Published role catalog reads over IAM-owned `iam_roles` and
    /// `iam_role_revisions`.
    fn role_one<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> PersonIdentityFuture<'a, Option<RoleRevision>>;

    fn role_list<'a>(
        &'a self,
        company: &'a str,
        ids: &'a [String],
    ) -> PersonIdentityFuture<'a, Vec<RoleRevision>>;

    fn role_ids<'a>(&'a self, company: &'a str) -> PersonIdentityFuture<'a, Vec<String>>;
}

pub async fn load_identity_ids(
    query: &dyn PersonIdentityQuery,
    company: &str,
    ids: &[String],
) -> Result<IdentityIdsResponse, BusinessError> {
    if !valid_company(company) {
        return Err(invalid());
    }
    validate_ids(ids)?;
    let aliases = query.aliases(company, ids).await?;
    identity_ids(ids, aliases).validated(ids)
}

pub async fn load_principals(
    query: &dyn PersonIdentityQuery,
    company: &str,
    ids: &[String],
    snapshot: bool,
) -> Result<PrincipalsResponse, BusinessError> {
    if !valid_company(company) {
        return Err(invalid());
    }
    validate_ids(ids)?;
    let rows = query
        .successors(company, if snapshot { None } else { Some(ids) })
        .await?;
    principals(ids, rows).validated(ids)
}

/// Validate the Company and the fixed read kind, then pin the row projection.
pub async fn load_directory_config(
    query: &dyn PersonIdentityQuery,
    company: &str,
    kind: &str,
    can_manage: bool,
) -> Result<DirectoryConfigResponse, BusinessError> {
    if !valid_company(company) || !valid_directory_kind(kind) {
        return Err(invalid());
    }
    let rows = query.directory_rows(company, kind, can_manage).await?;
    DirectoryConfigResponse { rows }.validated()
}

/// Validate one bounded role id and pin the published definition.
pub async fn load_role(
    query: &dyn PersonIdentityQuery,
    company: &str,
    id: &str,
) -> Result<RoleResponse, BusinessError> {
    if !valid_company(company) || !valid_id(id) {
        return Err(invalid());
    }
    let role = query.role_one(company, id).await?.map(RoleRevision::validated).transpose()?;
    Ok(RoleResponse { role })
}

/// Validate the bound list and pin every published definition. Duplicate ids
/// collapse to the first row exactly as the previous Map projection did.
pub async fn load_roles(
    query: &dyn PersonIdentityQuery,
    company: &str,
    ids: &[String],
) -> Result<RolesResponse, BusinessError> {
    if !valid_company(company) {
        return Err(invalid());
    }
    validate_ids(ids)?;
    let rows = query.role_list(company, ids).await?;
    let mut seen = Vec::new();
    let mut roles = Vec::new();
    for row in rows {
        let row = row.validated()?;
        if !seen.contains(&row.id) {
            seen.push(row.id.clone());
            roles.push(row);
        }
    }
    Ok(RolesResponse { roles })
}

pub async fn load_role_ids(
    query: &dyn PersonIdentityQuery,
    company: &str,
) -> Result<RoleIdsResponse, BusinessError> {
    if !valid_company(company) {
        return Err(invalid());
    }
    let ids = query.role_ids(company).await?;
    if ids.len() > 200 || ids.iter().any(|id| !valid_id(id)) {
        return Err(invalid());
    }
    Ok(RoleIdsResponse { ids })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_11::domain::unavailable;
    use serde_json::json;

    struct Stub {
        fail: bool,
        aliases: Vec<String>,
        successors: Vec<SuccessorRow>,
    }

    impl PersonIdentityQuery for Stub {
        fn aliases<'a>(
            &'a self,
            company: &'a str,
            ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<String>> {
            let fail = self.fail;
            let aliases = self.aliases.clone();
            let local = ids.to_vec();
            Box::pin(async move {
                if fail || company != "acme:co" || local.is_empty() {
                    return Err(unavailable());
                }
                Ok(aliases)
            })
        }

        fn successors<'a>(
            &'a self,
            company: &'a str,
            ids: Option<&'a [String]>,
        ) -> PersonIdentityFuture<'a, Vec<SuccessorRow>> {
            let fail = self.fail;
            let rows = self.successors.clone();
            let local = ids.map(<[String]>::to_vec);
            Box::pin(async move {
                if fail || company != "acme:co" || local.as_deref().is_some_and(<[String]>::is_empty) {
                    return Err(unavailable());
                }
                Ok(rows)
            })
        }

        fn directory_rows<'a>(
            &'a self,
            company: &'a str,
            _kind: &'a str,
            _can_manage: bool,
        ) -> PersonIdentityFuture<'a, Vec<Value>> {
            let fail = self.fail;
            let local = company.to_owned();
            Box::pin(async move {
                if fail || local != "acme:co" {
                    return Err(unavailable());
                }
                Ok(vec![json!({"id": "conn-1", "name": "Directory"})])
            })
        }

        fn role_one<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
        ) -> PersonIdentityFuture<'a, Option<RoleRevision>> {
            let fail = self.fail;
            let local = (company.to_owned(), id.to_owned());
            Box::pin(async move {
                if fail || local.0 != "acme:co" {
                    return Err(unavailable());
                }
                if local.1 != "employee" {
                    return Ok(None);
                }
                Ok(Some(role("employee".into())))
            })
        }

        fn role_list<'a>(
            &'a self,
            company: &'a str,
            ids: &'a [String],
        ) -> PersonIdentityFuture<'a, Vec<RoleRevision>> {
            let fail = self.fail;
            let local = (company.to_owned(), ids.to_vec());
            Box::pin(async move {
                if fail || local.0 != "acme:co" || local.1.is_empty() {
                    return Err(unavailable());
                }
                Ok(local.1.into_iter().map(role).collect())
            })
        }

        fn role_ids<'a>(
            &'a self,
            company: &'a str,
        ) -> PersonIdentityFuture<'a, Vec<String>> {
            let fail = self.fail;
            let local = company.to_owned();
            Box::pin(async move {
                if fail || local != "acme:co" {
                    return Err(unavailable());
                }
                Ok(vec!["employee".into()])
            })
        }
    }

    fn role(id: String) -> RoleRevision {
        RoleRevision {
            id,
            name: "Employee".into(),
            company_id: "acme:co".into(),
            revision: 2,
            built_in: true,
            permissions: vec!["iam.read".into()],
            grantable_permissions: vec![],
            scopes: vec!["holding".into()],
            status: "active".into(),
        }
    }

    #[tokio::test]
    async fn load_pins_identity_ids_projection() {
        let stub = Stub {
            fail: false,
            aliases: vec!["alias".into()],
            successors: vec![],
        };
        let result = load_identity_ids(&stub, "acme:co", &["user".into()])
            .await
            .expect("identity ids");
        assert_eq!(result.identity_ids, vec!["user".to_owned(), "alias".to_owned()]);
    }

    #[tokio::test]
    async fn load_pins_successor_projection_and_snapshot_flag() {
        let stub = Stub {
            fail: false,
            aliases: vec![],
            successors: vec![SuccessorRow {
                original_id: "old".into(),
                current_id: "new".into(),
            }],
        };
        let ids = vec!["old".to_owned()];
        let result = load_principals(&stub, "acme:co", &ids, false)
            .await
            .expect("principals");
        assert_eq!(result.principals[0].current, "new");
        assert!(load_principals(&stub, "acme:co", &ids, true).await.is_ok());
    }

    #[tokio::test]
    async fn load_pins_directory_configuration_rows() {
        let stub = Stub {
            fail: false,
            aliases: vec![],
            successors: vec![],
        };
        let result = load_directory_config(&stub, "acme:co", "connections", true)
            .await
            .expect("directory");
        assert_eq!(result.rows, vec![json!({"id": "conn-1", "name": "Directory"})]);
        assert!(load_directory_config(&stub, "acme:co", "not-a-kind", false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn load_pins_published_role_projection() {
        let stub = Stub {
            fail: false,
            aliases: vec![],
            successors: vec![],
        };
        let one = load_role(&stub, "acme:co", "employee").await.expect("role");
        assert_eq!(one.role, Some(role("employee".into())));
        assert_eq!(load_role(&stub, "acme:co", "missing").await.expect("role").role, None);
        let many = load_roles(&stub, "acme:co", &["employee".into()]).await.expect("roles");
        assert_eq!(many.roles.len(), 1);
        let ids = load_role_ids(&stub, "acme:co").await.expect("ids");
        assert_eq!(ids.ids, vec!["employee".to_owned()]);
        assert!(load_role(&stub, "bad/id", "employee").await.is_err());
    }

    #[tokio::test]
    async fn load_fails_closed_on_bad_scope_and_reader_fault() {
        let stub = Stub {
            fail: false,
            aliases: vec![],
            successors: vec![],
        };
        assert_eq!(
            load_identity_ids(&stub, "bad/id", &["user".into()]).await.unwrap_err(),
            invalid()
        );
        let broken = Stub {
            fail: true,
            aliases: vec![],
            successors: vec![],
        };
        assert_eq!(
            load_identity_ids(&broken, "acme:co", &["user".into()])
                .await
                .unwrap_err(),
            unavailable()
        );
    }
}
