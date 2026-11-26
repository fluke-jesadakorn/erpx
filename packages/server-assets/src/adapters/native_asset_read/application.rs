//! Commands/queries and owner ports for the native Asset register read.
//!
//! The router depends only on these traits. A query is read-only: it opens one
//! Holding unit, runs the existing owner read handlers against the physical
//! Asset repository and returns the complete application DTO. There is no
//! command, no fallback reader and no command ledger on this path.
use super::domain;
use crate::public::asset_register::{AssetActor, AssetDetail, AssetWorkspace};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type AssetReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// A read-only owner port. `actor` is supplied by composition's Account proof,
/// never by the inbound request body or by an untrusted header.
pub trait AssetRead: Send + Sync {
    fn workspace<'a>(&'a self, actor: AssetActor) -> AssetReadFuture<'a, AssetWorkspace>;
    fn detail<'a>(&'a self, actor: AssetActor, id: &'a str) -> AssetReadFuture<'a, AssetDetail>;
}

/// The current Account/IAM authority for one exact Holding target. Composition
/// resolves session, scope and permissions; the owner router only consumes the
/// resulting actor. Missing proof fails closed before any SQL is executed.
pub struct AssetReadActorRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a erpx_server_runtime::public::http_support::HeaderMap,
}

pub type AssetReadActorFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AssetActor, BusinessError>> + Send + 'a>>;

pub trait AssetReadActorSource: Send + Sync {
    fn actor<'a>(&'a self, request: AssetReadActorRequest<'a>) -> AssetReadActorFuture<'a>;
}

/// Workspace query: the exact payload the retained `/assets` handler returned
/// without an `id`, minus the optional presentation-only purchase controls.
pub fn load_workspace<R: AssetRead + ?Sized>(
    reader: &R,
    actor: AssetActor,
) -> AssetReadFuture<'_, AssetWorkspace> {
    Box::pin(async move {
        actor.require("records.read")?;
        actor.require_module()?;
        reader.workspace(actor).await
    })
}

/// Detail query: the exact payload the retained `/assets?id=` handler returned,
/// including history, maintenance references and the scoped Finance snapshot.
pub fn load_detail<'a, R: AssetRead + ?Sized>(
    reader: &'a R,
    actor: AssetActor,
    id: &'a str,
) -> AssetReadFuture<'a, AssetDetail> {
    Box::pin(async move {
        domain::validate_id(id)?;
        actor.require("records.read")?;
        actor.require_module()?;
        reader.detail(actor, id).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public::asset_register::{AssetDetail, AssetWorkspace};
    use erpx_server_shared::public::{CompanyId, RecordId, UserId};

    fn actor() -> AssetActor {
        AssetActor {
            user_id: UserId::new("user-1").unwrap(),
            company_id: CompanyId::new("acme:co").unwrap(),
            branch_id: Some(RecordId::new("branch-1").unwrap()),
            department_id: None,
            permissions: vec!["records.read".into()],
            modules: vec!["assets".into()],
        }
    }

    struct Stub;
    impl AssetRead for Stub {
        fn workspace<'a>(&'a self, actor: AssetActor) -> AssetReadFuture<'a, AssetWorkspace> {
            Box::pin(async move {
                Ok(AssetWorkspace {
                    items: Vec::new(),
                    people: Vec::new(),
                    nodes: Vec::new(),
                    finance_assets: Vec::new(),
                    can_link_finance: false,
                    can_create: actor.has("records.write"),
                    actor_id: actor.user_id.as_str().to_owned(),
                })
            })
        }
        fn detail<'a>(&'a self, actor: AssetActor, _: &'a str) -> AssetReadFuture<'a, AssetDetail> {
            Box::pin(async move {
                Err(erpx_server_shared::public::BusinessError::invalid(
                    "STUB", "stub",
                ))
            })
        }
    }

    #[tokio::test]
    async fn workspace_query_returns_the_complete_owner_dto() {
        let value = load_workspace(&Stub, actor()).await.unwrap();
        assert_eq!(value.actor_id, "user-1");
        assert_eq!(
            serde_json::to_value(&value).unwrap(),
            serde_json::json!({
                "items": [], "people": [], "nodes": [], "financeAssets": [],
                "canLinkFinance": false, "canCreate": false, "actorId": "user-1",
            })
        );
    }

    #[tokio::test]
    async fn detail_query_fails_closed_for_an_unbounded_id_before_the_reader() {
        let error = load_detail(&Stub, actor(), &"a".repeat(201))
            .await
            .unwrap_err();
        assert_eq!(error.code, "ASSETS_NATIVE_READ_INVALID");
    }

    #[tokio::test]
    async fn absent_permission_or_module_fails_closed() {
        let mut limited = actor();
        limited.permissions.clear();
        assert_eq!(
            load_workspace(&Stub, limited).await.unwrap_err().code,
            "ASSETS_ACCESS_CHANGED"
        );
        let mut limited = actor();
        limited.modules.clear();
        assert_eq!(
            load_workspace(&Stub, limited).await.unwrap_err().code,
            "MODULE_FORBIDDEN"
        );
    }
}
