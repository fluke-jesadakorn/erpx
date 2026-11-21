//! Native Sales customer-notification settings/queue/retry rules.
use erpx_server_sales::public::notification_commands::{
    NotificationCommandRepository, NotificationFuture, NotificationPurpose, QueuePlan,
    RetryPlan, SettingsPlan, SettingsAdapter, QueueNotification, RetryNotification,
    UpdateSettings, NotificationActor, DeliveryFacts, NoticeSource, NotificationReceiptSnapshot,
    queue_notification, retry_notification, update_settings,
};
use erpx_server_shared::public::BusinessError;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_800_000_000_000;

fn actor(permissions: &[&str]) -> NotificationActor {
    NotificationActor {
        user_id: "reviewer".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        assignment_id: "assignment".into(),
        role: "sales_manager".into(),
        session_id: None,
        assignment_version: None,
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn facts() -> DeliveryFacts {
    DeliveryFacts {
        fulfillment_id: "fulfillment".into(),
        fulfillment_version: 1,
        fulfillment_number: "F-1".into(),
        fulfillment_date: "2026-11-02".into(),
        order_id: "order".into(),
        order_number: "SO-1".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        party_id: "party".into(),
        party_name: "Customer".into(),
        party_version: 1,
        notification_email: Some("Receiver@Example.invalid".into()),
        notification_consent: Some("granted".into()),
        adapter: Some("sandbox".into()),
        settings_version: Some(1),
    }
}

fn source(status: &str) -> NoticeSource {
    NoticeSource {
        id: "notice".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        fulfillment_id: "fulfillment".into(),
        order_id: "order".into(),
        party_id: "party".into(),
        recipient_email: "receiver@example.invalid".into(),
        subject: "Delivery F-1 for order SO-1".into(),
        body_text: "Immutable body".into(),
        adapter: "sandbox".into(),
        status: status.into(),
        version: 2,
        attempts: 1,
        provider_message_id: None,
        created_by: "owner".into(),
        order_state: Some("confirmed".into()),
        customer_status: Some("active".into()),
        notification_email: Some("receiver@example.invalid".into()),
        notification_consent: Some("granted".into()),
        current_adapter: Some("sandbox".into()),
        source_company: Some("company".into()),
        customer_company: Some("company".into()),
        source_hash: "a".repeat(64),
    }
}

#[derive(Default)]
struct Fake {
    settings: Option<(String, i64)>,
    receipt: Option<NotificationReceiptSnapshot>,
    authority: bool,
    feature: bool,
    existing: bool,
    capability: Vec<NotificationPurpose>,
    notice_status: String,
    commits: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    fn ready() -> Self {
        Self {
            authority: true,
            feature: true,
            notice_status: "failed".into(),
            ..Default::default()
        }
    }
}

impl NotificationCommandRepository for Fake {
    type Error = BusinessError;

    fn lock_command<'a>(&'a mut self, _: &'a str) -> NotificationFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }
    fn digest<'a>(&'a mut self, value: &'a str) -> NotificationFuture<'a, String, Self::Error> {
        Box::pin(async move {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            Ok(format!("{:064x}", hasher.finish()))
        })
    }
    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> NotificationFuture<'a, Option<NotificationReceiptSnapshot>, Self::Error> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }
    fn settings<'a>(
        &'a mut self,
        _: &'a str,
    ) -> NotificationFuture<'a, Option<(String, i64)>, Self::Error> {
        let settings = self.settings.clone();
        Box::pin(async move { Ok(settings) })
    }
    fn delivery<'a>(
        &'a mut self,
        _: &'a NotificationActor,
        _: &'a str,
    ) -> NotificationFuture<'a, Option<DeliveryFacts>, Self::Error> {
        Box::pin(async { Ok(Some(facts())) })
    }
    fn delivery_lines<'a>(
        &'a mut self,
        _: &'a str,
    ) -> NotificationFuture<'a, Vec<(String, i64, String)>, Self::Error> {
        Box::pin(async { Ok(vec![("SKU-1".into(), 2000, "pcs".into())]) })
    }
    fn existing<'a>(
        &'a mut self,
        _: &'a str,
    ) -> NotificationFuture<'a, Option<String>, Self::Error> {
        let existing = self.existing;
        Box::pin(async move { Ok(existing.then(|| "pending".to_owned())) })
    }
    fn notice<'a>(
        &'a mut self,
        _: &'a str,
    ) -> NotificationFuture<'a, Option<NoticeSource>, Self::Error> {
        let status = self.notice_status.clone();
        Box::pin(async move { Ok(Some(source(&status))) })
    }
    fn feature_ready<'a>(
        &'a mut self,
        _: &'a str,
        _: i64,
    ) -> NotificationFuture<'a, bool, Self::Error> {
        let feature = self.feature;
        Box::pin(async move { Ok(feature) })
    }
    fn capability<'a>(
        &'a mut self,
        _: &'a str,
        purpose: NotificationPurpose,
        _: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        self.capability.push(purpose);
        Box::pin(async { Ok(()) })
    }
    fn current_authority<'a>(
        &'a mut self,
        _: &'a NotificationActor,
        _: &'a str,
        _: i64,
    ) -> NotificationFuture<'a, bool, Self::Error> {
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }
    fn commit_settings<'a>(
        &'a mut self,
        plan: &'a SettingsPlan<'a>,
        _: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        self.commits.lock().unwrap().push(format!("settings:{}", plan.receipt.adapter));
        Box::pin(async { Ok(()) })
    }
    fn commit_queue<'a>(
        &'a mut self,
        plan: &'a QueuePlan<'a>,
        _: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        self.commits.lock().unwrap().push(format!(
            "queue:{}:{}:{}",
            plan.id, plan.subject, plan.body
        ));
        Box::pin(async { Ok(()) })
    }
    fn commit_retry<'a>(
        &'a mut self,
        plan: &'a RetryPlan<'a>,
        _: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        self.commits.lock().unwrap().push(format!("retry:{}", plan.receipt.version));
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn settings_require_approver_feature_and_cas() {
    let mut repo = Fake::ready();
    repo.settings = Some(("disabled".into(), 2));
    let receipt = update_settings(
        &actor(&["commerce.read", "commerce.sales", "commerce.approve"]),
        UpdateSettings { expected_version: 2, adapter: SettingsAdapter::Sandbox },
        "notification-settings-0001",
        &mut repo,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.version, 3);
    assert_eq!(receipt.adapter, "sandbox");
    assert_eq!(repo.capability, [NotificationPurpose::NewWork]);

    let mut stale = Fake::ready();
    stale.settings = Some(("disabled".into(), 1));
    assert_eq!(
        update_settings(
            &actor(&["commerce.read", "commerce.sales", "commerce.approve"]),
            UpdateSettings { expected_version: 2, adapter: SettingsAdapter::Sandbox },
            "notification-settings-0002",
            &mut stale,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "VERSION_CONFLICT"
    );
    let mut role = Fake::ready();
    assert_eq!(
        update_settings(
            &actor(&["commerce.read", "commerce.sales"]),
            UpdateSettings { expected_version: 0, adapter: SettingsAdapter::Sandbox },
            "notification-settings-0003",
            &mut role,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "FOLLOWUP_ACCESS_CHANGED"
    );
    let mut feature = Fake::ready();
    feature.feature = false;
    assert_eq!(
        update_settings(
            &actor(&["commerce.read", "commerce.sales", "commerce.approve"]),
            UpdateSettings { expected_version: 0, adapter: SettingsAdapter::Sandbox },
            "notification-settings-0004",
            &mut feature,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "FEATURE_COMING_SOON"
    );
}

#[tokio::test]
async fn queue_requires_consent_version_provider_and_an_absent_notice() {
    let mut repo = Fake::ready();
    let receipt = queue_notification(
        &actor(&["commerce.read", "commerce.sales"]),
        "fulfillment",
        QueueNotification { expected_version: 1, note: "Carrier confirmed the delivery window".into() },
        "notification-queue-0001",
        &mut repo,
        NOW,
        "customer-notification-1",
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "customer-notification-1");
    assert_eq!(receipt.status, "pending");
    assert_eq!(repo.capability, [NotificationPurpose::NewWork]);
    let commit = repo.commits.lock().unwrap().clone();
    assert!(commit[0].contains("Delivery F-1 for order SO-1"));
    assert!(commit[0].contains("SKU-1: 2 pcs"));

    let mut version = Fake::ready();
    let mut repo_with_existing = Fake::ready();
    repo_with_existing.existing = true;
    assert_eq!(
        queue_notification(
            &actor(&["commerce.read", "commerce.sales"]),
            "fulfillment",
            QueueNotification { expected_version: 2, note: "Changed delivery source".into() },
            "notification-queue-0002",
            &mut version,
            NOW,
            "customer-notification-2",
        )
        .await
        .unwrap_err()
        .code,
        "VERSION_CONFLICT"
    );
    assert_eq!(
        queue_notification(
            &actor(&["commerce.read", "commerce.sales"]),
            "fulfillment",
            QueueNotification { expected_version: 1, note: "Duplicate customer notice".into() },
            "notification-queue-0003",
            &mut repo_with_existing,
            NOW,
            "customer-notification-3",
        )
        .await
        .unwrap_err()
        .code,
        "CUSTOMER_ALREADY_NOTIFIED"
    );
}

#[tokio::test]
async fn retry_only_settles_one_failed_notice_and_replays_without_writes() {
    let mut repo = Fake::ready();
    let receipt = retry_notification(
        &actor(&["commerce.read", "commerce.sales"]),
        "notice",
        RetryNotification { expected_version: 2 },
        "notification-retry-0001",
        &mut repo,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.version, 3);
    assert_eq!(receipt.status, "pending");
    assert_eq!(repo.capability, [NotificationPurpose::SettleExisting]);
    assert_eq!(repo.commits.lock().unwrap().as_slice(), ["retry:3"]);

    // A delivered notice cannot be retried.
    let mut delivered = Fake::ready();
    delivered.notice_status = "sent".into();
    assert_eq!(
        retry_notification(
            &actor(&["commerce.read", "commerce.sales"]),
            "notice",
            RetryNotification { expected_version: 2 },
            "notification-retry-0003",
            &mut delivered,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "CUSTOMER_NOTIFICATION_NOT_FAILED"
    );
    let mut stale = Fake::ready();
    assert_eq!(
        retry_notification(
            &actor(&["commerce.read", "commerce.sales"]),
            "notice",
            RetryNotification { expected_version: 1 },
            "notification-retry-0002",
            &mut stale,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "VERSION_CONFLICT"
    );
}
