#![allow(dead_code)]
use erpx_server_customer_360::public::customer_timeline::CustomerTimelineActor;
use erpx_server_customer_360::public::directory::{
    CustomerDirectoryEntry, CustomerDirectoryRepository, CustomerDirectoryView, customer_directory,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(permissions: &[&str], modules: &[&str]) -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: "user".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        role: "sales_manager".into(),
        permissions: permissions.iter().map(|v| (*v).to_owned()).collect(),
        modules: modules.iter().map(|v| (*v).to_owned()).collect(),
        assignment_id: "assignment".into(),
        expires_at: NOW.get() + 60_000,
    }
}

#[derive(Default)]
struct Repo {
    entries: Vec<CustomerDirectoryEntry>,
    calls: u32,
    seen_view: Option<CustomerDirectoryView>,
}

impl CustomerDirectoryRepository for Repo {
    type Error = BusinessError;
    fn customers<'a>(
        &'a mut self,
        _actor: &'a CustomerTimelineActor,
        view: CustomerDirectoryView,
    ) -> erpx_server_customer_360::public::directory::CustomerDirectoryFuture<
        'a,
        Vec<CustomerDirectoryEntry>,
        Self::Error,
    > {
        Box::pin(async move {
            self.calls += 1;
            self.seen_view = Some(view);
            Ok(self.entries.clone())
        })
    }
}

#[tokio::test]
async fn requires_commerce_read_and_current_context() {
    let mut repo = Repo::default();
    let denied = actor(&["finance.read"], &[]);
    let err = customer_directory(&denied, &mut repo, NOW)
        .await
        .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_DIRECTORY_PERMISSION_REQUIRED");
    assert_eq!(repo.calls, 0);

    let mut expired = actor(&["commerce.read"], &[]);
    expired.expires_at = NOW.get();
    let err = customer_directory(&expired, &mut repo, NOW)
        .await
        .unwrap_err();
    assert_eq!(err.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(repo.calls, 0);
}

#[tokio::test]
async fn returns_entries_and_gates_sources() {
    let mut repo = Repo {
        entries: vec![CustomerDirectoryEntry {
            id: "customer".into(),
            name: "Acme".into(),
            code: "C-1".into(),
        }],
        ..Repo::default()
    };
    let a = actor(&["commerce.read", "finance.read"], &["field_service"]);
    let page = customer_directory(&a, &mut repo, NOW).await.unwrap();
    assert_eq!(page.len(), 1);
    assert!(repo.seen_view.as_ref().unwrap().finance);
    assert!(repo.seen_view.as_ref().unwrap().field_service);
}
