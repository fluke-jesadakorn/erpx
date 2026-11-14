#![allow(dead_code)]
use erpx_server_customer_360::public::customer_timeline::CustomerTimelineActor;
use erpx_server_customer_360::public::directory::{
    CustomerDirectoryCursor, CustomerDirectoryEntry, CustomerDirectoryPage, CustomerDirectoryQuery,
    CustomerDirectoryRepository, CustomerDirectoryView, customer_directory_page,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(company: &str, permissions: &[&str], modules: &[&str]) -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: "user".into(),
        company_id: CompanyId::new(company).unwrap(),
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

fn entry(id: &str, name: &str) -> CustomerDirectoryEntry {
    CustomerDirectoryEntry {
        id: id.into(),
        name: name.into(),
        code: format!("C-{id}"),
    }
}

#[derive(Default)]
struct Repo {
    entries: Vec<CustomerDirectoryEntry>,
    seen_cursor: Option<CustomerDirectoryCursor>,
    seen_limit: Option<usize>,
    calls: u32,
}

impl CustomerDirectoryRepository for Repo {
    type Error = BusinessError;

    fn customers<'a>(
        &'a mut self,
        _actor: &'a CustomerTimelineActor,
        _view: CustomerDirectoryView,
    ) -> erpx_server_customer_360::public::directory::CustomerDirectoryFuture<
        'a,
        Vec<CustomerDirectoryEntry>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.entries.clone()) })
    }

    fn customers_page<'a>(
        &'a mut self,
        _actor: &'a CustomerTimelineActor,
        _view: CustomerDirectoryView,
        cursor: Option<&'a CustomerDirectoryCursor>,
        limit: usize,
    ) -> erpx_server_customer_360::public::directory::CustomerDirectoryFuture<
        'a,
        Vec<CustomerDirectoryEntry>,
        Self::Error,
    > {
        Box::pin(async move {
            self.calls += 1;
            self.seen_cursor = cursor.cloned();
            self.seen_limit = Some(limit);
            Ok(self
                .entries
                .iter()
                .filter(|entry| {
                    cursor.is_none_or(|cursor| {
                        (entry.name.as_str(), entry.id.as_str())
                            > (cursor.name.as_str(), cursor.id.as_str())
                    })
                })
                .cloned()
                .collect())
        })
    }
}

#[tokio::test]
async fn requires_commerce_read_and_current_context() {
    let mut repo = Repo::default();
    let denied = actor("company", &["finance.read"], &[]);
    let error =
        customer_directory_page(&denied, &CustomerDirectoryQuery::default(), &mut repo, NOW)
            .await
            .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_DIRECTORY_PERMISSION_REQUIRED");
    assert_eq!(repo.calls, 0);

    let mut expired = actor("company", &["commerce.read"], &[]);
    expired.expires_at = NOW.get();
    let error =
        customer_directory_page(&expired, &CustomerDirectoryQuery::default(), &mut repo, NOW)
            .await
            .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(repo.calls, 0);
}

#[tokio::test]
async fn validates_bounded_limit_and_opaque_cursor() {
    let mut repo = Repo::default();
    let a = actor("company", &["commerce.read"], &[]);
    for limit in [Some(0), Some(202)] {
        let query = CustomerDirectoryQuery {
            cursor: None,
            limit,
        };
        let error = customer_directory_page(&a, &query, &mut repo, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_DIRECTORY_LIMIT_INVALID");
    }
    let query = CustomerDirectoryQuery {
        cursor: Some("not-a-cursor".into()),
        limit: Some(10),
    };
    let error = customer_directory_page(&a, &query, &mut repo, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_DIRECTORY_CURSOR_INVALID");
    assert_eq!(repo.calls, 0);
}

#[tokio::test]
async fn pages_and_binds_cursor_to_company_and_source_gates() {
    let mut repo = Repo {
        entries: vec![entry("c1", "Alpha"), entry("c2", "Bravo")],
        ..Repo::default()
    };
    let a = actor(
        "company",
        &["commerce.read", "finance.read"],
        &["field_service"],
    );
    let first = customer_directory_page(
        &a,
        &CustomerDirectoryQuery {
            cursor: None,
            limit: Some(1),
        },
        &mut repo,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(repo.seen_limit, Some(2));
    assert_eq!(first.items.len(), 1);
    assert!(first.has_more);
    let cursor = first.next_cursor.expect("next cursor");
    assert_eq!(first.items[0].id, "c1");

    let second = customer_directory_page(
        &a,
        &CustomerDirectoryQuery {
            cursor: Some(cursor.clone()),
            limit: Some(1),
        },
        &mut repo,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        repo.seen_cursor,
        Some(CustomerDirectoryCursor {
            name: "Alpha".into(),
            id: "c1".into()
        })
    );
    assert_eq!(second.items.len(), 1);
    assert!(!second.has_more);
    assert_eq!(second.next_cursor, None);

    let other_company = actor(
        "other",
        &["commerce.read", "finance.read"],
        &["field_service"],
    );
    let error = customer_directory_page(
        &other_company,
        &CustomerDirectoryQuery {
            cursor: Some(cursor.clone()),
            limit: Some(1),
        },
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_DIRECTORY_CURSOR_INVALID");

    let other_view = actor("company", &["commerce.read"], &[]);
    let error = customer_directory_page(
        &other_view,
        &CustomerDirectoryQuery {
            cursor: Some(cursor),
            limit: Some(1),
        },
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_DIRECTORY_CURSOR_INVALID");
}

#[tokio::test]
async fn rejects_duplicate_and_invalid_rows() {
    let mut repo = Repo {
        entries: vec![entry("c1", "Alpha"), entry("c1", "Alpha again")],
        ..Repo::default()
    };
    let a = actor("company", &["commerce.read"], &[]);
    let error = customer_directory_page(&a, &CustomerDirectoryQuery::default(), &mut repo, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_DIRECTORY_SOURCE_INVALID");

    let mut repo = Repo {
        entries: vec![CustomerDirectoryEntry {
            id: String::new(),
            name: "Bad".into(),
            code: "C-1".into(),
        }],
        ..Repo::default()
    };
    let error = customer_directory_page(&a, &CustomerDirectoryQuery::default(), &mut repo, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_DIRECTORY_ENTRY_INVALID");
}

#[tokio::test]
async fn first_page_defaults_to_bounded_maximum() {
    let mut repo = Repo {
        entries: vec![entry("c1", "Alpha")],
        ..Repo::default()
    };
    let a = actor("company", &["commerce.read"], &[]);
    let page: CustomerDirectoryPage =
        customer_directory_page(&a, &CustomerDirectoryQuery::default(), &mut repo, NOW)
            .await
            .unwrap();
    assert_eq!(repo.seen_limit, Some(202));
    assert_eq!(page.items.len(), 1);
    assert!(!page.has_more);
}
