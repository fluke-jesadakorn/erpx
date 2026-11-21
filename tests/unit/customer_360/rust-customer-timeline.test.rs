//! Native Customer 360 timeline domain/application tests. These prove the
//! owner policy, source gates, bounded cursor/limit, ordering and exact page
//! shape against a recording repository; they make no browser or database
//! claim.
#![allow(dead_code)]
use erpx_server_customer_360::public::customer_timeline::{
    CustomerAccount, CustomerTimelineActor, CustomerTimelineCursor, CustomerTimelineFuture,
    CustomerTimelineItem, CustomerTimelinePage, CustomerTimelineQuery, CustomerTimelineRepository,
    CustomerTimelineSupportEntitlement, CustomerTimelineView, LIMITATIONS, customer_timeline,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
    modules: &[&str],
) -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: "user".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
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

fn customer() -> CustomerAccount {
    CustomerAccount {
        id: "customer".into(),
        name: "Acme Customer".into(),
        code: "C-001".into(),
    }
}

fn event(
    source_type: &str,
    prefix: &str,
    source_id: &str,
    at: i64,
    version: i64,
) -> CustomerTimelineItem {
    CustomerTimelineItem {
        id: format!("{prefix}:{source_id}"),
        source_id: source_id.into(),
        source_type: source_type.into(),
        title: format!("Synthetic {source_type} {source_id}"),
        status: "recorded".into(),
        version,
        at,
        href: format!("/synthetic/{source_id}"),
    }
}

#[derive(Default)]
struct Repo {
    customer: Option<CustomerAccount>,
    events: Vec<CustomerTimelineItem>,
    calls: u32,
    seen_view: Option<CustomerTimelineView>,
    seen_cursor: Option<CustomerTimelineCursor>,
    seen_limit: Option<usize>,
    corrupt_order: bool,
    ignore_cursor: bool,
    verify_calls: u32,
}

impl Repo {
    fn with(events: Vec<CustomerTimelineItem>) -> Self {
        Self {
            customer: Some(customer()),
            events,
            ..Self::default()
        }
    }
    fn calls(&self) -> u32 {
        self.calls
    }
}

impl CustomerTimelineRepository for Repo {
    type Error = BusinessError;

    fn customer<'a>(
        &'a mut self,
        _actor: &'a CustomerTimelineActor,
        _customer_id: &'a str,
        view: CustomerTimelineView,
    ) -> CustomerTimelineFuture<'a, Option<CustomerAccount>, Self::Error> {
        Box::pin(async move {
            self.calls += 1;
            self.seen_view = Some(view);
            Ok(self.customer.clone())
        })
    }

    fn verify_order<'a>(
        &'a mut self,
        rows: &'a [CustomerTimelineItem],
        cursor: Option<&'a CustomerTimelineCursor>,
    ) -> CustomerTimelineFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.verify_calls += 1;
            Ok(rows.windows(2).all(|pair| (pair[0].at, pair[0].id.as_str()) >= (pair[1].at, pair[1].id.as_str()))
                && rows.iter().all(|row| cursor.is_none_or(|cursor| (row.at, row.id.as_str()) < (cursor.at, cursor.id.as_str()))))
        })
    }

    fn events<'a>(
        &'a mut self,
        _actor: &'a CustomerTimelineActor,
        _customer_id: &'a str,
        view: CustomerTimelineView,
        cursor: Option<&'a CustomerTimelineCursor>,
        limit: usize,
    ) -> CustomerTimelineFuture<'a, Vec<CustomerTimelineItem>, Self::Error> {
        Box::pin(async move {
            self.calls += 1;
            self.seen_view = Some(view);
            self.seen_cursor = cursor.cloned();
            self.seen_limit = Some(limit);
            let mut rows = self
                .events
                .iter()
                .filter(|row| {
                    let enabled = match row.source_type.as_str() {
                        "support" => view.support,
                        "field_service" => view.field_service,
                        "invoice" => view.finance,
                        "legal" => view.legal,
                        _ => true,
                    };
                    enabled
                        && (self.ignore_cursor || cursor.is_none_or(|cursor| {
                            (row.at, row.id.as_str()) < (cursor.at, cursor.id.as_str())
                        }))
                })
                .cloned()
                .collect::<Vec<_>>();
            rows.sort_by(|left, right| right.at.cmp(&left.at).then_with(|| right.id.cmp(&left.id)));
            rows.truncate(limit);
            if self.corrupt_order { rows.reverse(); }
            Ok(rows)
        })
    }
}

struct Entitlement {
    allowed: bool,
    calls: u32,
}

impl Entitlement {
    fn new(allowed: bool) -> Self {
        Self { allowed, calls: 0 }
    }
}

impl CustomerTimelineSupportEntitlement for Entitlement {
    type Error = BusinessError;

    fn support_history<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> CustomerTimelineFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.calls += 1;
            Ok(self.allowed)
        })
    }
}

async fn run(
    actor: &CustomerTimelineActor,
    query: &CustomerTimelineQuery,
    repo: &mut Repo,
    entitlement: &mut Entitlement,
) -> Result<CustomerTimelinePage, BusinessError> {
    customer_timeline(actor, "customer", query, repo, entitlement, NOW).await
}

fn full_actor() -> CustomerTimelineActor {
    actor(
        "company",
        None,
        None,
        &[
            "commerce.read",
            "support.read",
            "finance.read",
            "legal.read",
        ],
        &["field_service"],
    )
}

#[tokio::test]
async fn requires_commerce_read_and_current_context_before_any_source() {
    let mut repo = Repo::with(vec![]);
    let mut entitlement = Entitlement::new(true);
    let denied = actor("company", None, None, &["finance.read"], &[]);
    let error = run(
        &denied,
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_PERMISSION_REQUIRED");
    assert_eq!(repo.calls(), 0);
    assert_eq!(entitlement.calls, 0);

    let mut expired = full_actor();
    expired.expires_at = NOW.get();
    let error = run(
        &expired,
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(repo.calls(), 0);
}

#[tokio::test]
async fn support_history_requires_purchase_and_does_not_block_other_sources() {
    let mut repo = Repo::with(vec![
        event("order", "order", "order-1", 20, 1),
        event("support", "support", "ticket-1", 10, 2),
    ]);
    let mut denied = Entitlement::new(false);
    let page = run(
        &full_actor(),
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut denied,
    )
    .await
    .unwrap();
    assert_eq!(denied.calls, 1);
    assert!(!repo.seen_view.unwrap().support);
    assert!(!page.coverage.contains(&"Support case history"));
    assert!(page.items.iter().all(|item| item.source_type != "support"));
    assert_eq!(page.items.len(), 1);

    let mut allowed = Entitlement::new(true);
    let page = run(
        &full_actor(),
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut allowed,
    )
    .await
    .unwrap();
    assert!(page.coverage.contains(&"Support case history"));
    assert!(page.items.iter().any(|item| item.source_type == "support"));

    let no_support = actor(
        "company",
        None,
        None,
        &["commerce.read", "finance.read", "legal.read"],
        &["field_service"],
    );
    let mut entitlement = Entitlement::new(true);
    run(
        &no_support,
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap();
    assert_eq!(entitlement.calls, 0);
    assert!(!repo.seen_view.unwrap().support);
}

#[tokio::test]
async fn query_limit_and_cursor_are_bounded() {
    let mut repo = Repo::with(vec![]);
    let mut entitlement = Entitlement::new(true);
    for limit in [0, 101] {
        let error = run(
            &full_actor(),
            &CustomerTimelineQuery {
                limit: Some(limit),
                ..CustomerTimelineQuery::default()
            },
            &mut repo,
            &mut entitlement,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_TIMELINE_LIMIT_INVALID");
    }
    for cursor in ["".to_owned(), "not-a-cursor".to_owned(), "a".repeat(1_001)] {
        let error = run(
            &full_actor(),
            &CustomerTimelineQuery {
                cursor: Some(cursor),
                ..CustomerTimelineQuery::default()
            },
            &mut repo,
            &mut entitlement,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_TIMELINE_CURSOR_INVALID");
    }
    assert_eq!(repo.calls(), 0);

    let error = customer_timeline(
        &full_actor(),
        "bad\u{7}customer",
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_CUSTOMER_INVALID");
    assert_eq!(repo.calls(), 0);
}

#[tokio::test]
async fn pages_are_ordered_unique_and_cursor_continuation_matches_the_full_set() {
    let mut events = (0..40)
        .map(|index| {
            event(
                "order",
                "order",
                &format!("order-{index:02}"),
                1_000 + index,
                1,
            )
        })
        .collect::<Vec<_>>();
    events.reverse();
    let mut repo = Repo::with(events);
    let mut entitlement = Entitlement::new(true);
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = run(
            &full_actor(),
            &CustomerTimelineQuery {
                cursor: cursor.clone(),
                limit: Some(7),
            },
            &mut repo,
            &mut entitlement,
        )
        .await
        .unwrap();
        assert!(page.items.len() <= 7);
        assert!(
            page.items.windows(2).all(|pair| {
                (pair[0].at, pair[0].id.as_str()) > (pair[1].at, pair[1].id.as_str())
            })
        );
        seen.extend(page.items.iter().map(|item| item.id.clone()));
        cursor = page.next_cursor.clone();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 40);
    assert_eq!(seen.first().unwrap(), "order:order-39");
    assert_eq!(seen.last().unwrap(), "order:order-00");
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 40);
}

#[tokio::test]
async fn rejects_repository_rows_in_wrong_cursor_order() {
    let mut repo = Repo::with(vec![event("order", "order", "a", 10, 1), event("order", "order", "z", 10, 1)]);
    repo.corrupt_order = true;
    let mut entitlement = Entitlement::new(true);
    let error = run(&full_actor(), &CustomerTimelineQuery::default(), &mut repo, &mut entitlement).await.unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_SOURCE_INVALID");
    assert_eq!(repo.verify_calls, 1);
}

#[tokio::test]
async fn rejects_repository_rows_at_or_before_exclusive_cursor() {
    let mut repo = Repo::with(vec![event("order", "order", "a", 10, 1), event("order", "order", "z", 10, 1)]);
    let mut entitlement = Entitlement::new(true);
    let first = run(&full_actor(), &CustomerTimelineQuery { cursor: None, limit: Some(1) }, &mut repo, &mut entitlement).await.unwrap();
    repo.ignore_cursor = true;
    let error = run(&full_actor(), &CustomerTimelineQuery { cursor: first.next_cursor, limit: Some(1) }, &mut repo, &mut entitlement).await.unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_SOURCE_INVALID");
    assert_eq!(repo.verify_calls, 2);
}

#[tokio::test]
async fn rejects_duplicate_and_incoherent_source_rows() {
    let good = event("order", "order", "order-1", 10, 1);
    for source in [
        vec![good.clone(), good.clone()],
        vec![CustomerTimelineItem {
            id: "invoice:invoice-1".into(),
            ..good.clone()
        }],
        vec![CustomerTimelineItem {
            title: "bad\u{7}title".into(),
            ..good.clone()
        }],
        vec![CustomerTimelineItem {
            version: 0,
            ..good.clone()
        }],
        vec![CustomerTimelineItem {
            href: "https://evil.example/path".into(),
            ..good.clone()
        }],
        vec![CustomerTimelineItem {
            href: "//evil.example/path".into(),
            ..good.clone()
        }],
        vec![CustomerTimelineItem {
            source_type: "portal".into(),
            ..good.clone()
        }],
    ] {
        let mut repo = Repo::with(source);
        let mut entitlement = Entitlement::new(true);
        let error = run(
            &full_actor(),
            &CustomerTimelineQuery::default(),
            &mut repo,
            &mut entitlement,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_TIMELINE_SOURCE_INVALID");
    }
}

#[tokio::test]
async fn cursor_is_bound_to_the_customer_and_company_scope() {
    let events = (0..20)
        .map(|index| {
            event(
                "order",
                "order",
                &format!("order-{index:02}"),
                1_000 + index,
                1,
            )
        })
        .collect::<Vec<_>>();
    let mut repo = Repo::with(events);
    let mut entitlement = Entitlement::new(true);
    let first = run(
        &full_actor(),
        &CustomerTimelineQuery {
            limit: Some(5),
            ..CustomerTimelineQuery::default()
        },
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap();
    let cursor = first.next_cursor.clone().expect("first page cursor");

    let mut foreign = full_actor();
    foreign.company_id = CompanyId::new("foreign").unwrap();
    let error = run(
        &foreign,
        &CustomerTimelineQuery {
            cursor: Some(cursor.clone()),
            ..CustomerTimelineQuery::default()
        },
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_CURSOR_INVALID");

    let error = customer_timeline(
        &full_actor(),
        "other-customer",
        &CustomerTimelineQuery {
            cursor: Some(cursor),
            ..CustomerTimelineQuery::default()
        },
        &mut repo,
        &mut entitlement,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_CURSOR_INVALID");
}

#[tokio::test]
async fn missing_customer_is_a_scoped_not_found_and_empty_history_is_valid() {
    let mut repo = Repo::default();
    let mut entitlement = Entitlement::new(true);
    let error = run(
        &full_actor(),
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_NOT_FOUND");

    let mut repo = Repo::with(vec![]);
    let page = run(
        &full_actor(),
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap();
    assert!(page.items.is_empty());
    assert!(!page.has_more);
    assert!(page.next_cursor.is_none());
    assert_eq!(
        page.coverage,
        vec![
            "Sales quotations/orders",
            "Fulfillments and customer returns",
            "Internal contact history",
            "Customer follow-up history",
            "Qualified CRM lead/opportunity history",
            "CRM follow-up activities",
            "Support case history",
            "Field Service agreements and visits",
            "Finance invoice status",
            "Legal records linked to orders",
        ]
    );
    assert_eq!(page.limitations, LIMITATIONS);
}

#[tokio::test]
async fn serialized_page_has_exact_additive_keys() {
    let mut repo = Repo::with(vec![event("order", "order", "order-1", 10, 3)]);
    let mut entitlement = Entitlement::new(true);
    let page = run(
        &full_actor(),
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap();
    let value = serde_json::to_value(page).unwrap();
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "customer",
            "items",
            "hasMore",
            "nextCursor",
            "coverage",
            "limitations"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    assert_eq!(
        value["items"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "id",
            "source_id",
            "source_type",
            "title",
            "status",
            "version",
            "at",
            "href"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    assert_eq!(
        value["customer"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        ["id", "name", "code"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
}

#[tokio::test]
async fn rejects_a_customer_row_that_does_not_match_the_request() {
    let mut repo = Repo::with(vec![]);
    repo.customer = Some(CustomerAccount {
        id: "other".into(),
        ..customer()
    });
    let mut entitlement = Entitlement::new(true);
    let error = run(
        &full_actor(),
        &CustomerTimelineQuery::default(),
        &mut repo,
        &mut entitlement,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_TIMELINE_SOURCE_INVALID");
}
