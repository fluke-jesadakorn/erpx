//! Application port and handler for the native lane 11 person-identity reads.
//!
//! No SQL lives here. Composition injects the owner reader; the handlers only
//! validate the Company boundary, pin the projection and enforce fail-closed
//! invariants before the transport serialises the response.

use crate::adapters::native_lane_11::domain::{
    IdentityIdsResponse, PrincipalsResponse, SuccessorRow, invalid, identity_ids, principals,
    valid_company, validate_ids,
};
use erpx_server_shared::public::BusinessError;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_11::domain::unavailable;

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
