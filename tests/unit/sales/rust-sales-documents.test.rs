//! Native Sales document list policy and row validation tests.
use erpx_server_sales::public::sales_documents::{
    SalesDocumentCursor, SalesDocumentKind, SalesDocumentReadActor, SalesDocumentReadFuture,
    SalesDocumentReadRepository, SalesDocumentRow, sales_documents,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use std::sync::{Arc, Mutex};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn actor(permissions: &[&str], modules: &[&str]) -> SalesDocumentReadActor {
    SalesDocumentReadActor {
        user_id: "owner".into(),
        company_id: company(),
        branch_id: None,
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        expires_at: NOW.get() + 60_000,
    }
}

fn row(id: &str, created_at: i64) -> SalesDocumentRow {
    SalesDocumentRow {
        id: id.into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        kind: "quote".into(),
        number: format!("QT-{id}"),
        party_id: "customer".into(),
        party_name: "Customer".into(),
        state: "draft".into(),
        order_date: "2026-11-04".into(),
        due_date: "2026-11-18".into(),
        currency: "THB".into(),
        subtotal_minor: 1_000,
        tax_minor: 70,
        total_minor: 1_070,
        version: 1,
        created_at,
    }
}

#[derive(Default)]
struct Probe {
    rows: Vec<SalesDocumentRow>,
    cursor: Arc<Mutex<Option<SalesDocumentCursor>>>,
    calls: usize,
}

impl SalesDocumentReadRepository for Probe {
    type Error = BusinessError;

    #[allow(clippy::too_many_arguments)]
    fn page<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: SalesDocumentKind,
        _: &'a str,
        _: Option<&'a str>,
        cursor: Option<&'a SalesDocumentCursor>,
        _: usize,
    ) -> SalesDocumentReadFuture<'a, Vec<SalesDocumentRow>, Self::Error> {
        self.calls += 1;
        *self.cursor.lock().unwrap() = cursor.cloned();
        Box::pin(async move { Ok(std::mem::take(&mut self.rows)) })
    }
}

#[tokio::test]
async fn policy_requires_permission_module_and_current_context() {
    let mut denied = Probe::default();
    let error = sales_documents(
        &actor(&[], &["sales"]),
        &mut denied,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "SALES_PERMISSION_REQUIRED");
    assert_eq!(denied.calls, 0);

    let mut module = Probe::default();
    let error = sales_documents(
        &actor(&["commerce.read"], &["finance"]),
        &mut module,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_MODULE_FORBIDDEN");
    assert_eq!(module.calls, 0);

    let mut expired = actor(&["commerce.read"], &["sales"]);
    expired.expires_at = NOW.get();
    let mut probe = Probe::default();
    let error = sales_documents(
        &expired,
        &mut probe,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(probe.calls, 0);
}

#[tokio::test]
async fn list_is_bounded_and_returns_a_scoped_cursor() {
    let mut probe = Probe {
        rows: (0..31)
            .map(|index| row(&format!("order-{index:02}"), 10_000 - index))
            .collect(),
        ..Probe::default()
    };
    let page = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut probe,
        SalesDocumentKind::Quote,
        "customer",
        Some("draft"),
        None,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 30);
    assert!(page.has_more);
    assert_eq!(page.items[0].id, "order-00");
    assert_eq!(page.items[29].id, "order-29");
    let cursor = page.next_cursor.expect("overflow cursor");
    assert!(!cursor.is_empty());

    let last = page.items.last().unwrap().clone();
    let mut second = Probe {
        rows: vec![row("order-30", 9_970)],
        ..Probe::default()
    };
    let page = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut second,
        SalesDocumentKind::Quote,
        "customer",
        Some("draft"),
        Some(&cursor),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(!page.has_more);
    assert!(page.next_cursor.is_none());
    let seen = second
        .cursor
        .lock()
        .unwrap()
        .clone()
        .expect("cursor forwarded");
    assert_eq!(seen.id, last.id);
    assert_eq!(seen.created_at, last.created_at);
}

#[tokio::test]
async fn malformed_search_status_and_cursor_fail_before_source_read() {
    let mut probe = Probe::default();
    let error = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut probe,
        SalesDocumentKind::Quote,
        &"x".repeat(201),
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENTS_REQUEST_INVALID");

    let mut probe = Probe::default();
    let error = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut probe,
        SalesDocumentKind::Quote,
        "bad\u{0}search",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENTS_REQUEST_INVALID");

    let mut probe = Probe::default();
    let error = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut probe,
        SalesDocumentKind::Quote,
        "",
        Some("paid"),
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENT_STATUS_INVALID");

    let mut probe = Probe::default();
    let error = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut probe,
        SalesDocumentKind::Quote,
        "",
        None,
        Some("not-a-cursor"),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENTS_CURSOR_INVALID");
    assert_eq!(probe.calls, 0);
}

#[tokio::test]
async fn cursor_scope_is_bound_to_company_scope_kind_search_and_status() {
    let mut first = Probe {
        rows: (0..31)
            .map(|index| row(&format!("order-{index:02}"), 10_000 - index))
            .collect(),
        ..Probe::default()
    };
    let page = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut first,
        SalesDocumentKind::Quote,
        "customer",
        Some("draft"),
        None,
        NOW,
    )
    .await
    .unwrap();
    let cursor = page.next_cursor.unwrap();

    for (kind, search, status) in [
        (SalesDocumentKind::Sales, "customer", Some("draft")),
        (SalesDocumentKind::Quote, "other", Some("draft")),
        (SalesDocumentKind::Quote, "customer", Some("submitted")),
    ] {
        let mut probe = Probe::default();
        let error = sales_documents(
            &actor(&["commerce.read"], &["sales"]),
            &mut probe,
            kind,
            search,
            status,
            Some(&cursor),
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "SALES_DOCUMENTS_CURSOR_INVALID");
        assert_eq!(probe.calls, 0);
    }
}

#[tokio::test]
async fn foreign_mismatched_duplicate_and_nonmonotonic_rows_are_rejected() {
    let cases = [
        {
            let mut value = row("order-1", 10);
            value.company_id = "foreign".into();
            value
        },
        {
            let mut value = row("order-1", 10);
            value.kind = "sales".into();
            value
        },
        {
            let mut value = row("order-1", 10);
            value.total_minor += 1;
            value
        },
        {
            let mut value = row("order-1", 10);
            value.state = "unknown".into();
            value
        },
        {
            let mut value = row("order-1", 10);
            value.order_date = "2026-02-30".into();
            value
        },
        {
            let mut value = row("order-1", 10);
            value.currency = "thb".into();
            value
        },
        {
            let mut value = row("order-1", 10);
            value.version = 0;
            value
        },
    ];
    for (index, value) in cases.into_iter().enumerate() {
        let mut probe = Probe {
            rows: vec![value],
            ..Probe::default()
        };
        let error = sales_documents(
            &actor(&["commerce.read"], &["sales"]),
            &mut probe,
            SalesDocumentKind::Quote,
            "",
            None,
            None,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "SALES_DOCUMENTS_SOURCE_INVALID", "case {index}");
    }

    let duplicate = Probe {
        rows: vec![row("order-1", 10), row("order-1", 9)],
        ..Probe::default()
    };
    let mut duplicate = duplicate;
    let error = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut duplicate,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENTS_SOURCE_INVALID");

    let mut nonmonotonic = Probe {
        rows: vec![row("order-1", 9), row("order-2", 10)],
        ..Probe::default()
    };
    let error = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut nonmonotonic,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENTS_SOURCE_INVALID");
}

#[tokio::test]
async fn selected_actor_scope_is_enforced_on_every_row() {
    let mut current = actor(&["commerce.read"], &["sales"]);
    current.branch_id = Some(RecordId::new("branch").unwrap());
    let mut probe = Probe {
        rows: vec![row("order-1", 10)],
        ..Probe::default()
    };
    let page = sales_documents(
        &current,
        &mut probe,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);

    let mut probe = Probe {
        rows: vec![row("order-1", 10)],
        ..Probe::default()
    };
    let mut foreign_branch = row("order-1", 10);
    foreign_branch.branch_id = Some("foreign-branch".into());
    probe.rows[0] = foreign_branch;
    let error = sales_documents(
        &current,
        &mut probe,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENTS_SOURCE_INVALID");

    let mut current = actor(&["commerce.read"], &["sales"]);
    current.department_id = Some(RecordId::new("department").unwrap());
    let mut probe = Probe {
        rows: vec![row("order-1", 10)],
        ..Probe::default()
    };
    let error = sales_documents(
        &current,
        &mut probe,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_DOCUMENTS_SOURCE_INVALID");
}

#[tokio::test]
async fn serialized_page_has_exact_public_keys() {
    let mut probe = Probe {
        rows: vec![row("order-1", 10)],
        ..Probe::default()
    };
    let page = sales_documents(
        &actor(&["commerce.read"], &["sales"]),
        &mut probe,
        SalesDocumentKind::Quote,
        "",
        None,
        None,
        NOW,
    )
    .await
    .unwrap();
    let value = serde_json::to_value(&page).unwrap();
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        ["items", "nextCursor", "hasMore"]
            .into_iter()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>()
    );
    let row = value["items"][0].as_object().unwrap();
    assert_eq!(
        row.keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "id",
            "company_id",
            "branch_id",
            "department_id",
            "kind",
            "number",
            "party_id",
            "party_name",
            "state",
            "order_date",
            "due_date",
            "currency",
            "subtotal_minor",
            "tax_minor",
            "total_minor",
            "version",
            "created_at",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>()
    );
}
