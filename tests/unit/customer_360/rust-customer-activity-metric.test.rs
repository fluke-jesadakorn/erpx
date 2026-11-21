use erpx_server_customer_360::public::customer_activity_metric::{
    CustomerActivityBreakdown, CustomerActivityMetricFuture, CustomerActivityMetricGroup,
    CustomerActivityMetricQuery, CustomerActivityMetricRepository, MAX_GROUPS,
    customer_activity_metric,
};
use erpx_server_customer_360::public::customer_timeline::{
    CustomerTimelineActor, CustomerTimelineFuture, CustomerTimelineSupportEntitlement,
    CustomerTimelineView,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const DAY: i64 = 24 * 60 * 60 * 1000;

fn actor(permissions: &[&str], modules: &[&str]) -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: "user".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        role: "sales_manager".into(),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        assignment_id: "assignment".into(),
        expires_at: NOW.get() + 60_000,
    }
}

struct Entitlement(bool);

impl CustomerTimelineSupportEntitlement for Entitlement {
    type Error = BusinessError;

    fn support_history<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> CustomerTimelineFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.0) })
    }
}

struct Repo {
    groups: Vec<CustomerActivityMetricGroup>,
    seen_view: Option<CustomerTimelineView>,
    seen_query: Option<CustomerActivityMetricQuery>,
}

impl CustomerActivityMetricRepository for Repo {
    type Error = BusinessError;

    fn metric<'a>(
        &'a mut self,
        _actor: &'a CustomerTimelineActor,
        view: CustomerTimelineView,
        query: &'a CustomerActivityMetricQuery,
    ) -> CustomerActivityMetricFuture<'a, Vec<CustomerActivityMetricGroup>, Self::Error> {
        Box::pin(async move {
            self.seen_view = Some(view);
            self.seen_query = Some(query.clone());
            Ok(self.groups.clone())
        })
    }
}

fn query(breakdown: CustomerActivityBreakdown) -> CustomerActivityMetricQuery {
    CustomerActivityMetricQuery {
        from: NOW.get() - DAY,
        to: NOW.get(),
        breakdown,
    }
}

fn group(label: &str, value: &str) -> CustomerActivityMetricGroup {
    CustomerActivityMetricGroup {
        label: label.into(),
        value: value.into(),
    }
}

#[tokio::test]
async fn requires_commerce_read_and_validate_range() {
    let mut repo = Repo {
        groups: vec![],
        seen_view: None,
        seen_query: None,
    };
    let mut entitlement = Entitlement(false);
    let denied = actor(&["finance.read"], &[]);
    let error = customer_activity_metric(
        &denied,
        &query(CustomerActivityBreakdown::Month),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_PERMISSION_REQUIRED");
    assert!(repo.seen_query.is_none());

    let a = actor(&["commerce.read"], &[]);
    for (from, to) in [(10, 10), (10, 9), (0, 366 * DAY + 1), (-1, 10)] {
        let error = customer_activity_metric(
            &a,
            &CustomerActivityMetricQuery {
                from,
                to,
                breakdown: CustomerActivityBreakdown::Month,
            },
            &mut repo,
            &mut entitlement,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_ACTIVITY_RANGE_INVALID");
    }
    assert!(repo.seen_query.is_none());
}

#[tokio::test]
async fn gates_support_history_purchase_and_module_sources() {
    let a = actor(
        &[
            "commerce.read",
            "support.read",
            "finance.read",
            "legal.read",
        ],
        &["field_service"],
    );
    let mut entitlement = Entitlement(false);
    let mut repo = Repo {
        groups: vec![],
        seen_view: None,
        seen_query: None,
    };
    customer_activity_metric(
        &a,
        &query(CustomerActivityBreakdown::SourceType),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap();
    let view = repo.seen_view.unwrap();
    assert!(!view.support);
    assert!(view.field_service);
    assert!(view.finance);
    assert!(view.legal);

    let mut entitlement = Entitlement(true);
    let mut repo = Repo {
        groups: vec![],
        seen_view: None,
        seen_query: None,
    };
    customer_activity_metric(
        &a,
        &query(CustomerActivityBreakdown::Month),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap();
    assert!(repo.seen_view.unwrap().support);
    assert_eq!(
        repo.seen_query.unwrap().breakdown,
        CustomerActivityBreakdown::Month
    );
}

#[tokio::test]
async fn accepts_bounded_month_and_source_groups() {
    let a = actor(&["commerce.read"], &[]);
    let mut entitlement = Entitlement(false);
    let mut repo = Repo {
        groups: vec![group("2026-09", "4"), group("2026-10", "9")],
        seen_view: None,
        seen_query: None,
    };
    let groups = customer_activity_metric(
        &a,
        &query(CustomerActivityBreakdown::Month),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(groups.len(), 2);

    let mut repo = Repo {
        groups: vec![group("order", "3"), group("support", "1")],
        seen_view: None,
        seen_query: None,
    };
    let groups = customer_activity_metric(
        &a,
        &query(CustomerActivityBreakdown::SourceType),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(groups.len(), 2);
}

#[tokio::test]
async fn rejects_unbounded_or_malformed_groups() {
    let a = actor(&["commerce.read"], &[]);
    let mut entitlement = Entitlement(false);
    let cases = [
        (
            CustomerActivityBreakdown::SourceType,
            vec![group("unknown", "1")],
        ),
        (
            CustomerActivityBreakdown::Month,
            vec![group("2026-13", "1")],
        ),
        (CustomerActivityBreakdown::Month, vec![group("2026-9", "1")]),
        (
            CustomerActivityBreakdown::SourceType,
            vec![group("order", "-1")],
        ),
        (
            CustomerActivityBreakdown::SourceType,
            vec![group("order", "01")],
        ),
        (
            CustomerActivityBreakdown::SourceType,
            vec![group("order", "1"), group("order", "2")],
        ),
    ];
    for (breakdown, groups) in cases {
        let mut repo = Repo {
            groups,
            seen_view: None,
            seen_query: None,
        };
        let error =
            customer_activity_metric(&a, &query(breakdown), &mut repo, &mut entitlement, NOW)
                .await
                .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_ACTIVITY_METRIC_INVALID");
    }

    let mut repo = Repo {
        groups: (0..=MAX_GROUPS)
            .map(|index| group(&format!("order{}", index % 2), "1"))
            .collect(),
        seen_view: None,
        seen_query: None,
    };
    let error = customer_activity_metric(
        &a,
        &query(CustomerActivityBreakdown::SourceType),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACTIVITY_METRIC_INVALID");
}
