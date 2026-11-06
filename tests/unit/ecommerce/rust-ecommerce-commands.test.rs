//! Native Ecommerce catalog, channel-draft and review policy/sequencing.
use erpx_server_ecommerce::public::catalog::{
    CATALOG_LIMIT, EcommerceCatalogRepository, EcommerceChannelRow, EcommerceFuture,
    EcommerceListingRow, storefront_catalog,
};
use erpx_server_ecommerce::public::channel::{
    AuditEntry, ChannelDraft, ChannelReadback, CommandReceipt, CreateChannel,
    EcommerceCommandRepository, OutboxEntry, ReceiptEntry, create_channel,
};
use erpx_server_ecommerce::public::review::{
    ReviewAction, ReviewCommand, ReviewKind, ReviewReadback, ReviewSource, ValidatedReview,
    review_ecommerce_record,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "ecommerce-command-0001";
const CHANNEL_ID: &str = "ecommerce-channel-ecommerce-command-0001";

fn actor() -> erpx_server_ecommerce::public::catalog::EcommerceActor {
    erpx_server_ecommerce::public::catalog::EcommerceActor {
        user_id: UserId::new("user").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        role: Some("holding_owner".into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: vec![
            "commerce.read".into(),
            "commerce.sales".into(),
            "commerce.approve".into(),
        ],
        modules: vec!["ecommerce".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn channel() -> EcommerceChannelRow {
    EcommerceChannelRow {
        id: "channel-1".into(),
        company_id: "company".into(),
        name: "Storefront".into(),
        channel_type: "web".into(),
        configuration_json: "{\"storefront\":\"https://shop.example\"}".into(),
        status: "active".into(),
        created_by: "creator".into(),
        reviewed_by: Some("reviewer".into()),
        version: 2,
        created_at: 1,
        updated_at: 2,
    }
}

fn listing() -> EcommerceListingRow {
    EcommerceListingRow {
        id: "listing-1".into(),
        channel_id: "channel-1".into(),
        product_id: "product-1".into(),
        title: "Online product".into(),
        currency: "THB".into(),
        price_minor: 12_500,
        status: "published".into(),
        created_by: "creator".into(),
        reviewed_by: Some("reviewer".into()),
        version: 2,
        created_at: 1,
        updated_at: 2,
    }
}

#[derive(Default)]
struct CatalogProbe {
    channels: Vec<EcommerceChannelRow>,
    listings: Vec<EcommerceListingRow>,
    calls: Vec<&'static str>,
}

impl EcommerceCatalogRepository for CatalogProbe {
    type Error = BusinessError;

    fn channels<'a>(
        &'a mut self,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Vec<EcommerceChannelRow>, Self::Error> {
        self.calls.push("channels");
        let rows = self.channels.clone();
        Box::pin(async move { Ok(rows) })
    }

    fn listings<'a>(
        &'a mut self,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Vec<EcommerceListingRow>, Self::Error> {
        self.calls.push("listings");
        let rows = self.listings.clone();
        Box::pin(async move { Ok(rows) })
    }
}

#[derive(Default)]
struct CommandProbe {
    hash: String,
    receipt: Option<CommandReceipt>,
    insert: bool,
    channel_readback: Option<ChannelReadback>,
    review_source: Option<ReviewSource>,
    commit: bool,
    review_readback: Option<ReviewReadback>,
    receipt_insert: bool,
    calls: Vec<&'static str>,
    inserted: Option<ChannelDraft>,
    audited: Vec<(String, String, String, i64)>,
    outboxed: Vec<(String, String, String, i64, String)>,
}

impl CommandProbe {
    fn ready() -> Self {
        Self {
            hash: "a".repeat(64),
            insert: true,
            receipt_insert: true,
            commit: true,
            calls: Vec::new(),
            ..Self::default()
        }
    }
}

impl EcommerceCommandRepository for CommandProbe {
    type Error = BusinessError;

    fn digest<'a>(&'a mut self, value: &'a str) -> EcommerceFuture<'a, String, Self::Error> {
        self.calls.push("digest");
        assert!(value.contains("\"operation\""), "{value}");
        let hash = self.hash.clone();
        Box::pin(async move { Ok(hash) })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EcommerceFuture<'a, Option<CommandReceipt>, Self::Error> {
        self.calls.push("receipt");
        assert_eq!(key, KEY);
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn insert_channel<'a>(
        &'a mut self,
        _: &'a erpx_server_ecommerce::public::catalog::EcommerceActor,
        channel: &'a ChannelDraft,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("insert");
        self.inserted = Some(channel.clone());
        let insert = self.insert;
        Box::pin(async move { Ok(insert) })
    }

    fn channel_readback<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ChannelReadback>, Self::Error> {
        self.calls.push("readback");
        let readback = self.channel_readback.clone();
        Box::pin(async move { Ok(readback) })
    }

    fn review_source<'a>(
        &'a mut self,
        _: ReviewKind,
        _: &'a str,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ReviewSource>, Self::Error> {
        self.calls.push("source");
        let source = self.review_source.clone();
        Box::pin(async move { Ok(source) })
    }

    fn commit_review<'a>(
        &'a mut self,
        _: &'a ValidatedReview,
        _: &'a CompanyId,
        _: &'a erpx_server_ecommerce::public::catalog::EcommerceActor,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("commit");
        let commit = self.commit;
        Box::pin(async move { Ok(commit) })
    }

    fn review_readback<'a>(
        &'a mut self,
        _: ReviewKind,
        _: &'a str,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ReviewReadback>, Self::Error> {
        self.calls.push("review-readback");
        let readback = self.review_readback.clone();
        Box::pin(async move { Ok(readback) })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        _: &'a ReceiptEntry<'a>,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("receipt-insert");
        let insert = self.receipt_insert;
        Box::pin(async move { Ok(insert) })
    }

    fn audit<'a>(
        &'a mut self,
        _: &'a erpx_server_ecommerce::public::catalog::EcommerceActor,
        entry: &'a AuditEntry<'a>,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error> {
        self.calls.push("audit");
        self.audited.push((
            entry.operation.into(),
            entry.record_type.into(),
            entry.record_id.into(),
            entry.version,
        ));
        Box::pin(async move { Ok(()) })
    }

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error> {
        self.calls.push("outbox");
        self.outboxed.push((
            entry.operation.into(),
            entry.record_type.into(),
            entry.record_id.into(),
            entry.version,
            entry.state.into(),
        ));
        Box::pin(async move { Ok(()) })
    }
}

fn run_catalog(
    probe: &mut CatalogProbe,
    actor: &erpx_server_ecommerce::public::catalog::EcommerceActor,
) -> Result<erpx_server_ecommerce::public::catalog::StorefrontCatalog, BusinessError> {
    sqlx::test_block_on(storefront_catalog(actor, probe, NOW))
}

fn create_command() -> CreateChannel {
    CreateChannel {
        key: CommandKey::new(KEY).unwrap(),
        name: "  Storefront  ".into(),
        channel_type: "web".into(),
        configuration: json!({"storefront":"https://shop.example"}),
    }
}

fn channel_readback() -> ChannelReadback {
    ChannelReadback {
        id: CHANNEL_ID.into(),
        company_id: "company".into(),
        name: "Storefront".into(),
        channel_type: "web".into(),
        configuration_json: "{\"storefront\":\"https://shop.example\"}".into(),
        status: "draft".into(),
        created_by: "user".into(),
        reviewed_by: None,
        version: 1,
        created_at: NOW.get(),
        updated_at: NOW.get(),
    }
}

fn review(kind: ReviewKind, action: ReviewAction) -> ReviewCommand {
    ReviewCommand {
        key: CommandKey::new(KEY).unwrap(),
        kind,
        id: RecordId::new(if kind == ReviewKind::Channel {
            "channel-1"
        } else {
            "listing-1"
        })
        .unwrap(),
        action,
        expected_version: 3,
        reason: "  Independent review of current terms  ".into(),
    }
}

fn source(kind: ReviewKind, created_by: &str) -> ReviewSource {
    ReviewSource {
        id: if kind == ReviewKind::Channel {
            "channel-1"
        } else {
            "listing-1"
        }
        .into(),
        company_id: "company".into(),
        status: "draft".into(),
        version: 3,
        created_by: created_by.into(),
    }
}

fn review_readback(kind: ReviewKind, status: &str) -> ReviewReadback {
    ReviewReadback {
        id: if kind == ReviewKind::Channel {
            "channel-1"
        } else {
            "listing-1"
        }
        .into(),
        company_id: "company".into(),
        status: status.into(),
        version: 4,
        reviewed_by: Some("user".into()),
    }
}

fn run_create(
    probe: &mut CommandProbe,
    actor: &erpx_server_ecommerce::public::catalog::EcommerceActor,
) -> Result<erpx_server_ecommerce::public::channel::EcommerceReceipt, BusinessError> {
    let command = create_command();
    sqlx::test_block_on(create_channel(actor, &command, probe, NOW))
}

fn run_review(
    probe: &mut CommandProbe,
    actor: &erpx_server_ecommerce::public::catalog::EcommerceActor,
    command: &ReviewCommand,
) -> Result<erpx_server_ecommerce::public::channel::EcommerceReceipt, BusinessError> {
    sqlx::test_block_on(review_ecommerce_record(actor, command, probe, NOW))
}

#[test]
fn catalog_policy_precedes_every_source_read() {
    let mut no_permission = actor();
    no_permission.permissions.clear();
    let mut probe = CatalogProbe::default();
    let error = run_catalog(&mut probe, &no_permission).unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "ECOMMERCE_PERMISSION_REQUIRED");
    assert!(probe.calls.is_empty());

    let mut no_module = actor();
    no_module.modules.clear();
    let error = run_catalog(&mut CatalogProbe::default(), &no_module).unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "ECOMMERCE_MODULE_FORBIDDEN");

    let mut expired = actor();
    expired.expires_at = NOW.get();
    let error = run_catalog(&mut CatalogProbe::default(), &expired).unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
}

#[test]
fn catalog_rejects_foreign_unordered_and_orphan_rows_before_presentation() {
    let mut foreign = CatalogProbe {
        channels: vec![EcommerceChannelRow {
            company_id: "foreign".into(),
            ..channel()
        }],
        listings: vec![],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut foreign, &actor()).unwrap_err().code,
        "ECOMMERCE_SOURCE_INVALID"
    );

    let mut duplicate = CatalogProbe {
        channels: vec![channel(), channel()],
        listings: vec![],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut duplicate, &actor()).unwrap_err().code,
        "ECOMMERCE_SOURCE_INVALID"
    );

    let mut orphan = CatalogProbe {
        channels: vec![channel()],
        listings: vec![EcommerceListingRow {
            channel_id: "other-channel".into(),
            ..listing()
        }],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut orphan, &actor()).unwrap_err().code,
        "ECOMMERCE_SOURCE_INVALID"
    );

    let mut unordered = CatalogProbe {
        channels: vec![
            EcommerceChannelRow {
                id: "channel-2".into(),
                updated_at: 1,
                ..channel()
            },
            channel(),
        ],
        listings: vec![],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut unordered, &actor()).unwrap_err().code,
        "ECOMMERCE_SOURCE_INVALID"
    );

    let mut invalid_listing = CatalogProbe {
        channels: vec![channel()],
        listings: vec![EcommerceListingRow {
            currency: "thb".into(),
            ..listing()
        }],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut invalid_listing, &actor())
            .unwrap_err()
            .code,
        "ECOMMERCE_SOURCE_INVALID"
    );
}

#[test]
fn catalog_rejects_an_oversized_page_before_presentation() {
    let channels = (0..=CATALOG_LIMIT)
        .map(|index| EcommerceChannelRow {
            id: format!("channel-{index:04}"),
            updated_at: 1_000 - index as i64,
            created_at: 0,
            ..channel()
        })
        .collect::<Vec<_>>();
    let mut probe = CatalogProbe {
        channels,
        listings: vec![],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut probe, &actor()).unwrap_err().code,
        "ECOMMERCE_CATALOG_TOO_LARGE"
    );
}

#[test]
fn catalog_returns_only_the_exact_owner_columns() {
    let mut probe = CatalogProbe {
        channels: vec![channel()],
        listings: vec![listing()],
        calls: vec![],
    };
    let catalog = run_catalog(&mut probe, &actor()).unwrap();
    assert_eq!(probe.calls, ["channels", "listings"]);
    let value = serde_json::to_value(&catalog).unwrap();
    assert_eq!(value["channels"][0]["id"], "channel-1");
    assert_eq!(
        value["channels"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "channel_type",
            "company_id",
            "configuration_json",
            "created_at",
            "created_by",
            "id",
            "name",
            "reviewed_by",
            "status",
            "updated_at",
            "version",
        ]
    );
    assert_eq!(
        value["listings"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "channel_id",
            "created_at",
            "created_by",
            "currency",
            "id",
            "price_minor",
            "product_id",
            "reviewed_by",
            "status",
            "title",
            "updated_at",
            "version",
        ]
    );
}

#[test]
fn create_channel_writes_one_trimmed_draft_and_exact_receipt_audit_outbox() {
    let mut probe = CommandProbe {
        channel_readback: Some(channel_readback()),
        ..CommandProbe::ready()
    };
    let receipt = run_create(&mut probe, &actor()).unwrap();
    assert_eq!(receipt.id, CHANNEL_ID);
    assert_eq!(receipt.status, "draft");
    assert_eq!(receipt.version, 1);
    assert_eq!(
        probe.calls,
        [
            "digest",
            "receipt",
            "insert",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
    let inserted = probe.inserted.as_ref().unwrap();
    assert_eq!(inserted.name, "Storefront");
    assert_eq!(inserted.channel_type, "web");
    assert_eq!(
        inserted.configuration_json,
        "{\"storefront\":\"https://shop.example\"}"
    );
    assert_eq!(
        probe.audited,
        [(
            "ecommerce.channel.create".into(),
            "ecommerce_channel".into(),
            CHANNEL_ID.into(),
            1
        )]
    );
    assert_eq!(
        probe.outboxed,
        [(
            "ecommerce.channel.create".into(),
            "ecommerce_channel".into(),
            CHANNEL_ID.into(),
            1,
            "draft".into()
        )]
    );
}

#[test]
fn create_channel_replays_the_original_receipt_without_a_fresh_write() {
    let prior = json!({"id":CHANNEL_ID,"status":"draft","version":1}).to_string();
    let mut probe = CommandProbe {
        receipt: Some(CommandReceipt {
            request_hash: "a".repeat(64),
            result_json: prior,
            operation: Some("ecommerce.channel.create".into()),
            actor_id: Some("user".into()),
            record_id: Some(CHANNEL_ID.into()),
            authority_json: None,
        }),
        ..CommandProbe::ready()
    };
    let receipt = run_create(&mut probe, &actor()).unwrap();
    assert_eq!(receipt.id, CHANNEL_ID);
    assert_eq!(probe.calls, ["digest", "receipt"]);

    let mut conflict = CommandProbe {
        receipt: Some(CommandReceipt {
            request_hash: "b".repeat(64),
            result_json: json!({"id":CHANNEL_ID,"status":"draft","version":1}).to_string(),
            operation: Some("ecommerce.channel.create".into()),
            actor_id: Some("user".into()),
            record_id: Some(CHANNEL_ID.into()),
            authority_json: None,
        }),
        ..CommandProbe::ready()
    };
    assert_eq!(
        run_create(&mut conflict, &actor()).unwrap_err().code,
        "IDEMPOTENCY_CONFLICT"
    );
}

#[test]
fn create_channel_rejects_invalid_input_duplicate_identity_and_bad_readback() {
    for command in [
        CreateChannel {
            name: " ".into(),
            ..create_command()
        },
        CreateChannel {
            channel_type: "store".into(),
            ..create_command()
        },
        CreateChannel {
            configuration: json!(["not-an-object"]),
            ..create_command()
        },
    ] {
        let mut probe = CommandProbe::ready();
        assert_eq!(
            sqlx::test_block_on(create_channel(&actor(), &command, &mut probe, NOW))
                .unwrap_err()
                .kind,
            FailureKind::InvalidInput
        );
        assert!(probe.calls.is_empty());
    }

    let mut duplicate = CommandProbe {
        insert: false,
        ..CommandProbe::ready()
    };
    assert_eq!(
        run_create(&mut duplicate, &actor()).unwrap_err().code,
        "IDEMPOTENCY_CONFLICT"
    );

    let mut stale = CommandProbe {
        channel_readback: Some(ChannelReadback {
            version: 2,
            ..channel_readback()
        }),
        ..CommandProbe::ready()
    };
    assert_eq!(
        run_create(&mut stale, &actor()).unwrap_err().code,
        "ECOMMERCE_SOURCE_INVALID"
    );
}

#[test]
fn review_requires_company_assignment_and_independent_approval() {
    let mut branch = actor();
    branch.branch_id = Some(RecordId::new("branch").unwrap());
    let mut probe = CommandProbe::default();
    let error = run_review(
        &mut probe,
        &branch,
        &review(ReviewKind::Channel, ReviewAction::Activate),
    )
    .unwrap_err();
    assert_eq!(error.code, "COMPANY_ECOMMERCE_REQUIRED");
    assert!(probe.calls.is_empty());

    let mut no_approval = actor();
    no_approval
        .permissions
        .retain(|value| value != "commerce.approve");
    let error = run_review(
        &mut CommandProbe::default(),
        &no_approval,
        &review(ReviewKind::Channel, ReviewAction::Activate),
    )
    .unwrap_err();
    assert_eq!(error.code, "ECOMMERCE_APPROVAL_REQUIRED");

    let mut creator = CommandProbe {
        review_source: Some(source(ReviewKind::Channel, "user")),
        ..CommandProbe::ready()
    };
    let error = run_review(
        &mut creator,
        &actor(),
        &review(ReviewKind::Channel, ReviewAction::Activate),
    )
    .unwrap_err();
    assert_eq!(error.code, "ECOMMERCE_INDEPENDENT_REVIEW_REQUIRED");

    let mut retired = CommandProbe {
        review_source: Some(ReviewSource {
            status: "active".into(),
            version: 4,
            ..source(ReviewKind::Channel, "user")
        }),
        review_readback: Some(ReviewReadback {
            version: 5,
            ..review_readback(ReviewKind::Channel, "retired")
        }),
        ..CommandProbe::ready()
    };
    let receipt = run_review(
        &mut retired,
        &actor(),
        &ReviewCommand {
            expected_version: 4,
            ..review(ReviewKind::Channel, ReviewAction::Retire)
        },
    )
    .unwrap();
    assert_eq!(receipt.status, "retired");
    assert_eq!(receipt.version, 5);
}

#[test]
fn review_applies_exact_kind_action_transition_and_writes_receipt_audit_outbox() {
    let mut activate = CommandProbe {
        review_source: Some(source(ReviewKind::Channel, "creator")),
        review_readback: Some(review_readback(ReviewKind::Channel, "active")),
        ..CommandProbe::ready()
    };
    let receipt = run_review(
        &mut activate,
        &actor(),
        &review(ReviewKind::Channel, ReviewAction::Activate),
    )
    .unwrap();
    assert_eq!(receipt.status, "active");
    assert_eq!(receipt.version, 4);
    assert_eq!(
        activate.audited,
        [(
            "ecommerce.channel.activate".into(),
            "ecommerce_channel".into(),
            "channel-1".into(),
            4
        )]
    );
    assert_eq!(
        activate.outboxed,
        [(
            "ecommerce.channel.activate".into(),
            "ecommerce_channel".into(),
            "channel-1".into(),
            4,
            "active".into()
        )]
    );

    let mut publish = CommandProbe {
        review_source: Some(source(ReviewKind::Listing, "creator")),
        review_readback: Some(review_readback(ReviewKind::Listing, "published")),
        ..CommandProbe::ready()
    };
    let receipt = run_review(
        &mut publish,
        &actor(),
        &review(ReviewKind::Listing, ReviewAction::Publish),
    )
    .unwrap();
    assert_eq!(receipt.status, "published");
    assert_eq!(publish.audited[0].0, "ecommerce.listing.publish");

    for command in [
        review(ReviewKind::Listing, ReviewAction::Activate),
        review(ReviewKind::Channel, ReviewAction::Publish),
    ] {
        assert_eq!(
            run_review(&mut CommandProbe::ready(), &actor(), &command)
                .unwrap_err()
                .kind,
            FailureKind::InvalidInput
        );
    }
}

#[test]
fn review_rejects_stale_source_cas_loss_and_bad_readback() {
    let mut stale = CommandProbe {
        review_source: Some(ReviewSource {
            version: 2,
            ..source(ReviewKind::Channel, "creator")
        }),
        ..CommandProbe::ready()
    };
    assert_eq!(
        run_review(
            &mut stale,
            &actor(),
            &review(ReviewKind::Channel, ReviewAction::Activate)
        )
        .unwrap_err()
        .code,
        "ECOMMERCE_SOURCE_CHANGED"
    );

    let mut cas = CommandProbe {
        review_source: Some(source(ReviewKind::Channel, "creator")),
        commit: false,
        ..CommandProbe::ready()
    };
    assert_eq!(
        run_review(
            &mut cas,
            &actor(),
            &review(ReviewKind::Channel, ReviewAction::Activate)
        )
        .unwrap_err()
        .code,
        "ECOMMERCE_SOURCE_CHANGED"
    );

    let mut missing = CommandProbe::ready();
    assert_eq!(
        run_review(
            &mut missing,
            &actor(),
            &review(ReviewKind::Channel, ReviewAction::Activate)
        )
        .unwrap_err()
        .code,
        "ECOMMERCE_RECORD_NOT_FOUND"
    );

    let mut bad = CommandProbe {
        review_source: Some(source(ReviewKind::Channel, "creator")),
        review_readback: Some(ReviewReadback {
            status: "draft".into(),
            ..review_readback(ReviewKind::Channel, "active")
        }),
        ..CommandProbe::ready()
    };
    assert_eq!(
        run_review(
            &mut bad,
            &actor(),
            &review(ReviewKind::Channel, ReviewAction::Activate)
        )
        .unwrap_err()
        .code,
        "ECOMMERCE_SOURCE_INVALID"
    );
}

#[test]
fn channel_name_uses_js_trim_and_code_point_length_parity() {
    let validate = |name: String| {
        CreateChannel {
            name,
            ..create_command()
        }
        .validate()
        .map(|validated| validated.name)
        .map_err(|error| error.code)
    };
    // 160 Thai scalars: 160 code points (allowed), 480 UTF-8 bytes.
    assert_eq!(validate("ก".repeat(160)), Ok("ก".repeat(160)));
    assert_eq!(validate("ก".repeat(161)), Err("ECOMMERCE_REQUEST_INVALID"));
    // One astral scalar is one code point, so it fails the JS minimum of 2.
    assert_eq!(
        validate("😀".to_owned()),
        Err("ECOMMERCE_REQUEST_INVALID")
    );
    assert_eq!(validate("😀😀".to_owned()), Ok("😀😀".to_owned()));
    // 160 astral scalars: 160 code points; 161 exceed the maximum.
    assert!(validate("😀".repeat(160)).is_ok());
    assert!(validate("😀".repeat(161)).is_err());
    // JS trim strips ZWNBSP (U+FEFF); Rust trim does not.
    assert_eq!(
        validate("\u{FEFF}ab\u{FEFF}".to_owned()),
        Ok("ab".to_owned())
    );
    assert!(validate("\u{FEFF}a\u{FEFF}".to_owned()).is_err());
    assert_eq!(
        validate("  Storefront  ".to_owned()),
        Ok("Storefront".to_owned())
    );
}

#[test]
fn review_reason_uses_js_trim_and_code_point_length_parity() {
    let validate = |reason: String| {
        ReviewCommand {
            reason,
            ..review(ReviewKind::Channel, ReviewAction::Activate)
        }
        .validate()
        .map(|validated| validated.reason)
        .map_err(|error| error.code)
    };
    // 2000 Thai scalars: 2000 code points (allowed), 6000 UTF-8 bytes.
    assert_eq!(validate("ก".repeat(2000)), Ok("ก".repeat(2000)));
    assert_eq!(validate("ก".repeat(2001)), Err("ECOMMERCE_REQUEST_INVALID"));
    // Astral text counts code points: 2 scalars fail the 3-point minimum,
    // while 1 scalar must also fail; 3 scalars pass.
    assert!(validate("😀😀".to_owned()).is_err());
    assert!(validate("😀".to_owned()).is_err());
    assert_eq!(validate("😀😀😀".to_owned()), Ok("😀😀😀".to_owned()));
    assert!(validate("😀".repeat(2000)).is_ok());
    assert!(validate("😀".repeat(2001)).is_err());
    // JS trim strips ZWNBSP padding before the length check.
    assert_eq!(
        validate("\u{FEFF}abc\u{FEFF}".to_owned()),
        Ok("abc".to_owned())
    );
    assert!(validate("\u{FEFF}ab\u{FEFF}".to_owned()).is_err());
    assert_eq!(
        validate("  Independent review  ".to_owned()),
        Ok("Independent review".to_owned())
    );
}

#[test]
fn catalog_text_uses_utf16_length_and_js_trim_for_stored_rows() {
    let mut thai_channel = CatalogProbe {
        channels: vec![EcommerceChannelRow {
            name: "ก".repeat(160),
            ..channel()
        }],
        listings: vec![listing()],
        calls: vec![],
    };
    assert!(run_catalog(&mut thai_channel, &actor()).is_ok());

    let mut oversized_channel = CatalogProbe {
        channels: vec![EcommerceChannelRow {
            name: "ก".repeat(161),
            ..channel()
        }],
        listings: vec![listing()],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut oversized_channel, &actor())
            .unwrap_err()
            .code,
        "ECOMMERCE_SOURCE_INVALID"
    );

    let mut astral_channel = CatalogProbe {
        channels: vec![EcommerceChannelRow {
            name: "😀".repeat(80),
            ..channel()
        }],
        listings: vec![listing()],
        calls: vec![],
    };
    assert!(run_catalog(&mut astral_channel, &actor()).is_ok());

    let mut thai_listing = CatalogProbe {
        channels: vec![channel()],
        listings: vec![EcommerceListingRow {
            title: "ก".repeat(200),
            ..listing()
        }],
        calls: vec![],
    };
    assert!(run_catalog(&mut thai_listing, &actor()).is_ok());

    let mut oversized_listing = CatalogProbe {
        channels: vec![channel()],
        listings: vec![EcommerceListingRow {
            title: "ก".repeat(201),
            ..listing()
        }],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut oversized_listing, &actor())
            .unwrap_err()
            .code,
        "ECOMMERCE_SOURCE_INVALID"
    );

    // A stored name/title of only ZWNBSP is empty after JS trim.
    let mut feff_channel = CatalogProbe {
        channels: vec![EcommerceChannelRow {
            name: "\u{FEFF}".to_owned(),
            ..channel()
        }],
        listings: vec![listing()],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut feff_channel, &actor()).unwrap_err().code,
        "ECOMMERCE_SOURCE_INVALID"
    );
    let mut feff_listing = CatalogProbe {
        channels: vec![channel()],
        listings: vec![EcommerceListingRow {
            title: "\u{FEFF}".to_owned(),
            ..listing()
        }],
        calls: vec![],
    };
    assert_eq!(
        run_catalog(&mut feff_listing, &actor()).unwrap_err().code,
        "ECOMMERCE_SOURCE_INVALID"
    );
}
