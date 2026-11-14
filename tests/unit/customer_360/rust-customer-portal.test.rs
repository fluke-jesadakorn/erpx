use erpx_server_customer_360::public::customer_portal::{
    CustomerPortalEffects, CustomerPortalRepository, CustomerPortalSharedFiles, CustomerSharedFile,
    PortalAction, PortalActionFact, PortalActionRow, PortalBinding, PortalCommandEvidence,
    PortalCommandFacts, PortalCommandInput, PortalCommandPurpose, PortalCustomerActor,
    PortalFulfillment, PortalHolding, PortalPurchaseScope, customer_portal_command,
    customer_portal_projection,
};
use erpx_server_shared::public::BusinessError;
use std::collections::BTreeMap;

const NOW: i64 = 1_800_000_000_000;

fn actor() -> PortalCustomerActor {
    PortalCustomerActor::new(
        "user",
        "User@Example.com",
        Some(PortalHolding {
            id: "holding".into(),
            slug: "acme".into(),
            name: "Acme".into(),
        }),
    )
    .unwrap()
}

fn binding() -> PortalBinding {
    PortalBinding {
        binding_id: "binding".into(),
        company_id: "company".into(),
        order_id: "order".into(),
        customer_id: "customer".into(),
        email: "user@example.com".into(),
        status: "approved".into(),
        expires_at: NOW + 60_000,
        binding_version: 3,
        order_version: 4,
        number: "SO-100".into(),
        state: "confirmed".into(),
    }
}

fn input(action: PortalAction) -> PortalCommandInput {
    PortalCommandInput {
        binding_id: "binding".into(),
        expected_binding_version: 3,
        action,
        note: String::new(),
        attachment_set_ids: Vec::new(),
    }
}

fn item(binding_id: &str) -> erpx_server_customer_360::public::customer_portal::CustomerPortalItem {
    erpx_server_customer_360::public::customer_portal::CustomerPortalItem {
        binding_id: binding_id.into(),
        binding_version: 3,
        order_id: "order".into(),
        number: "SO-100".into(),
        kind: "sales".into(),
        state: "confirmed".into(),
        due_date: "2026-11-30".into(),
        currency: "THB".into(),
        updated_at: NOW,
        fulfillments: vec![PortalFulfillment {
            number: "F-1".into(),
            created_at: NOW,
            recorded: true,
        }],
        actions: vec![PortalActionFact {
            action: "acknowledge_delivery".into(),
            created_at: NOW,
        }],
        shared_files: Vec::new(),
        purchase: None,
        support_threads: None,
    }
}

#[derive(Default)]
struct Repo {
    projection: Vec<erpx_server_customer_360::public::customer_portal::CustomerPortalItem>,
    prior: Option<PortalActionRow>,
    binding: Option<PortalBinding>,
    identity: bool,
    existing: Option<PortalActionRow>,
    recorded: Vec<PortalCommandFacts>,
}

impl CustomerPortalRepository for Repo {
    type Error = BusinessError;

    fn projection<'a>(
        &'a mut self,
        _actor: &'a PortalCustomerActor,
        _scope: &'a PortalPurchaseScope,
        _now: i64,
        _read_id: &'a str,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<
        'a,
        Vec<erpx_server_customer_360::public::customer_portal::CustomerPortalItem>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.projection.clone()) })
    }

    fn replay<'a>(
        &'a mut self,
        _key: &'a str,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<
        'a,
        Option<PortalActionRow>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.prior.clone()) })
    }

    fn binding<'a>(
        &'a mut self,
        _binding_id: &'a str,
        _email: &'a str,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<
        'a,
        Option<PortalBinding>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.binding.clone()) })
    }

    fn identity<'a>(
        &'a mut self,
        _binding_id: &'a str,
        _actor: &'a PortalCustomerActor,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<'a, bool, Self::Error>
    {
        Box::pin(async move { Ok(self.identity) })
    }

    fn existing<'a>(
        &'a mut self,
        _binding_id: &'a str,
        _action: PortalAction,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<
        'a,
        Option<PortalActionRow>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.existing.clone()) })
    }

    fn record<'a>(
        &'a mut self,
        command: &'a PortalCommandFacts,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.recorded.push(command.clone());
            Ok(())
        })
    }
}

#[derive(Default)]
struct Files {
    shared: BTreeMap<String, Vec<CustomerSharedFile>>,
}

impl CustomerPortalSharedFiles for Files {
    type Error = BusinessError;

    fn shared_files<'a>(
        &'a mut self,
        _actor: &'a PortalCustomerActor,
        _binding_ids: &'a [String],
        _now: i64,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<
        'a,
        BTreeMap<String, Vec<CustomerSharedFile>>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.shared.clone()) })
    }
}

#[derive(Default)]
struct Effects {
    authorized: Vec<PortalCommandPurpose>,
    evidence: u32,
    notified: Vec<String>,
    audited: Vec<String>,
    outboxed: Vec<(String, usize)>,
    fail_authorize: bool,
}

impl CustomerPortalEffects for Effects {
    type Error = BusinessError;

    fn authorize<'a>(
        &'a mut self,
        _actor: &'a PortalCustomerActor,
        _binding: &'a PortalBinding,
        purpose: PortalCommandPurpose,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if self.fail_authorize {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Forbidden,
                    "FEATURE_COMING_SOON",
                    "Not released.",
                ));
            }
            self.authorized.push(purpose);
            Ok(())
        })
    }

    fn evidence<'a>(
        &'a mut self,
        _command: &'a PortalCommandFacts,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<
        'a,
        Option<PortalCommandEvidence>,
        Self::Error,
    > {
        Box::pin(async move {
            self.evidence += 1;
            Ok(Some(PortalCommandEvidence {
                manifest: vec![serde_json::json!({"attachment":"set-1"})],
            }))
        })
    }

    fn notify<'a>(
        &'a mut self,
        _command: &'a PortalCommandFacts,
        message: &'a str,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.notified.push(message.into());
            Ok(())
        })
    }

    fn audit<'a>(
        &'a mut self,
        _command: &'a PortalCommandFacts,
        event_id: &'a str,
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.audited.push(event_id.into());
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        _command: &'a PortalCommandFacts,
        event_id: &'a str,
        manifest: &'a [serde_json::Value],
    ) -> erpx_server_customer_360::public::customer_portal::PortalFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.outboxed.push((event_id.into(), manifest.len()));
            Ok(())
        })
    }
}

#[tokio::test]
async fn rejects_invalid_identity_and_purchase_scope() {
    assert!(PortalCustomerActor::new("user", "not-an-email", None).is_err());
    assert!(PortalCustomerActor::new("", "user@example.com", None).is_err());
    assert!(PortalPurchaseScope::enforced(vec!["company".into(), "company".into()]).is_err());
    assert!(PortalPurchaseScope::enforced(vec!["".into()]).is_err());
}

#[tokio::test]
async fn projection_merges_files_and_bounds_read_id() {
    let actor = actor();
    let mut first = item("binding");
    first.purchase = Some(serde_json::json!({"history":true,"revision":2}));
    first.support_threads = Some(serde_json::json!([{"requestId":"thread"}]));
    let mut repo = Repo {
        projection: vec![first, item("binding-2")],
        ..Repo::default()
    };
    let mut files = Files::default();
    files.shared.insert(
        "binding".into(),
        vec![CustomerSharedFile {
            link_id: "link".into(),
            label: "Preview".into(),
            preview_available: true,
        }],
    );
    let projection = customer_portal_projection(
        &actor,
        &PortalPurchaseScope::disabled(),
        &mut repo,
        &mut files,
        NOW,
        "customer-read-1",
    )
    .await
    .unwrap();
    assert_eq!(projection.projection_version, "customer-order-v1");
    assert_eq!(projection.items.len(), 2);
    assert_eq!(projection.items[0].shared_files.len(), 1);
    assert!(projection.items[1].shared_files.is_empty());
    let value = serde_json::to_value(&projection).unwrap();
    assert_eq!(value["items"][0]["purchase"]["revision"], 2);
    assert_eq!(
        value["items"][0]["supportThreads"][0]["requestId"],
        "thread"
    );
    assert!(value["items"][1].get("purchase").is_none());

    let error = customer_portal_projection(
        &actor,
        &PortalPurchaseScope::disabled(),
        &mut repo,
        &mut files,
        NOW,
        "bad read id",
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_PROJECTION_INVALID");
}

#[tokio::test]
async fn command_records_once_with_effects_in_order() {
    let actor = actor();
    let mut repo = Repo {
        binding: Some(binding()),
        identity: true,
        ..Repo::default()
    };
    let mut effects = Effects::default();
    let mut input = input(PortalAction::RequestSupport);
    input.note = "Please help".into();
    input.attachment_set_ids = vec!["set-1".into()];

    let outcome = customer_portal_command(
        &actor,
        &input,
        "command-key-0001",
        &"a".repeat(64),
        "customer-portal-action-1",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap();

    assert_eq!(outcome.binding_id, "binding");
    assert_eq!(outcome.action, "request_support");
    assert_eq!(outcome.status, "recorded");
    assert_eq!(repo.recorded.len(), 1);
    assert_eq!(repo.recorded[0].input.note, "Please help");
    assert_eq!(
        effects.authorized,
        vec![PortalCommandPurpose::SettleExisting]
    );
    assert_eq!(effects.evidence, 1);
    assert_eq!(effects.notified.len(), 1);
    assert!(effects.notified[0].contains("requested support"));
    assert_eq!(effects.audited, vec!["customer-portal-action-1-audit"]);
    assert_eq!(
        effects.outboxed,
        vec![("customer-portal-action-1-outbox".into(), 1)]
    );
}

#[tokio::test]
async fn command_rejects_bad_key_hash_and_input() {
    let actor = actor();
    let mut repo = Repo {
        binding: Some(binding()),
        identity: true,
        ..Repo::default()
    };
    let mut effects = Effects::default();
    let good_hash = "a".repeat(64);
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "short",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "COMMAND_KEY_REQUIRED");

    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        "not-a-hash",
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_REQUEST_INVALID");

    let mut too_long = input(PortalAction::AcknowledgeDelivery);
    too_long.note = "x".repeat(501);
    let error = customer_portal_command(
        &actor,
        &too_long,
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_NOTE_INVALID");

    let mut too_many = input(PortalAction::RequestSupport);
    too_many.attachment_set_ids = (0..21).map(|index| format!("set-{index}")).collect();
    let error = customer_portal_command(
        &actor,
        &too_many,
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_ATTACHMENT_INVALID");
    assert!(repo.recorded.is_empty());
}

#[tokio::test]
async fn command_gates_binding_state_identity_and_duplicates() {
    let actor = actor();
    let good_hash = "a".repeat(64);
    let mut effects = Effects::default();

    let mut repo = Repo::default();
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_BINDING_UNAVAILABLE");

    let mut repo = Repo {
        binding: Some(PortalBinding {
            expires_at: NOW,
            ..binding()
        }),
        identity: true,
        ..Repo::default()
    };
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_BINDING_EXPIRED");

    let mut repo = Repo {
        binding: Some(PortalBinding {
            binding_version: 4,
            ..binding()
        }),
        identity: true,
        ..Repo::default()
    };
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_BINDING_CHANGED");

    let mut repo = Repo {
        binding: Some(PortalBinding {
            state: "cancelled".into(),
            ..binding()
        }),
        identity: true,
        ..Repo::default()
    };
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_SOURCE_UNAVAILABLE");

    let mut repo = Repo {
        binding: Some(binding()),
        identity: false,
        ..Repo::default()
    };
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_IDENTITY_REQUIRED");

    let mut repo = Repo {
        binding: Some(binding()),
        identity: true,
        existing: Some(PortalActionRow {
            id: "prior-action".into(),
            binding_id: "binding".into(),
            order_id: "order".into(),
            action: "acknowledge_delivery".into(),
            status: "recorded".into(),
            created_at: NOW - 10,
            request_hash: good_hash.clone(),
            user_id: "user".into(),
        }),
        ..Repo::default()
    };
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_ACTION_ALREADY_RECORDED");
}

#[tokio::test]
async fn command_rejects_attachment_on_acknowledgement_and_missing_holding() {
    let actor = actor();
    let good_hash = "a".repeat(64);
    let mut repo = Repo {
        binding: Some(binding()),
        identity: true,
        ..Repo::default()
    };
    let mut effects = Effects::default();
    let mut acknowledgement = input(PortalAction::AcknowledgeDelivery);
    acknowledgement.attachment_set_ids = vec!["set-1".into()];
    let error = customer_portal_command(
        &actor,
        &acknowledgement,
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_ATTACHMENT_ACTION");

    let without_holding = PortalCustomerActor::new("user", "user@example.com", None).unwrap();
    let mut support = input(PortalAction::RequestSupport);
    support.attachment_set_ids = vec!["set-1".into()];
    let error = customer_portal_command(
        &without_holding,
        &support,
        "command-key-0001",
        &good_hash,
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_PORTAL_ATTACHMENT_CONTEXT");
    assert!(repo.recorded.is_empty());
}

#[tokio::test]
async fn replay_returns_prior_result_and_rejects_a_different_request() {
    let actor = actor();
    let prior = PortalActionRow {
        id: "prior-action".into(),
        binding_id: "binding".into(),
        order_id: "order".into(),
        action: "acknowledge_delivery".into(),
        status: "recorded".into(),
        created_at: NOW - 10,
        request_hash: "b".repeat(64),
        user_id: "user".into(),
    };
    let mut repo = Repo {
        binding: Some(binding()),
        identity: true,
        prior: Some(prior.clone()),
        ..Repo::default()
    };
    let mut effects = Effects::default();
    let outcome = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &prior.request_hash,
        "new-id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap();
    assert_eq!(outcome.id, "prior-action");
    assert_eq!(outcome.created_at, prior.created_at);
    assert_eq!(effects.authorized, vec![PortalCommandPurpose::History]);
    assert!(repo.recorded.is_empty());
    assert_eq!(effects.evidence, 0);

    let mut repo = Repo {
        binding: Some(binding()),
        identity: true,
        prior: Some(prior),
        ..Repo::default()
    };
    let mut effects = Effects::default();
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &"c".repeat(64),
        "new-id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
    assert!(repo.recorded.is_empty());
}

#[tokio::test]
async fn purchase_authority_failure_blocks_the_write() {
    let actor = actor();
    let mut repo = Repo {
        binding: Some(binding()),
        identity: true,
        ..Repo::default()
    };
    let mut effects = Effects {
        fail_authorize: true,
        ..Effects::default()
    };
    let error = customer_portal_command(
        &actor,
        &input(PortalAction::AcknowledgeDelivery),
        "command-key-0001",
        &"a".repeat(64),
        "id",
        NOW,
        &mut repo,
        &mut effects,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "FEATURE_COMING_SOON");
    assert!(repo.recorded.is_empty());
}
