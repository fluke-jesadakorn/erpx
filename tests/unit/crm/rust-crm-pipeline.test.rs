//! Native CRM pipeline read policy, page, forecast and directory tests.
//! These are owner unit facts, not HTTP, PostgreSQL, command or browser proof.
use erpx_server_crm::public::pipeline::{
    CrmForecastSourceRow, CrmPipelineActor, CrmPipelineCustomerDirectory, CrmPipelineEntitlement,
    CrmPipelineEntitlementPort, CrmPipelineFuture, CrmPipelinePeopleDirectory, CrmPipelineQuery,
    CrmPipelineRepository, CrmPipelineRow, crm_pipeline, validate_forecast,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
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

fn row(id: &str, updated_at: i64) -> CrmPipelineRow {
    CrmPipelineRow {
        id: id.into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        title: "Office installation inquiry".into(),
        contact_name: "Synthetic contact".into(),
        status: "open".into(),
        stage: None,
        opportunity_id: None,
        expected_minor: None,
        currency: None,
        close_date: None,
        due_date: "2026-11-20".into(),
        owner_id: "owner".into(),
        owner_name: None,
        customer_id: None,
        customer_name: None,
        version: 1,
        opportunity_version: None,
        updated_at,
    }
}

fn opportunity(id: &str, updated_at: i64, stage: &str) -> CrmPipelineRow {
    CrmPipelineRow {
        id: id.into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        title: "Reviewed customer opportunity".into(),
        contact_name: "Synthetic contact".into(),
        status: "qualified".into(),
        stage: Some(stage.into()),
        opportunity_id: Some(format!("opportunity-{id}")),
        expected_minor: Some(10_000),
        currency: Some("THB".into()),
        close_date: Some("2026-11-30".into()),
        due_date: "2026-11-20".into(),
        owner_id: "owner".into(),
        owner_name: None,
        customer_id: Some("customer".into()),
        customer_name: None,
        version: 2,
        opportunity_version: Some(1),
        updated_at,
    }
}

struct Repo {
    page: Vec<CrmPipelineRow>,
    forecast: Vec<CrmForecastSourceRow>,
    page_calls: usize,
    forecast_calls: usize,
    last_search: String,
    last_stage: Option<String>,
}

impl Repo {
    fn new(page: Vec<CrmPipelineRow>, forecast: Vec<CrmForecastSourceRow>) -> Self {
        Self {
            page,
            forecast,
            page_calls: 0,
            forecast_calls: 0,
            last_search: String::new(),
            last_stage: None,
        }
    }
}

impl CrmPipelineRepository for Repo {
    type Error = BusinessError;
    fn page<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch_id: Option<&'a str>,
        _department_id: Option<&'a str>,
        search: &'a str,
        stage: Option<&'a str>,
        _cursor: Option<&'a erpx_server_crm::public::pipeline::CrmPipelineCursor>,
        _limit: usize,
    ) -> CrmPipelineFuture<'a, Vec<CrmPipelineRow>, Self::Error> {
        self.page_calls += 1;
        self.last_search = search.to_owned();
        self.last_stage = stage.map(str::to_owned);
        Box::pin(async move { Ok(self.page.clone()) })
    }
    fn forecast<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch_id: Option<&'a str>,
        _department_id: Option<&'a str>,
    ) -> CrmPipelineFuture<'a, Vec<CrmForecastSourceRow>, Self::Error> {
        self.forecast_calls += 1;
        Box::pin(async move { Ok(self.forecast.clone()) })
    }
}

struct People;
impl CrmPipelinePeopleDirectory for People {
    type Error = BusinessError;
    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        user_ids: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error> {
        Box::pin(async move {
            Ok(user_ids
                .iter()
                .map(|id| (id.clone(), format!("Staff {id}")))
                .collect())
        })
    }
}

struct Customers;
impl CrmPipelineCustomerDirectory for Customers {
    type Error = BusinessError;
    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        customer_ids: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error> {
        Box::pin(async move {
            Ok(customer_ids
                .iter()
                .map(|id| (id.clone(), format!("Customer {id}")))
                .collect())
        })
    }
}

struct Packages {
    entitlement: CrmPipelineEntitlement,
    calls: usize,
}
impl CrmPipelineEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> CrmPipelineFuture<'a, CrmPipelineEntitlement, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.entitlement.clone()) })
    }
}

fn allowed() -> Packages {
    Packages {
        entitlement: CrmPipelineEntitlement {
            enforced: true,
            history_allowed: true,
            new_work: true,
            settle_existing: true,
            revision: "7".into(),
        },
        calls: 0,
    }
}

fn query() -> CrmPipelineQuery {
    CrmPipelineQuery::default()
}

#[tokio::test]
async fn actor_policy_fails_before_any_source_or_directory_read() {
    let mut denied = actor();
    denied.permissions.clear();
    let mut repo = Repo::new(vec![row("lead-1", 10)], Vec::new());
    let mut packages = allowed();
    let error = crm_pipeline(
        &denied,
        &query(),
        &mut repo,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "CRM_PERMISSION_REQUIRED");
    assert_eq!(repo.page_calls, 0);
    assert_eq!(repo.forecast_calls, 0);
    assert_eq!(packages.calls, 0);

    let mut module = actor();
    module.modules = vec!["sales".into()];
    let error = crm_pipeline(
        &module,
        &query(),
        &mut repo,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CRM_MODULE_FORBIDDEN");
    assert_eq!(repo.page_calls, 0);

    let mut expired = actor();
    expired.expires_at = NOW.get();
    let error = crm_pipeline(
        &expired,
        &query(),
        &mut repo,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(repo.page_calls, 0);
}

#[tokio::test]
async fn current_package_history_is_required_before_the_owner_read() {
    let mut repo = Repo::new(vec![row("lead-1", 10)], Vec::new());
    let mut packages = Packages {
        entitlement: CrmPipelineEntitlement {
            enforced: true,
            history_allowed: false,
            new_work: false,
            settle_existing: false,
            revision: "8".into(),
        },
        calls: 0,
    };
    let error = crm_pipeline(
        &actor(),
        &query(),
        &mut repo,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PACKAGE_REQUIRED");
    assert_eq!(repo.page_calls, 0);
    assert_eq!(repo.forecast_calls, 0);
    assert_eq!(packages.calls, 1);
}

#[test]
fn query_rejects_control_search_unknown_stage_and_unbounded_cursor() {
    for search in ["x".repeat(201), "control\u{0007}".into()] {
        let error = CrmPipelineQuery {
            search,
            ..CrmPipelineQuery::default()
        }
        .validate()
        .unwrap_err();
        assert_eq!(error.code, "CRM_PIPELINE_REQUEST_INVALID");
    }
    let error = CrmPipelineQuery {
        stage: Some("paid".into()),
        ..CrmPipelineQuery::default()
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code, "CRM_PIPELINE_STAGE_INVALID");
    for cursor in ["", "x".repeat(4_097).as_str()] {
        let error = CrmPipelineQuery {
            cursor: Some(cursor.into()),
            ..CrmPipelineQuery::default()
        }
        .validate()
        .unwrap_err();
        assert_eq!(error.code, "CRM_PIPELINE_CURSOR_INVALID");
    }
    assert!(
        CrmPipelineQuery {
            stage: Some(String::new()),
            ..CrmPipelineQuery::default()
        }
        .validate()
        .is_ok()
    );
}

#[tokio::test]
async fn page_rows_are_scoped_ordered_unique_and_cursor_bound() {
    let mut first = Repo::new(
        vec![row("lead-3", 30), row("lead-2", 20), row("lead-1", 10)],
        Vec::new(),
    );
    let mut packages = allowed();
    let page = crm_pipeline(
        &actor(),
        &query(),
        &mut first,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 3);
    assert!(!page.has_more);
    assert!(page.next_cursor.is_none());
    assert_eq!(page.items[0].owner_name.as_deref(), Some("Staff owner"));

    let cursor = "cursor-from-search";
    let cursor_query = CrmPipelineQuery {
        search: String::new(),
        stage: None,
        cursor: Some(cursor.into()),
    };
    let mut repo = Repo::new(vec![row("lead-4", 40)], Vec::new());
    let error = crm_pipeline(
        &actor(),
        &cursor_query,
        &mut repo,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CRM_PIPELINE_CURSOR_INVALID");
    assert_eq!(repo.page_calls, 0);

    let mut scoped = Repo::new(
        vec![
            opportunity("lead-4", 40, "won"),
            opportunity("lead-3", 30, "lost"),
            opportunity("lead-2", 20, "discovery"),
            opportunity("lead-1", 10, "proposal"),
        ],
        Vec::new(),
    );
    let page = crm_pipeline(
        &actor(),
        &CrmPipelineQuery {
            search: "  customer  ".into(),
            stage: Some("discovery".into()),
            ..CrmPipelineQuery::default()
        },
        &mut scoped,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(page.code, "CRM_PIPELINE_SOURCE_INVALID");
    assert_eq!(scoped.last_search, "customer");
    assert_eq!(scoped.last_stage.as_deref(), Some("discovery"));
}

#[tokio::test]
async fn page_accepts_iso_year_zero_and_upper_date_boundary() {
    for date in ["0000-01-01", "0000-02-29", "2000-02-29", "9999-12-31"] {
        let mut item = opportunity("lead-1", 10, "discovery");
        item.due_date = date.into();
        item.close_date = Some(date.into());
        let mut repo = Repo::new(vec![item], Vec::new());
        assert!(crm_pipeline(&actor(), &query(), &mut repo, &mut People, &mut Customers, &mut allowed(), NOW).await.is_ok(), "{date}");
    }
}

#[tokio::test]
async fn page_source_negatives_fail_closed() {
    let mut cases: Vec<(CrmPipelineRow, &'static str)> = Vec::new();

    let mut foreign = row("lead-1", 10);
    foreign.company_id = "foreign".into();
    cases.push((foreign, "foreign company"));

    let mut foreign_branch = row("lead-1", 10);
    foreign_branch.branch_id = None;
    cases.push((foreign_branch, "missing branch"));

    let mut duplicate = opportunity("lead-1", 10, "discovery");
    duplicate.status = "open".into();
    cases.push((duplicate, "qualified status mismatch"));

    let mut no_stage = opportunity("lead-1", 10, "discovery");
    no_stage.stage = None;
    cases.push((no_stage, "missing stage"));

    let mut empty_opportunity = opportunity("lead-1", 10, "discovery");
    empty_opportunity.opportunity_id = Some(String::new());
    cases.push((empty_opportunity, "empty opportunity id"));

    let mut stray_customer_name = row("lead-1", 10);
    stray_customer_name.customer_name = Some("Unlinked customer".into());
    cases.push((stray_customer_name, "customer name without opportunity"));

    let mut negative = opportunity("lead-1", 10, "discovery");
    negative.expected_minor = Some(-1);
    cases.push((negative, "negative amount"));

    let mut currency = opportunity("lead-1", 10, "discovery");
    currency.currency = Some("thb".into());
    cases.push((currency, "invalid currency"));

    let mut date = opportunity("lead-1", 10, "discovery");
    date.close_date = Some("2026-02-30".into());
    cases.push((date, "invalid date"));

    let mut year_zero = opportunity("lead-1", 10, "discovery");
    year_zero.close_date = Some("0000-02-30".into());
    cases.push((year_zero, "invalid year zero leap day"));

    let mut version = row("lead-1", 10);
    version.version = 0;
    cases.push((version, "invalid version"));

    let mut status = row("lead-1", 10);
    status.status = "won".into();
    cases.push((status, "invalid status"));

    let mut packages = allowed();
    for (invalid, label) in cases {
        let mut repo = Repo::new(vec![invalid], Vec::new());
        let error = crm_pipeline(
            &actor(),
            &query(),
            &mut repo,
            &mut People,
            &mut Customers,
            &mut packages,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CRM_PIPELINE_SOURCE_INVALID", "case: {label}");
    }

    let mut duplicate = Repo::new(vec![row("lead-1", 10), row("lead-1", 9)], Vec::new());
    let error = crm_pipeline(
        &actor(),
        &query(),
        &mut duplicate,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CRM_PIPELINE_SOURCE_INVALID");

    let mut order = Repo::new(vec![row("lead-1", 10), row("lead-2", 20)], Vec::new());
    let error = crm_pipeline(
        &actor(),
        &query(),
        &mut order,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CRM_PIPELINE_SOURCE_INVALID");

    let mut too_many = Repo::new(
        (0..32)
            .map(|index| row(&format!("lead-{index}"), 1_000 + i64::from(index)))
            .collect(),
        Vec::new(),
    );
    let error = crm_pipeline(
        &actor(),
        &query(),
        &mut too_many,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CRM_PIPELINE_TOO_LARGE");
}

#[tokio::test]
async fn page_and_forecast_serialize_exact_owner_keys() {
    let mut repo = Repo::new(
        vec![opportunity("lead-2", 20, "won"), row("lead-1", 10)],
        vec![
            CrmForecastSourceRow {
                currency: "THB".into(),
                stage: "discovery".into(),
                unweighted_minor: 1_000,
                weighted_minor: 250,
                won_minor: 0,
            },
            CrmForecastSourceRow {
                currency: "THB".into(),
                stage: "proposal".into(),
                unweighted_minor: 2_000,
                weighted_minor: 1_000,
                won_minor: 0,
            },
            CrmForecastSourceRow {
                currency: "THB".into(),
                stage: "won".into(),
                unweighted_minor: 0,
                weighted_minor: 0,
                won_minor: 3_000,
            },
            CrmForecastSourceRow {
                currency: "USD".into(),
                stage: "lost".into(),
                unweighted_minor: 0,
                weighted_minor: 0,
                won_minor: 0,
            },
        ],
    );
    let mut packages = allowed();
    let page = crm_pipeline(
        &actor(),
        &query(),
        &mut repo,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert!(page.can_create);
    assert_eq!(
        page.purchase,
        Some(erpx_server_crm::public::pipeline::CrmPurchaseControls {
            new_work: true,
            settle_existing: true,
            history: true,
            revision: "7".into(),
        })
    );
    assert_eq!(page.forecast["THB"].unweighted_minor, "3000");
    assert_eq!(page.forecast["THB"].weighted_minor, "1250");
    assert_eq!(page.forecast["THB"].won_minor, "3000");
    assert_eq!(page.forecast["USD"].unweighted_minor, "0");

    let value = serde_json::to_value(&page).unwrap();
    assert_eq!(
        value["items"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "id",
            "company_id",
            "branch_id",
            "department_id",
            "title",
            "contact_name",
            "status",
            "stage",
            "opportunity_id",
            "expected_minor",
            "currency",
            "close_date",
            "due_date",
            "owner_id",
            "owner_name",
            "customer_id",
            "customer_name",
            "version",
            "opportunity_version",
            "updated_at",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    assert_eq!(value["canCreate"], true);
    assert_eq!(value["hasMore"], false);
    assert!(value["nextCursor"].is_null());
    assert_eq!(value["forecast"]["THB"]["unweightedMinor"], "3000");
    assert_eq!(value["purchase"]["revision"], "7");
}

#[test]
fn forecast_validation_preserves_per_row_half_up_totals_and_coherence() {
    let source = vec![
        CrmForecastSourceRow {
            currency: "THB".into(),
            stage: "discovery".into(),
            unweighted_minor: 999,
            weighted_minor: 250,
            won_minor: 0,
        },
        CrmForecastSourceRow {
            currency: "THB".into(),
            stage: "negotiation".into(),
            unweighted_minor: 1,
            weighted_minor: 1,
            won_minor: 0,
        },
    ];
    let totals = validate_forecast(source).unwrap();
    assert_eq!(totals["THB"].unweighted_minor, "1000");
    assert_eq!(totals["THB"].weighted_minor, "251");

    for invalid in [
        CrmForecastSourceRow {
            currency: "thb".into(),
            stage: "discovery".into(),
            unweighted_minor: 1,
            weighted_minor: 1,
            won_minor: 0,
        },
        CrmForecastSourceRow {
            currency: "THB".into(),
            stage: "won".into(),
            unweighted_minor: 1,
            weighted_minor: 0,
            won_minor: 1,
        },
        CrmForecastSourceRow {
            currency: "THB".into(),
            stage: "lost".into(),
            unweighted_minor: 0,
            weighted_minor: 0,
            won_minor: 1,
        },
        CrmForecastSourceRow {
            currency: "THB".into(),
            stage: "paid".into(),
            unweighted_minor: 0,
            weighted_minor: 0,
            won_minor: 0,
        },
    ] {
        assert_eq!(
            validate_forecast(vec![invalid]).unwrap_err().code,
            "CRM_PIPELINE_SOURCE_INVALID"
        );
    }
}

#[tokio::test]
async fn empty_pipeline_keeps_empty_forecast_and_no_cursor() {
    let mut repo = Repo::new(Vec::new(), Vec::new());
    let mut packages = allowed();
    let page = crm_pipeline(
        &actor(),
        &query(),
        &mut repo,
        &mut People,
        &mut Customers,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert!(page.items.is_empty());
    assert!(page.forecast.is_empty());
    assert!(!page.has_more);
    assert!(page.next_cursor.is_none());
    assert_eq!(repo.page_calls, 1);
    assert_eq!(repo.forecast_calls, 1);
}

#[test]
fn selected_branch_and_department_are_required_on_every_row() {
    let mut scoped = actor();
    scoped.branch_id = Some(RecordId::new("branch").unwrap());
    scoped.department_id = Some(RecordId::new("department").unwrap());
    let mut row = row("lead-1", 10);
    row.department_id = Some("department".into());
    row.validate(&scoped, "", None).unwrap();
    row.department_id = Some("other".into());
    assert_eq!(
        row.validate(&scoped, "", None).unwrap_err().code,
        "CRM_PIPELINE_SOURCE_INVALID"
    );
    row.department_id = Some("department".into());
    row.branch_id = Some("other".into());
    assert_eq!(
        row.validate(&scoped, "", None).unwrap_err().code,
        "CRM_PIPELINE_SOURCE_INVALID"
    );
}
