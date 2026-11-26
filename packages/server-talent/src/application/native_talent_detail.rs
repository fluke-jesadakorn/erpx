//! Application query and owner port for the native Talent detail read.
//!
//! SQL, connections and row mapping stay in the Postgres adapter; this layer
//! owns validation and the read-only query boundary. There is no command key
//! and no write ledger: the read is one snapshot over the owner tables.

use crate::domain::native_talent_detail::{
    TalentDetail, invalid, validate_id, validate_optional_scope,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type TalentDetailReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port injected by composition. The reader executes the owner-authored
/// SELECTs and returns the complete `TalentDetail` projection; there is no
/// TypeScript preparation or execution behind this port.
pub trait TalentDetailRead: Send + Sync {
    fn detail<'a>(
        &'a self,
        query: &'a TalentDetailQuery,
    ) -> TalentDetailReadFuture<'a, Option<TalentDetail>>;
}

/// Read-only query carrying only validated wire facts. `can_manage_documents`
/// is the caller's already-resolved IAM permission; the owner does not mint it,
/// it only snapshots it into the returned payload for parity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TalentDetailQuery {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub actor_id: String,
    pub can_manage_documents: bool,
}

impl TalentDetailQuery {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
        actor_id: &str,
        can_manage_documents: bool,
    ) -> Result<Self, BusinessError> {
        validate_id(id)?;
        validate_optional_scope(Some(company_id))?;
        validate_optional_scope(branch_id)?;
        validate_optional_scope(department_id)?;
        validate_id(actor_id)?;
        if company_id.is_empty() || actor_id.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            id: id.to_owned(),
            company_id: company_id.to_owned(),
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            actor_id: actor_id.to_owned(),
            can_manage_documents,
        })
    }
}

#[derive(Clone)]
pub struct TalentDetailHandler {
    reader: Arc<dyn TalentDetailRead>,
}

impl TalentDetailHandler {
    pub fn new(reader: Arc<dyn TalentDetailRead>) -> Self {
        Self { reader }
    }

    pub fn handle<'a>(
        &'a self,
        query: TalentDetailQuery,
    ) -> TalentDetailReadFuture<'a, Option<TalentDetail>> {
        let reader = self.reader.clone();
        Box::pin(async move { reader.detail(&query).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Probe;

    impl TalentDetailRead for Probe {
        fn detail<'a>(
            &'a self,
            query: &'a TalentDetailQuery,
        ) -> TalentDetailReadFuture<'a, Option<TalentDetail>> {
            let id = query.id.clone();
            Box::pin(async move {
                Ok(Some(TalentDetail {
                    application: serde_json::json!({"id": id, "status": "applied", "version": 1}),
                    history: vec![serde_json::json!({"version": 1, "action": "create"})],
                    proposal: serde_json::Value::Null,
                    offer: serde_json::Value::Null,
                    handoff: serde_json::Value::Null,
                    actor_id: "user-1".into(),
                    can_review: true,
                    can_manage_documents: false,
                }))
            })
        }
    }

    #[test]
    fn query_validates_every_wire_fact_before_reaching_the_owner() {
        assert!(TalentDetailQuery::new("app-1", "company-1", None, None, "user-1", false).is_ok());
        assert!(TalentDetailQuery::new("bad id!", "company-1", None, None, "user-1", false).is_err());
        assert!(TalentDetailQuery::new("app-1", "", None, None, "user-1", false).is_err());
        assert!(TalentDetailQuery::new("app-1", "company-1", Some("bad branch!"), None, "user-1", false).is_err());
    }

    #[tokio::test]
    async fn handler_returns_the_complete_owner_projection() {
        let handler = TalentDetailHandler::new(Arc::new(Probe));
        let query = TalentDetailQuery::new("app-1", "company-1", None, None, "user-1", true).unwrap();
        let detail = handler.handle(query).await.unwrap().expect("detail");
        assert_eq!(detail.application["id"], "app-1");
        assert_eq!(detail.history.len(), 1);
        assert!(detail.proposal.is_null());
        assert!(detail.offer.is_null());
        assert!(detail.handoff.is_null());
        assert_eq!(detail.actor_id, "user-1");
        assert!(detail.can_review);
    }
}
