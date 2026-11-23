//! Unit proof for the Files-side native Asset descriptor binding seam. The
//! connection and owner are test doubles; the policy being bound is the real
//! `asset_descriptor_read_scope`, already proved against the TypeScript oracle
//! in `rust-attachment-read-policy.test.rs`. These cases cover the binding
//! itself: source observation, IAM/scope gating, freshness and exact version.

use erpx_server_files::public::asset_descriptor_binding::{
    AssetDescriptorEvidence, AssetDescriptorFacts, AssetDescriptorPort, AssetReadAccess,
    bind_asset_attachment_target_scope,
};
use erpx_server_files::public::attachment_read::{asset_descriptor_read_scope, AssetReadDescriptor};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use std::sync::{Arc, Mutex};

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn row(branch: &str, department: Option<&str>, version: i64) -> AssetDescriptorFacts {
    AssetDescriptorFacts {
        company_id: "company".into(),
        branch_id: Some(branch.into()),
        department_id: department.map(str::to_owned),
        version,
    }
}

fn access() -> AssetReadAccess<'static> {
    AssetReadAccess {
        has_current_records_read: true,
        has_current_assets_module: true,
        actor_branch: None,
        actor_department: None,
    }
}

struct FakeEvidence {
    data: Option<AssetDescriptorFacts>,
    stale: bool,
}

impl AssetDescriptorEvidence<()> for FakeEvidence {
    fn data(&self) -> Option<&AssetDescriptorFacts> {
        self.data.as_ref()
    }
    fn require_current<'a>(&'a self, _: &'a mut ()) -> PgWorkFuture<'a, (), BusinessError> {
        let stale = self.stale;
        Box::pin(async move {
            if stale {
                Err(PgTransactionError::Business(BusinessError::new(
                    FailureKind::Conflict,
                    "ASSET_ATTACHMENT_DESCRIPTOR_CHANGED",
                    "The Asset attachment source changed.",
                )))
            } else {
                Ok(())
            }
        })
    }
}

struct FakePort {
    data: Option<AssetDescriptorFacts>,
    stale: bool,
    observed: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakePort {
    fn new(data: Option<AssetDescriptorFacts>) -> Self {
        Self {
            data,
            stale: false,
            observed: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AssetDescriptorPort<()> for FakePort {
    fn observe<'a>(
        &'a self,
        _: &'a mut (),
        company: &'a CompanyId,
        target_id: &'a str,
    ) -> PgWorkFuture<'a, Box<dyn AssetDescriptorEvidence<()>>, BusinessError> {
        self.observed
            .lock()
            .unwrap()
            .push((company.as_str().into(), target_id.into()));
        let data = self.data.clone();
        let stale = self.stale;
        Box::pin(async move {
            Ok(Box::new(FakeEvidence { data, stale }) as Box<dyn AssetDescriptorEvidence<()>>)
        })
    }
}

#[tokio::test]
async fn binds_owner_descriptor_and_agrees_with_the_verified_policy() {
    let port = FakePort::new(Some(row("branch", Some("department"), 7)));
    let scope = bind_asset_attachment_target_scope(&mut (), &port, access(), &company(), "asset-id")
        .await
        .unwrap();
    assert_eq!(scope.company_id.as_str(), "company");
    assert_eq!(scope.branch_id.as_deref(), Some("branch"));
    assert_eq!(scope.department_id.as_deref(), Some("department"));
    assert_eq!(scope.version, 7);
    assert_eq!(
        port.observed.lock().unwrap().as_slice(),
        &[("company".into(), "asset-id".into())]
    );
    // The binding must not reinterpret the verified policy decision.
    let d = port.data.clone().unwrap();
    let expected = asset_descriptor_read_scope(
        true,
        true,
        None,
        None,
        Some(AssetReadDescriptor {
            company_id: &d.company_id,
            branch_id: d.branch_id.as_deref(),
            department_id: d.department_id.as_deref(),
            version: d.version,
        }),
    )
    .unwrap();
    assert_eq!(scope.branch_id.as_deref(), expected.branch_id);
    assert_eq!(scope.department_id.as_deref(), expected.department_id);
}

#[tokio::test]
async fn keeps_the_exact_bigint_version() {
    let version = 9_007_199_254_740_993_i64;
    let port = FakePort::new(Some(row("branch", None, version)));
    let scope = bind_asset_attachment_target_scope(&mut (), &port, access(), &company(), "asset-id")
        .await
        .unwrap();
    assert_eq!(scope.version, version);
}

#[tokio::test]
async fn denies_without_current_records_read_before_freshness() {
    let mut port = FakePort::new(Some(row("branch", None, 1)));
    port.stale = true;
    let a = AssetReadAccess {
        has_current_records_read: false,
        ..access()
    };
    let error = bind_asset_attachment_target_scope(&mut (), &port, a, &company(), "asset-id")
        .await
        .err()
        .unwrap();
    match error {
        PgTransactionError::Business(e) => assert_eq!(e.code, "ATTACHMENT_FORBIDDEN"),
        other => panic!("expected business denial, got {other:?}"),
    }
}

#[tokio::test]
async fn denies_without_the_assets_module() {
    let port = FakePort::new(Some(row("branch", None, 1)));
    let a = AssetReadAccess {
        has_current_assets_module: false,
        ..access()
    };
    let error = bind_asset_attachment_target_scope(&mut (), &port, a, &company(), "asset-id")
        .await
        .err()
        .unwrap();
    match error {
        PgTransactionError::Business(e) => assert_eq!(e.code, "ATTACHMENT_FORBIDDEN"),
        other => panic!("expected business denial, got {other:?}"),
    }
}

#[tokio::test]
async fn missing_descriptor_is_target_not_found() {
    let port = FakePort::new(None);
    let error = bind_asset_attachment_target_scope(&mut (), &port, access(), &company(), "asset-id")
        .await
        .err()
        .unwrap();
    match error {
        PgTransactionError::Business(e) => assert_eq!(e.code, "ATTACHMENT_TARGET_NOT_FOUND"),
        other => panic!("expected not found, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_branch_and_department_scope_mismatch() {
    for (actor_branch, actor_department) in [
        (Some("other"), None),
        (None, Some("other")),
    ] {
        let port = FakePort::new(Some(row("branch", Some("department"), 1)));
        let a = AssetReadAccess {
            actor_branch,
            actor_department,
            ..access()
        };
        let error = bind_asset_attachment_target_scope(&mut (), &port, a, &company(), "asset-id")
            .await
            .err()
            .unwrap();
        match error {
            PgTransactionError::Business(e) => assert_eq!(e.code, "ATTACHMENT_SCOPE_FORBIDDEN"),
            other => panic!("expected scope denial, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn empty_actor_selectors_are_inactive() {
    let port = FakePort::new(Some(row("branch", None, 1)));
    let a = AssetReadAccess {
        actor_branch: Some(""),
        actor_department: Some(""),
        ..access()
    };
    let scope = bind_asset_attachment_target_scope(&mut (), &port, a, &company(), "asset-id")
        .await
        .unwrap();
    assert_eq!(scope.branch_id.as_deref(), Some("branch"));
}

#[tokio::test]
async fn stale_owner_descriptor_aborts_the_bind() {
    let mut port = FakePort::new(Some(row("branch", None, 1)));
    port.stale = true;
    let error = bind_asset_attachment_target_scope(&mut (), &port, access(), &company(), "asset-id")
        .await
        .err()
        .unwrap();
    match error {
        PgTransactionError::Business(e) => {
            assert_eq!(e.code, "ASSET_ATTACHMENT_DESCRIPTOR_CHANGED")
        }
        other => panic!("expected freshness conflict, got {other:?}"),
    }
}
