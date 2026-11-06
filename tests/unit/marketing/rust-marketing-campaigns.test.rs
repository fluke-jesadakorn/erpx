//! Native Marketing campaign/attribution policy, row, bound and ordering
//! tests. These are owner unit facts, not HTTP, PostgreSQL, command or browser
//! proof.
use erpx_server_marketing::public::campaign_attribution::{
    ATTRIBUTION_LIMIT, CAMPAIGN_LIMIT, CandidateProvenance, MarketingAttributionMode,
    MarketingAttributionRow, MarketingCampaignActor, MarketingCampaignEntitlement,
    MarketingCampaignEntitlementPort, MarketingCampaignFuture, MarketingCampaignRepository,
    MarketingCampaignRow, marketing_campaign_report,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use std::collections::BTreeMap;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

#[tokio::test]
async fn scoped_report_fails_closed_when_repository_cannot_filter_provenance() {
    for mode in [
        MarketingAttributionMode::Shadow,
        MarketingAttributionMode::Enforced,
    ] {
        let mut repo = DefaultScopeRepo(Repo::new(vec![], vec![attribution("AUTUMN", "web")]));
        let mut packages = allowed(mode == MarketingAttributionMode::Enforced);
        let error = marketing_campaign_report(&actor(), &mut repo, &mut packages, NOW, mode)
            .await
            .unwrap_err();
        assert_eq!(error.code, "MARKETING_PROVENANCE_UNAVAILABLE");
        assert_eq!(repo.0.attribution_calls, 0);
    }
    let mut repo = Repo::new(vec![], vec![attribution("AUTUMN", "web")]);
    let mut packages = allowed(false);
    assert_eq!(
        marketing_campaign_report(
            &actor(),
            &mut repo,
            &mut packages,
            NOW,
            MarketingAttributionMode::Disabled
        )
        .await
        .unwrap()
        .attribution
        .len(),
        1
    );
}

#[tokio::test]
async fn report_rejects_inconsistent_mode_and_preserves_shadow_controls() {
    for (mode, enforced) in [
        (MarketingAttributionMode::Disabled, true),
        (MarketingAttributionMode::Shadow, true),
        (MarketingAttributionMode::Enforced, false),
    ] {
        let mut repo = Repo::new(vec![], vec![]);
        let mut packages = allowed(enforced);
        let error = marketing_campaign_report(&actor(), &mut repo, &mut packages, NOW, mode)
            .await
            .unwrap_err();
        assert_eq!(error.code, "MARKETING_PURCHASE_MODE_MISMATCH");
        assert_eq!(repo.campaign_calls, 0);
        assert_eq!(repo.attribution_calls, 0);
    }
    let mut repo = Repo::new(vec![], vec![]);
    let mut packages = allowed(false);
    packages.entitlement.history_allowed = false;
    packages.entitlement.new_work = false;
    packages.entitlement.settle_existing = false;
    let report = marketing_campaign_report(
        &actor(),
        &mut repo,
        &mut packages,
        NOW,
        MarketingAttributionMode::Shadow,
    )
    .await
    .unwrap();
    let purchase = report.purchase.unwrap();
    assert!(purchase.history);
    assert!(purchase.new_work);
    assert!(purchase.settle_existing);
    assert_eq!(purchase.revision, "7");
}

fn actor() -> MarketingCampaignActor {
    MarketingCampaignActor {
        user_id: "owner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["marketing".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn campaign(id: &str, code: &str) -> MarketingCampaignRow {
    MarketingCampaignRow {
        id: id.into(),
        company_id: "company".into(),
        code: code.into(),
        name: "Autumn demand campaign".into(),
        version: 1,
        created_at: 100,
    }
}

fn attribution(campaign: &str, source: &str) -> MarketingAttributionRow {
    MarketingAttributionRow {
        campaign: campaign.into(),
        source: source.into(),
        captured: 3,
        pending: 1,
        handed_off: 1,
        rejected: 1,
    }
}

struct Repo {
    campaigns: Vec<MarketingCampaignRow>,
    attribution: Vec<MarketingAttributionRow>,
    error: Option<BusinessError>,
    campaign_calls: usize,
    attribution_calls: usize,
    last_campaign_limit: usize,
    last_attribution_limit: usize,
    last_company: String,
    last_branch: Option<String>,
    last_department: Option<String>,
}

impl Repo {
    fn new(
        campaigns: Vec<MarketingCampaignRow>,
        attribution: Vec<MarketingAttributionRow>,
    ) -> Self {
        Self {
            campaigns,
            attribution,
            error: None,
            campaign_calls: 0,
            attribution_calls: 0,
            last_campaign_limit: 0,
            last_attribution_limit: 0,
            last_company: String::new(),
            last_branch: None,
            last_department: None,
        }
    }
}

impl MarketingCampaignRepository for Repo {
    type Error = BusinessError;

    fn campaigns<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingCampaignRow>, Self::Error> {
        self.campaign_calls += 1;
        self.last_campaign_limit = limit;
        self.last_company = company.as_str().into();
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.campaigns.clone())
        })
    }

    fn attribution<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingAttributionRow>, Self::Error> {
        self.attribution_calls += 1;
        self.last_attribution_limit = limit;
        self.last_company = company.as_str().into();
        self.last_branch = branch_id.map(str::to_owned);
        self.last_department = department_id.map(str::to_owned);
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.attribution.clone())
        })
    }

    fn attribution_scoped<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        limit: usize,
        provenance: CandidateProvenance,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingAttributionRow>, Self::Error> {
        let _ = provenance;
        self.attribution(company, branch_id, department_id, limit)
    }
}

struct DefaultScopeRepo(Repo);
impl MarketingCampaignRepository for DefaultScopeRepo {
    type Error = BusinessError;
    fn campaigns<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingCampaignRow>, Self::Error> {
        self.0.campaigns(company, limit)
    }
    fn attribution<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingAttributionRow>, Self::Error> {
        self.0.attribution(company, branch_id, department_id, limit)
    }
}

struct Packages {
    entitlement: MarketingCampaignEntitlement,
    calls: usize,
}

impl MarketingCampaignEntitlementPort for Packages {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.entitlement.clone()) })
    }
}

fn allowed(enforced: bool) -> Packages {
    Packages {
        entitlement: MarketingCampaignEntitlement {
            enforced,
            history_allowed: true,
            new_work: true,
            settle_existing: false,
            revision: "7".into(),
        },
        calls: 0,
    }
}

async fn run(
    actor: &MarketingCampaignActor,
    repo: &mut Repo,
    packages: &mut Packages,
) -> Result<
    erpx_server_marketing::public::campaign_attribution::MarketingCampaignReport,
    BusinessError,
> {
    let mode = if packages.entitlement.enforced {
        MarketingAttributionMode::Enforced
    } else {
        MarketingAttributionMode::Disabled
    };
    marketing_campaign_report(actor, repo, packages, NOW, mode).await
}

#[tokio::test]
async fn current_permission_module_and_context_are_checked_before_source() {
    for (current, code) in [
        (
            {
                let mut value = actor();
                value.permissions.clear();
                value
            },
            "MARKETING_CAMPAIGN_PERMISSION_REQUIRED",
        ),
        (
            {
                let mut value = actor();
                value.modules.clear();
                value
            },
            "MARKETING_MODULE_FORBIDDEN",
        ),
        (
            {
                let mut value = actor();
                value.expires_at = NOW.get();
                value
            },
            "WORK_CONTEXT_CHANGED",
        ),
    ] {
        let mut repo = Repo::new(vec![], vec![]);
        let mut packages = allowed(true);
        let error = run(&current, &mut repo, &mut packages).await.unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(repo.campaign_calls, 0);
        assert_eq!(repo.attribution_calls, 0);
        assert_eq!(packages.calls, 0);
    }
}

#[tokio::test]
async fn package_history_is_required_before_any_owner_read() {
    let mut repo = Repo::new(vec![], vec![]);
    let mut packages = Packages {
        entitlement: MarketingCampaignEntitlement {
            enforced: true,
            history_allowed: false,
            new_work: true,
            settle_existing: true,
            revision: "9".into(),
        },
        calls: 0,
    };
    let error = run(&actor(), &mut repo, &mut packages).await.unwrap_err();
    assert_eq!(error.code, "PACKAGE_REQUIRED");
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(packages.calls, 1);
    assert_eq!(repo.campaign_calls, 0);
    assert_eq!(repo.attribution_calls, 0);
}

#[tokio::test]
async fn empty_scope_returns_bounded_empty_lists_not_null() {
    let mut repo = Repo::new(vec![], vec![]);
    let mut packages = allowed(false);
    let report = run(&actor(), &mut repo, &mut packages).await.unwrap();
    assert!(report.campaigns.is_empty());
    assert!(report.attribution.is_empty());
    assert!(report.purchase.is_none());
    assert_eq!(repo.last_campaign_limit, CAMPAIGN_LIMIT + 1);
    assert_eq!(repo.last_attribution_limit, ATTRIBUTION_LIMIT + 1);
}

#[tokio::test]
async fn enabled_package_projects_current_purchase_controls() {
    let mut repo = Repo::new(
        vec![campaign("campaign-1", "AUTUMN")],
        vec![attribution("AUTUMN", "web")],
    );
    let mut packages = allowed(true);
    let report = run(&actor(), &mut repo, &mut packages).await.unwrap();
    assert_eq!(
        report.purchase,
        Some(
            erpx_server_marketing::public::campaign_attribution::MarketingCampaignPurchaseControls {
                new_work: true,
                settle_existing: false,
                history: true,
                revision: "7".into(),
            }
        )
    );
    assert_eq!(report.campaigns.len(), 1);
    assert_eq!(report.attribution.len(), 1);
}

#[tokio::test]
async fn attribution_repository_receives_exact_selected_scope() {
    let mut current = actor();
    current.branch_id = Some(RecordId::new("branch").unwrap());
    current.department_id = Some(RecordId::new("department").unwrap());
    let mut repo = Repo::new(vec![], vec![]);
    let mut packages = allowed(false);
    run(&current, &mut repo, &mut packages).await.unwrap();
    assert_eq!(repo.last_company, "company");
    assert_eq!(repo.last_branch.as_deref(), Some("branch"));
    assert_eq!(repo.last_department.as_deref(), Some("department"));
}

#[tokio::test]
async fn campaign_rows_reject_foreign_company_and_incoherent_shapes() {
    let mut cases = vec![
        MarketingCampaignRow {
            company_id: "foreign".into(),
            ..campaign("campaign-1", "AUTUMN")
        },
        MarketingCampaignRow {
            id: String::new(),
            ..campaign("campaign-1", "AUTUMN")
        },
        MarketingCampaignRow {
            code: "A".into(),
            ..campaign("campaign-1", "AUTUMN")
        },
        MarketingCampaignRow {
            code: "AUT UMN".into(),
            ..campaign("campaign-1", "AUTUMN")
        },
        MarketingCampaignRow {
            name: " ".into(),
            ..campaign("campaign-1", "AUTUMN")
        },
        MarketingCampaignRow {
            version: 0,
            ..campaign("campaign-1", "AUTUMN")
        },
        MarketingCampaignRow {
            created_at: -1,
            ..campaign("campaign-1", "AUTUMN")
        },
    ];
    for row in cases.drain(..) {
        let mut repo = Repo::new(vec![row], vec![]);
        let mut packages = allowed(false);
        let error = run(&actor(), &mut repo, &mut packages).await.unwrap_err();
        assert_eq!(error.code, "MARKETING_CAMPAIGN_SOURCE_INVALID");
        assert_eq!(error.kind, FailureKind::Unavailable);
    }
}

#[tokio::test]
async fn campaign_rows_reject_duplicates_descending_order_and_excess() {
    let mut duplicate = Repo::new(
        vec![
            campaign("campaign-1", "AUTUMN"),
            campaign("campaign-1", "WINTER"),
        ],
        vec![],
    );
    let mut packages = allowed(false);
    assert_eq!(
        run(&actor(), &mut duplicate, &mut packages)
            .await
            .unwrap_err()
            .code,
        "MARKETING_CAMPAIGN_SOURCE_INVALID"
    );

    let mut descending = Repo::new(
        vec![
            campaign("campaign-1", "WINTER"),
            campaign("campaign-2", "AUTUMN"),
        ],
        vec![],
    );
    assert_eq!(
        run(&actor(), &mut descending, &mut packages)
            .await
            .unwrap_err()
            .code,
        "MARKETING_CAMPAIGN_SOURCE_INVALID"
    );

    let too_many = (0..=CAMPAIGN_LIMIT)
        .map(|index| campaign(&format!("campaign-{index:04}"), &format!("C{index:04}")))
        .collect();
    let mut too_many = Repo::new(too_many, vec![]);
    assert_eq!(
        run(&actor(), &mut too_many, &mut packages)
            .await
            .unwrap_err()
            .code,
        "MARKETING_CAMPAIGN_TOO_LARGE"
    );
}

#[tokio::test]
async fn campaign_names_match_zod_utf16_and_content_boundaries() {
    let mut packages = allowed(false);
    for name in [
        "Line\nbreak".to_owned(),
        "😀".repeat(80),
        "\u{85}A".to_owned(),
    ] {
        let mut row = campaign("campaign-1", "AUTUMN");
        row.name = name.clone();
        let mut repo = Repo::new(vec![row], vec![]);
        let report = run(&actor(), &mut repo, &mut packages).await.unwrap();
        assert_eq!(report.campaigns[0].name, name);
    }
    for name in [
        "😀".repeat(81),
        "\u{feff}A\u{feff}".to_owned(),
        "A\0B".to_owned(),
    ] {
        let mut row = campaign("campaign-1", "AUTUMN");
        row.name = name;
        let mut repo = Repo::new(vec![row], vec![]);
        assert_eq!(
            run(&actor(), &mut repo, &mut packages)
                .await
                .unwrap_err()
                .code,
            "MARKETING_CAMPAIGN_SOURCE_INVALID"
        );
    }
}

#[tokio::test]
async fn exact_campaign_limit_is_accepted() {
    let rows = (0..CAMPAIGN_LIMIT)
        .map(|index| campaign(&format!("campaign-{index:04}"), &format!("C{index:04}")))
        .collect();
    let mut repo = Repo::new(rows, vec![]);
    let mut packages = allowed(false);
    let report = run(&actor(), &mut repo, &mut packages).await.unwrap();
    assert_eq!(report.campaigns.len(), CAMPAIGN_LIMIT);
}

#[tokio::test]
async fn attribution_keys_match_transport_utf16_and_edge_boundaries() {
    let mut packages = allowed(false);
    let accept = [
        // Exactly 300 UTF-16 code units in astral pairs, the transport maximum.
        "\u{1F600}".repeat(150),
        // Interior zero-width no-break spaces are not edge content.
        "AUT\u{FEFF}UMN".to_owned(),
    ];
    for campaign in accept {
        let repo_row = attribution(&campaign, "web");
        let mut repo = Repo::new(vec![], vec![repo_row]);
        let report = run(&actor(), &mut repo, &mut packages).await.unwrap();
        assert_eq!(report.attribution[0].campaign, campaign);
    }
    let reject = [
        // 302 UTF-16 code units in astral pairs exceeds the 300-unit limit.
        "\u{1F600}".repeat(151),
        // Zero-width no-break space is trimmed by String.prototype.trim.
        "\u{FEFF}AUTUMN".to_owned(),
        "AUTUMN\u{FEFF}".to_owned(),
        "\u{2000}AUTUMN".to_owned(),
        "AUTUMN\u{3000}".to_owned(),
        " AUTUMN".to_owned(),
        "AUTUMN ".to_owned(),
        "   ".to_owned(),
        // C1 controls stay rejected even though the transport permits them.
        "\u{85}AUTUMN".to_owned(),
        "AUTUMN\u{85}".to_owned(),
    ];
    for campaign in reject {
        let mut repo = Repo::new(vec![], vec![attribution(&campaign, "web")]);
        assert_eq!(
            run(&actor(), &mut repo, &mut packages)
                .await
                .unwrap_err()
                .code,
            "MARKETING_CAMPAIGN_SOURCE_INVALID"
        );
    }
}

#[tokio::test]
async fn attribution_rows_reject_incoherent_counts_and_keys() {
    let cases = [
        MarketingAttributionRow {
            campaign: String::new(),
            ..attribution("AUTUMN", "web")
        },
        MarketingAttributionRow {
            source: "web\u{7}".into(),
            ..attribution("AUTUMN", "web")
        },
        MarketingAttributionRow {
            source: "x".repeat(301),
            ..attribution("AUTUMN", "web")
        },
        MarketingAttributionRow {
            captured: 0,
            pending: 0,
            handed_off: 0,
            rejected: 0,
            ..attribution("AUTUMN", "web")
        },
        MarketingAttributionRow {
            pending: 2,
            ..attribution("AUTUMN", "web")
        },
        MarketingAttributionRow {
            handed_off: -1,
            ..attribution("AUTUMN", "web")
        },
        MarketingAttributionRow {
            captured: i64::MAX,
            ..attribution("AUTUMN", "web")
        },
    ];
    for row in cases {
        let mut repo = Repo::new(vec![], vec![row]);
        let mut packages = allowed(false);
        let error = run(&actor(), &mut repo, &mut packages).await.unwrap_err();
        assert_eq!(error.code, "MARKETING_CAMPAIGN_SOURCE_INVALID");
    }
}

#[tokio::test]
async fn attribution_rejects_duplicates_descending_order_and_excess() {
    let mut packages = allowed(false);
    let mut duplicate = Repo::new(
        vec![],
        vec![attribution("AUTUMN", "web"), attribution("AUTUMN", "web")],
    );
    assert_eq!(
        run(&actor(), &mut duplicate, &mut packages)
            .await
            .unwrap_err()
            .code,
        "MARKETING_CAMPAIGN_SOURCE_INVALID"
    );

    let mut ascending = Repo::new(
        vec![],
        vec![attribution("AUTUMN", "web"), {
            let mut row = attribution("AUTUMN", "email");
            row.captured = 4;
            row.pending = 2;
            row
        }],
    );
    assert_eq!(
        run(&actor(), &mut ascending, &mut packages)
            .await
            .unwrap_err()
            .code,
        "MARKETING_CAMPAIGN_SOURCE_INVALID"
    );

    let rows = (0..=ATTRIBUTION_LIMIT)
        .map(|index| {
            let mut row = attribution(&format!("campaign-{index:05}"), "web");
            row.captured = 1;
            row.pending = 1;
            row.handed_off = 0;
            row.rejected = 0;
            row
        })
        .collect();
    let mut too_many = Repo::new(vec![], rows);
    assert_eq!(
        run(&actor(), &mut too_many, &mut packages)
            .await
            .unwrap_err()
            .code,
        "MARKETING_CAMPAIGN_TOO_LARGE"
    );
}

#[tokio::test]
async fn exact_attribution_limit_is_accepted() {
    let rows = (0..ATTRIBUTION_LIMIT)
        .map(|index| {
            let mut row = attribution(&format!("campaign-{index:05}"), "web");
            row.captured = 1;
            row.pending = 1;
            row.handed_off = 0;
            row.rejected = 0;
            row
        })
        .collect();
    let mut repo = Repo::new(vec![], rows);
    let mut packages = allowed(false);
    let report = run(&actor(), &mut repo, &mut packages).await.unwrap();
    assert_eq!(report.attribution.len(), ATTRIBUTION_LIMIT);
}

#[tokio::test]
async fn repository_business_failure_propagates_unchanged() {
    let mut repo = Repo::new(vec![], vec![]);
    repo.error = Some(BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_CAMPAIGN_SOURCE_INVALID",
        "Synthetic invalid source.",
    ));
    let mut packages = allowed(false);
    let error = run(&actor(), &mut repo, &mut packages).await.unwrap_err();
    assert_eq!(error.code, "MARKETING_CAMPAIGN_SOURCE_INVALID");
}

#[tokio::test]
async fn purchase_control_shape_is_exact_camel_case() {
    let controls =
        erpx_server_marketing::public::campaign_attribution::MarketingCampaignPurchaseControls {
            new_work: true,
            settle_existing: true,
            history: false,
            revision: "rev-1".into(),
        };
    let value = serde_json::to_value(controls).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "newWork": true,
            "settleExisting": true,
            "history": false,
            "revision": "rev-1"
        })
    );
    let mut map = BTreeMap::new();
    map.insert("campaign".to_owned(), "AUTUMN".to_owned());
    assert_eq!(map["campaign"], "AUTUMN");
}
