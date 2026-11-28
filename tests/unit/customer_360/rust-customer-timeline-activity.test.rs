//! Owner-local validation and policy tests for the native Customer 360
//! timeline activity read port. No PostgreSQL or composition mount required.
use erpx_customer_360_server::public::customer_timeline::{
    CustomerTimelineActor, CustomerTimelineFuture, CustomerTimelineSupportEntitlement,
    CustomerTimelineView,
};
use erpx_customer_360_server::public::customer_timeline_activity::{
    CustomerTimelineActivityQuery, CustomerTimelineActivityRepository,
    CustomerTimelineActivityRow, customer_timeline_activity,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const DAY: i64 = 24 * 60 * 60 * 1000;

fn actor(permissions: &[&str], modules: &[&str]) -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: "user".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        role: "reader".into(),
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        assignment_id: "assignment".into(),
        expires_at: NOW.get() + 60_000,
    }
}

fn query() -> CustomerTimelineActivityQuery {
    CustomerTimelineActivityQuery {
        from: NOW.get() - DAY,
        to: NOW.get(),
        customer_id: Some("customer".into()),
        query: None,
    }
}

fn row(at: i64, id: &str) -> CustomerTimelineActivityRow {
    CustomerTimelineActivityRow {
        id: id.into(),
        source_id: "order-main".into(),
        source_type: "order".into(),
        title: "sales · SO-100".into(),
        status: "confirmed".into(),
        version: 1,
        at,
        href: "/commerce/orders/order-main".into(),
        customer_id: "customer".into(),
        customer_name: "Acme Customer".into(),
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
    rows: Vec<CustomerTimelineActivityRow>,
    seen_view: Option<CustomerTimelineView>,
}

impl CustomerTimelineActivityRepository for Repo {
    type Error = BusinessError;
    fn activity<'a>(
        &'a mut self,
        _actor: &'a CustomerTimelineActor,
        view: CustomerTimelineView,
        _query: &'a CustomerTimelineActivityQuery,
        _limit: usize,
    ) -> CustomerTimelineFuture<'a, Vec<CustomerTimelineActivityRow>, Self::Error> {
        Box::pin(async move {
            self.seen_view = Some(view);
            Ok(self.rows.clone())
        })
    }
}

#[test]
fn validates_bounded_query() {
    assert!(query().validate().is_ok());
    for invalid in [
        CustomerTimelineActivityQuery { from: -1, ..query() },
        CustomerTimelineActivityQuery { from: 10, to: 10, ..query() },
        CustomerTimelineActivityQuery { from: 10, to: 9, ..query() },
        CustomerTimelineActivityQuery {
            from: 0,
            to: 366 * DAY + 1,
            ..query()
        },
        CustomerTimelineActivityQuery {
            customer_id: Some(String::new()),
            ..query()
        },
        CustomerTimelineActivityQuery {
            query: Some("a".repeat(201)),
            ..query()
        },
        CustomerTimelineActivityQuery {
            query: Some("line\nbreak".into()),
            ..query()
        },
    ] {
        assert!(invalid.validate().is_err(), "{invalid:?}");
    }
}

#[test]
fn validates_row_shape_and_customer() {
    assert!(row(100, "order:order-main").validate().is_ok());
    let mut bad = row(100, "order:order-main");
    bad.id = "unknown:order-main".into();
    assert!(bad.validate().is_err());
    let mut bad = row(100, "order:order-main");
    bad.customer_id = String::new();
    assert!(bad.validate().is_err());
    let mut bad = row(100, "order:order-main");
    bad.customer_name = String::new();
    assert!(bad.validate().is_err());
    let mut bad = row(100, "order:order-main");
    bad.at = -1;
    assert!(bad.validate().is_err());
}

#[tokio::test]
async fn requires_read_and_applies_source_view() {
    let mut repo = Repo { rows: vec![], seen_view: None };
    let mut entitlement = Entitlement(true);
    let denied = actor(&["finance.read"], &[]);
    let error = customer_timeline_activity(
        &denied,
        &query(),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_PERMISSION_REQUIRED");
    assert!(repo.seen_view.is_none());

    let a = actor(
        &[
            "commerce.read",
            "support.read",
            "finance.read",
            "legal.read",
        ],
        &["field_service"],
    );
    customer_timeline_activity(&a, &query(), &mut repo, &mut entitlement, NOW)
        .await
        .unwrap();
    let view = repo.seen_view.unwrap();
    assert!(view.support && view.field_service && view.finance && view.legal);

    let no_purchase = actor(&["commerce.read", "support.read"], &[]);
    customer_timeline_activity(&no_purchase, &query(), &mut repo, &mut Entitlement(false), NOW)
        .await
        .unwrap();
    assert!(!repo.seen_view.unwrap().support);
}

#[tokio::test]
async fn fails_closed_on_out_of_window_wrong_customer_and_bad_order() {
    let mut repo = Repo { rows: vec![], seen_view: None };
    let a = actor(&["commerce.read"], &[]);
    let mut entitlement = Entitlement(false);

    repo.rows = vec![row(NOW.get() + 1, "order:order-main")];
    assert_eq!(
        customer_timeline_activity(&a, &query(), &mut repo, &mut entitlement, NOW)
            .await
            .unwrap_err()
            .code,
        "CUSTOMER_TIMELINE_ACTIVITY_INVALID"
    );

    let mut other = row(NOW.get() - 100, "order:order-main");
    other.customer_id = "another".into();
    repo.rows = vec![other];
    assert_eq!
        (customer_timeline_activity(&a, &query(), &mut repo, &mut entitlement, NOW)
            .await
            .unwrap_err()
            .code, "CUSTOMER_TIMELINE_ACTIVITY_INVALID");

    repo.rows = vec![
        row(NOW.get() - 300, "order:a"),
        row(NOW.get() - 200, "order:b"),
    ];
    assert!(customer_timeline_activity(&a, &query(), &mut repo, &mut entitlement, NOW)
        .await
        .is_err());

    repo.rows = vec![
        row(NOW.get() - 200, "order:a"),
        row(NOW.get() - 300, "order:b"),
    ];
    let rows = customer_timeline_activity(&a, &query(), &mut repo, &mut entitlement, NOW)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
}
