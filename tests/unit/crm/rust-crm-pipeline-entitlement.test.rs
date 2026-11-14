//! Native CRM pipeline entitlement contract tests.
//! The permissive composition stub is not a silent bypass: an entitlement
//! without the current Commercial revision fails closed before any owner row
//! is read.
use erpx_server_crm::public::pipeline::{
    CRM_PIPELINE_ENTITLEMENT_UNPROVEN, CrmForecastSourceRow, CrmPipelineActor,
    CrmPipelineCustomerDirectory, CrmPipelineEntitlement, CrmPipelineEntitlementPort,
    CrmPipelineFuture, CrmPipelinePeopleDirectory, CrmPipelineQuery, CrmPipelineRepository,
    CrmPipelineRow, crm_pipeline,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use std::collections::BTreeMap;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> CrmPipelineActor {
    CrmPipelineActor {
        user_id: "owner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        permissions: vec!["commerce.read".into(), "commerce.sales".into()],
        modules: vec!["crm".into()],
        expires_at: NOW.get() + 60_000,
    }
}

struct Repo {
    page_calls: usize,
    forecast_calls: usize,
}

impl CrmPipelineRepository for Repo {
    type Error = BusinessError;

    fn page<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: &'a str,
        _: Option<&'a str>,
        _: Option<&'a erpx_server_crm::public::pipeline::CrmPipelineCursor>,
        _: usize,
    ) -> CrmPipelineFuture<'a, Vec<CrmPipelineRow>, Self::Error> {
        self.page_calls += 1;
        Box::pin(async { Ok(Vec::new()) })
    }

    fn forecast<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> CrmPipelineFuture<'a, Vec<CrmForecastSourceRow>, Self::Error> {
        self.forecast_calls += 1;
        Box::pin(async { Ok(Vec::new()) })
    }
}

struct EmptyDirectory;

impl CrmPipelinePeopleDirectory for EmptyDirectory {
    type Error = BusinessError;

    fn names<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }
}

impl CrmPipelineCustomerDirectory for EmptyDirectory {
    type Error = BusinessError;

    fn names<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }
}

struct Packages(CrmPipelineEntitlement);

impl CrmPipelineEntitlementPort for Packages {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _: &'a str,
    ) -> CrmPipelineFuture<'a, CrmPipelineEntitlement, Self::Error> {
        Box::pin(async { Ok(self.0.clone()) })
    }
}

fn stub() -> CrmPipelineEntitlement {
    CrmPipelineEntitlement {
        enforced: false,
        history_allowed: false,
        new_work: false,
        settle_existing: false,
        revision: String::new(),
    }
}

#[tokio::test]
async fn unproven_entitlement_fails_closed_before_the_owner_read() {
    let mut repo = Repo {
        page_calls: 0,
        forecast_calls: 0,
    };
    let mut packages = Packages(stub());
    let mut people = EmptyDirectory;
    let mut customers = EmptyDirectory;
    let error = crm_pipeline(
        &actor(),
        &CrmPipelineQuery::default(),
        &mut repo,
        &mut people,
        &mut customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Unavailable);
    assert_eq!(error.code, CRM_PIPELINE_ENTITLEMENT_UNPROVEN);
    assert_eq!(repo.page_calls, 0);
    assert_eq!(repo.forecast_calls, 0);
}

#[tokio::test]
async fn explicit_disabled_policy_requires_a_current_revision() {
    let mut repo = Repo {
        page_calls: 0,
        forecast_calls: 0,
    };
    let mut packages = Packages(CrmPipelineEntitlement {
        enforced: false,
        history_allowed: true,
        new_work: true,
        settle_existing: true,
        revision: "7".into(),
    });
    let mut people = EmptyDirectory;
    let mut customers = EmptyDirectory;
    let page = crm_pipeline(
        &actor(),
        &CrmPipelineQuery::default(),
        &mut repo,
        &mut people,
        &mut customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert!(page.purchase.is_none());
    assert_eq!(repo.page_calls, 1);
    assert_eq!(repo.forecast_calls, 1);
}

#[test]
fn entitlement_requires_a_revision_even_when_enforcement_is_off() {
    let error = stub().require_history().unwrap_err();
    assert_eq!(error.kind, FailureKind::Unavailable);
    assert_eq!(error.code, CRM_PIPELINE_ENTITLEMENT_UNPROVEN);

    let whitespace = CrmPipelineEntitlement {
        revision: "   ".into(),
        ..stub()
    };
    assert_eq!(
        whitespace.require_history().unwrap_err().code,
        CRM_PIPELINE_ENTITLEMENT_UNPROVEN
    );

    let denied = CrmPipelineEntitlement {
        enforced: true,
        history_allowed: false,
        revision: "8".into(),
        ..stub()
    };
    assert_eq!(
        denied.require_history().unwrap_err().code,
        "PACKAGE_REQUIRED"
    );
}
