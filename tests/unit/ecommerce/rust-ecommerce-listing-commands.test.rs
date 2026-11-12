//! Native Ecommerce listing draft/create/change policy, sequencing and HTTP
//! ingress. Owner dispatch is exercised without opening a socket; mounting the
//! composed Commerce/Finance producer ports and the router remains a lead
//! integration step.
use erpx_server_ecommerce::public::catalog::EcommerceActor;
use erpx_server_ecommerce::public::ecommerce_http::{
    EcommerceHttpFuture, EcommerceHttpResponse, EcommerceListingChangeHttpRequest,
    EcommerceListingCreateHttpRequest, EcommerceListingHttpBackend, EcommerceListingHttpState,
    EcommerceListingHttpTarget,
};
use erpx_server_ecommerce::public::listing::{
    AuditEntry, ChangeListing, CommandReceipt, CreateListing, EcommerceFunctionalCurrency,
    EcommerceFuture, EcommerceListingMoneyRequest, EcommerceListingMoneySnapshot,
    EcommerceListingRepository, ListingDraft, ListingRecord, OutboxEntry, ReceiptEntry,
    change_listing, create_listing,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, StatusCode, body::to_bytes,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "ecommerce-listing-command-0001";
const LISTING_ID: &str = "ecommerce-listing-ecommerce-listing-command-0001";
const CHANNEL_ID: &str = "channel-1";
const PRODUCT_ID: &str = "product-1";

fn actor() -> EcommerceActor {
    EcommerceActor {
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

fn create(key: &str) -> CreateListing {
    CreateListing {
        key: CommandKey::new(key).unwrap(),
        channel_id: RecordId::new(CHANNEL_ID).unwrap(),
        product_id: RecordId::new(PRODUCT_ID).unwrap(),
        title: "  Online product  ".into(),
        currency: "thb".into(),
        price: "12.5".into(),
    }
}

fn draft() -> ListingDraft {
    ListingDraft {
        id: LISTING_ID.into(),
        channel_id: CHANNEL_ID.into(),
        product_id: PRODUCT_ID.into(),
        title: "Online product".into(),
        currency: "THB".into(),
        price_minor: 1_250,
    }
}

fn record() -> ListingRecord {
    ListingRecord {
        id: LISTING_ID.into(),
        company_id: "company".into(),
        channel_id: CHANNEL_ID.into(),
        product_id: PRODUCT_ID.into(),
        title: "Online product".into(),
        currency: "THB".into(),
        price_minor: 1_250,
        status: "draft".into(),
        created_by: "user".into(),
        reviewed_by: None,
        version: 1,
        created_at: NOW.get(),
        updated_at: NOW.get(),
    }
}

fn change(key: &str) -> ChangeListing {
    ChangeListing {
        key: CommandKey::new(key).unwrap(),
        id: RecordId::new("listing-1").unwrap(),
        expected_version: 1,
        title: "  Revised online product  ".into(),
    }
}

fn changed_source() -> ListingRecord {
    ListingRecord {
        id: "listing-1".into(),
        title: "Online product".into(),
        ..record()
    }
}

fn changed_readback() -> ListingRecord {
    ListingRecord {
        id: "listing-1".into(),
        title: "Revised online product".into(),
        version: 2,
        updated_at: NOW.get() + 1,
        ..changed_source()
    }
}

#[derive(Default)]
struct ListingProbe {
    hash: String,
    receipt: Option<CommandReceipt>,
    channel: bool,
    product: bool,
    functional: Option<EcommerceFunctionalCurrency>,
    money_override: Option<Result<EcommerceListingMoneySnapshot, BusinessError>>,
    insert: bool,
    source: Option<ListingRecord>,
    commit: bool,
    readback: Option<ListingRecord>,
    receipt_insert: bool,
    calls: Vec<String>,
    inserted: Option<ListingDraft>,
    captured: Option<EcommerceListingMoneyRequest>,
    audited: Vec<(String, String, String, i64)>,
    outboxed: Vec<(String, String, String, i64, String)>,
}

impl ListingProbe {
    fn ready() -> Self {
        Self {
            hash: "a".repeat(64),
            channel: true,
            product: true,
            functional: Some(EcommerceFunctionalCurrency {
                currency: "THB".into(),
                base_currency: "THB".into(),
            }),
            insert: true,
            commit: true,
            receipt_insert: true,
            ..Self::default()
        }
    }
}

impl EcommerceListingRepository for ListingProbe {
    type Error = BusinessError;

    fn digest<'a>(&'a mut self, value: &'a str) -> EcommerceFuture<'a, String, Self::Error> {
        self.calls.push("digest".into());
        assert!(value.contains("\"operation\""), "{value}");
        assert!(value.contains("\"company\""), "{value}");
        let hash = self.hash.clone();
        Box::pin(async move { Ok(hash) })
    }

    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> EcommerceFuture<'a, Option<CommandReceipt>, Self::Error> {
        self.calls.push("receipt".into());
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn active_channel<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("channel".into());
        let channel = self.channel;
        Box::pin(async move { Ok(channel) })
    }

    fn active_product<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("product".into());
        let product = self.product;
        Box::pin(async move { Ok(product) })
    }

    fn functional_currency<'a>(
        &'a mut self,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<EcommerceFunctionalCurrency>, Self::Error> {
        self.calls.push("currency".into());
        let functional = self.functional.clone();
        Box::pin(async move { Ok(functional) })
    }

    fn capture_money<'a>(
        &'a mut self,
        _: &'a EcommerceActor,
        request: &'a EcommerceListingMoneyRequest,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, EcommerceListingMoneySnapshot, Self::Error> {
        self.calls.push("money".into());
        self.captured = Some(request.clone());
        if let Some(money) = self.money_override.clone() {
            return Box::pin(async move { money });
        }
        let amount_minor = request.amount_minor;
        Box::pin(async move {
            Ok(EcommerceListingMoneySnapshot {
                currency_scale: 2,
                amount_units: amount_minor,
                usd_units: amount_minor,
                rate_id: None,
                used_latest_fallback: false,
                captured_at: NOW.get(),
            })
        })
    }

    fn insert_listing<'a>(
        &'a mut self,
        _: &'a EcommerceActor,
        listing: &'a ListingDraft,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("insert".into());
        self.inserted = Some(listing.clone());
        let insert = self.insert;
        Box::pin(async move { Ok(insert) })
    }

    fn listing_source<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ListingRecord>, Self::Error> {
        self.calls.push("source".into());
        let source = self.source.clone();
        Box::pin(async move { Ok(source) })
    }

    fn commit_listing_change<'a>(
        &'a mut self,
        _: &'a erpx_server_ecommerce::public::listing::ValidatedListingChange,
        _: &'a CompanyId,
        _: &'a EcommerceActor,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("commit".into());
        let commit = self.commit;
        Box::pin(async move { Ok(commit) })
    }

    fn listing_readback<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ListingRecord>, Self::Error> {
        self.calls.push("readback".into());
        let readback = self.readback.clone();
        Box::pin(async move { Ok(readback) })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        _: &'a ReceiptEntry<'a>,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        self.calls.push("receipt-insert".into());
        let insert = self.receipt_insert;
        Box::pin(async move { Ok(insert) })
    }

    fn audit<'a>(
        &'a mut self,
        _: &'a EcommerceActor,
        entry: &'a AuditEntry<'a>,
        _: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error> {
        self.calls.push("audit".into());
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
        self.calls.push("outbox".into());
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

fn run_create(
    probe: &mut ListingProbe,
    actor: &EcommerceActor,
    command: &CreateListing,
) -> Result<erpx_server_ecommerce::public::listing::EcommerceReceipt, BusinessError> {
    sqlx::test_block_on(create_listing(actor, command, probe, NOW))
}

fn run_change(
    probe: &mut ListingProbe,
    actor: &EcommerceActor,
    command: &ChangeListing,
) -> Result<erpx_server_ecommerce::public::listing::EcommerceReceipt, BusinessError> {
    sqlx::test_block_on(change_listing(actor, command, probe, NOW))
}

#[test]
fn listing_terms_normalize_trim_currency_and_exact_minor_price() {
    let validated = create(KEY).validate().unwrap();
    assert_eq!(validated.title, "Online product");
    assert_eq!(validated.currency, "THB");
    assert_eq!(validated.price_minor, 1_250);
    assert_eq!(validated.channel_id, CHANNEL_ID);
    assert_eq!(validated.product_id, PRODUCT_ID);
    for (price, minor) in [
        ("0", 0),
        ("1", 100),
        ("12.5", 1_250),
        ("12.05", 1_205),
        ("0007.90", 790),
        ("90071992547409.91", 9_007_199_254_740_991),
    ] {
        let validated = CreateListing {
            price: price.into(),
            ..create(KEY)
        }
        .validate()
        .unwrap();
        assert_eq!(validated.price_minor, minor, "{price}");
    }
    let identity = draft().identity(&actor());
    assert!(identity.contains("\"operation\":\"ecommerce.listing.create\""));
    assert!(identity.contains("\"priceMinor\":1250"));
    assert!(identity.contains("\"channelId\":\"channel-1\""));
}

#[test]
fn listing_terms_reject_invalid_title_currency_and_price_forms() {
    for command in [
        CreateListing {
            title: " ".into(),
            ..create(KEY)
        },
        CreateListing {
            title: "a".into(),
            ..create(KEY)
        },
        CreateListing {
            title: "line\nbreak".into(),
            ..create(KEY)
        },
        CreateListing {
            // 200 emoji are 400 UTF-16 units in the catalog read.
            title: "😀".repeat(200),
            ..create(KEY)
        },
        CreateListing {
            currency: "TH".into(),
            ..create(KEY)
        },
        CreateListing {
            currency: "THBB".into(),
            ..create(KEY)
        },
        CreateListing {
            currency: "ไทย".into(),
            ..create(KEY)
        },
        CreateListing {
            price: String::new(),
            ..create(KEY)
        },
        CreateListing {
            price: " 12.50".into(),
            ..create(KEY)
        },
        CreateListing {
            price: "12.345".into(),
            ..create(KEY)
        },
        CreateListing {
            price: "12.".into(),
            ..create(KEY)
        },
        CreateListing {
            price: ".5".into(),
            ..create(KEY)
        },
        CreateListing {
            price: "-1".into(),
            ..create(KEY)
        },
        CreateListing {
            price: "1e3".into(),
            ..create(KEY)
        },
        CreateListing {
            price: "90071992547410.00".into(),
            ..create(KEY)
        },
        CreateListing {
            price: "123456789012345678901.00".into(),
            ..create(KEY)
        },
    ] {
        assert_eq!(
            command.validate().unwrap_err().code,
            "ECOMMERCE_REQUEST_INVALID"
        );
    }
}

#[test]
fn listings_require_sales_permission_and_live_context() {
    let mut probe = ListingProbe::ready();
    let forbidden = EcommerceActor {
        permissions: vec!["commerce.read".into(), "commerce.approve".into()],
        ..actor()
    };
    assert_eq!(
        run_create(&mut probe, &forbidden, &create(KEY))
            .unwrap_err()
            .code,
        "ECOMMERCE_PERMISSION_REQUIRED"
    );
    assert_eq!(
        run_change(&mut probe, &forbidden, &change(KEY))
            .unwrap_err()
            .code,
        "ECOMMERCE_PERMISSION_REQUIRED"
    );
    let expired = EcommerceActor {
        expires_at: NOW.get(),
        ..actor()
    };
    assert_eq!(
        run_create(&mut probe, &expired, &create(KEY))
            .unwrap_err()
            .code,
        "WORK_CONTEXT_CHANGED"
    );
    let wrong_module = EcommerceActor {
        modules: vec!["commerce".into()],
        ..actor()
    };
    assert_eq!(
        run_create(&mut probe, &wrong_module, &create(KEY))
            .unwrap_err()
            .code,
        "ECOMMERCE_MODULE_FORBIDDEN"
    );
    assert!(probe.calls.is_empty());
}

#[test]
fn create_listing_checks_sources_and_records_one_draft_receipt() {
    let mut probe = ListingProbe {
        readback: Some(record()),
        ..ListingProbe::ready()
    };
    let receipt = run_create(&mut probe, &actor(), &create(KEY)).unwrap();
    assert_eq!(receipt.id, LISTING_ID);
    assert_eq!(receipt.status, "draft");
    assert_eq!(receipt.version, 1);
    assert_eq!(
        probe.calls,
        [
            "digest",
            "receipt",
            "channel",
            "product",
            "currency",
            "money",
            "insert",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
    let inserted = probe.inserted.as_ref().unwrap();
    assert_eq!(inserted.title, "Online product");
    assert_eq!(inserted.currency, "THB");
    assert_eq!(inserted.price_minor, 1_250);
    let captured = probe.captured.as_ref().unwrap();
    assert_eq!(captured.company_id, "company");
    assert_eq!(captured.listing_id, LISTING_ID);
    assert_eq!(captured.currency, "THB");
    assert_eq!(captured.amount_minor, 1_250);
    assert_eq!(
        probe.audited,
        [(
            "ecommerce.listing.create".into(),
            "ecommerce_listing".into(),
            LISTING_ID.into(),
            1
        )]
    );
    assert_eq!(
        probe.outboxed,
        [(
            "ecommerce.listing.create".into(),
            "ecommerce_listing".into(),
            LISTING_ID.into(),
            1,
            "draft".into()
        )]
    );
}

#[test]
fn create_listing_replays_idempotently_and_rejects_another_input() {
    let prior = json!({"id":LISTING_ID,"status":"draft","version":1}).to_string();
    let mut probe = ListingProbe {
        receipt: Some(CommandReceipt {
            request_hash: "a".repeat(64),
            result_json: prior.clone(),
            operation: Some("ecommerce.listing.create".into()),
            actor_id: Some("user".into()),
            record_id: Some(LISTING_ID.into()),
            authority_json: None,
        }),
        ..ListingProbe::ready()
    };
    let receipt = run_create(&mut probe, &actor(), &create(KEY)).unwrap();
    assert_eq!(receipt.id, LISTING_ID);
    assert_eq!(receipt.version, 1);
    assert_eq!(probe.calls, ["digest", "receipt"]);

    let mut conflict = ListingProbe {
        receipt: Some(CommandReceipt {
            request_hash: "b".repeat(64),
            result_json: prior,
            operation: Some("ecommerce.listing.create".into()),
            actor_id: Some("user".into()),
            record_id: Some(LISTING_ID.into()),
            authority_json: None,
        }),
        ..ListingProbe::ready()
    };
    assert_eq!(
        run_create(&mut conflict, &actor(), &create(KEY))
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(conflict.calls, ["digest", "receipt"]);
}

#[test]
fn create_listing_fails_closed_on_missing_or_stale_sources() {
    let mut channel = ListingProbe {
        channel: false,
        ..ListingProbe::ready()
    };
    assert_eq!(
        run_create(&mut channel, &actor(), &create(KEY))
            .unwrap_err()
            .code,
        "ECOMMERCE_CHANNEL_NOT_ACTIVE"
    );
    assert_eq!(channel.calls, ["digest", "receipt", "channel"]);

    let mut product = ListingProbe {
        product: false,
        ..ListingProbe::ready()
    };
    assert_eq!(
        run_create(&mut product, &actor(), &create(KEY))
            .unwrap_err()
            .code,
        "ECOMMERCE_PRODUCT_NOT_ACTIVE"
    );
    assert_eq!(product.calls.last().map(String::as_str), Some("product"));

    for functional in [
        None,
        Some(EcommerceFunctionalCurrency {
            currency: "USD".into(),
            base_currency: "USD".into(),
        }),
        Some(EcommerceFunctionalCurrency {
            currency: "THB".into(),
            base_currency: "USD".into(),
        }),
    ] {
        let mut probe = ListingProbe {
            functional,
            ..ListingProbe::ready()
        };
        assert_eq!(
            run_create(&mut probe, &actor(), &create(KEY))
                .unwrap_err()
                .code,
            "ECOMMERCE_CURRENCY_MAPPING_REQUIRED"
        );
        assert!(!probe.calls.contains(&"money".to_string()));
    }

    let mut unavailable = ListingProbe {
        money_override: Some(Err(
            erpx_server_ecommerce::public::listing::money_source_unavailable(),
        )),
        ..ListingProbe::ready()
    };
    assert_eq!(
        run_create(&mut unavailable, &actor(), &create(KEY))
            .unwrap_err()
            .code,
        "ECOMMERCE_MONEY_SOURCE_UNAVAILABLE"
    );

    for snapshot in [
        EcommerceListingMoneySnapshot {
            currency_scale: 3,
            amount_units: 1_250,
            usd_units: 1_250,
            rate_id: None,
            used_latest_fallback: false,
            captured_at: NOW.get(),
        },
        EcommerceListingMoneySnapshot {
            currency_scale: 2,
            amount_units: 1_249,
            usd_units: 1_250,
            rate_id: None,
            used_latest_fallback: false,
            captured_at: NOW.get(),
        },
        EcommerceListingMoneySnapshot {
            currency_scale: 2,
            amount_units: 1_250,
            usd_units: 1_250,
            rate_id: Some(String::new()),
            used_latest_fallback: false,
            captured_at: NOW.get(),
        },
        EcommerceListingMoneySnapshot {
            currency_scale: 2,
            amount_units: 1_250,
            usd_units: 1_250,
            rate_id: None,
            used_latest_fallback: false,
            captured_at: NOW.get() + 1,
        },
    ] {
        let mut probe = ListingProbe {
            money_override: Some(Ok(snapshot)),
            ..ListingProbe::ready()
        };
        assert_eq!(
            run_create(&mut probe, &actor(), &create(KEY))
                .unwrap_err()
                .code,
            "ECOMMERCE_MONEY_EVIDENCE_INVALID"
        );
        assert!(!probe.calls.contains(&"insert".to_string()));
    }
}

#[test]
fn create_listing_rejects_a_stale_identity_and_unreadable_readback() {
    let mut duplicate = ListingProbe {
        insert: false,
        ..ListingProbe::ready()
    };
    assert_eq!(
        run_create(&mut duplicate, &actor(), &create(KEY))
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );

    for readback in [
        None,
        Some(ListingRecord {
            title: "Other title".into(),
            ..record()
        }),
        Some(ListingRecord {
            price_minor: 99,
            ..record()
        }),
        Some(ListingRecord {
            company_id: "foreign".into(),
            ..record()
        }),
        Some(ListingRecord {
            version: 2,
            ..record()
        }),
    ] {
        let mut probe = ListingProbe {
            readback,
            ..ListingProbe::ready()
        };
        assert_eq!(
            run_create(&mut probe, &actor(), &create(KEY))
                .unwrap_err()
                .code,
            "ECOMMERCE_SOURCE_INVALID"
        );
    }
}

#[test]
fn change_listing_requires_the_draft_author_and_exact_version() {
    for (source, code) in [
        (None, "ECOMMERCE_RECORD_NOT_FOUND"),
        (
            Some(ListingRecord {
                version: 2,
                ..changed_source()
            }),
            "ECOMMERCE_SOURCE_CHANGED",
        ),
        (
            Some(ListingRecord {
                status: "published".into(),
                ..changed_source()
            }),
            "ECOMMERCE_SOURCE_CHANGED",
        ),
        (
            Some(ListingRecord {
                created_by: "reviewer".into(),
                ..changed_source()
            }),
            "ECOMMERCE_LISTING_AUTHOR_REQUIRED",
        ),
        (
            Some(ListingRecord {
                company_id: "foreign".into(),
                ..changed_source()
            }),
            "ECOMMERCE_RECORD_NOT_FOUND",
        ),
    ] {
        let mut probe = ListingProbe {
            source,
            ..ListingProbe::ready()
        };
        assert_eq!(
            run_change(&mut probe, &actor(), &change(KEY)).unwrap_err().code,
            code
        );
        assert!(!probe.calls.contains(&"commit".to_string()));
    }
}

#[test]
fn change_listing_commits_one_cas_and_keeps_source_terms() {
    let mut probe = ListingProbe {
        source: Some(changed_source()),
        readback: Some(changed_readback()),
        ..ListingProbe::ready()
    };
    let receipt = run_change(&mut probe, &actor(), &change(KEY)).unwrap();
    assert_eq!(receipt.id, "listing-1");
    assert_eq!(receipt.status, "draft");
    assert_eq!(receipt.version, 2);
    assert_eq!(
        probe.calls,
        [
            "digest",
            "receipt",
            "source",
            "commit",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
    assert_eq!(
        probe.audited,
        [(
            "ecommerce.listing.change".into(),
            "ecommerce_listing".into(),
            "listing-1".into(),
            2
        )]
    );
    assert_eq!(probe.outboxed[0].4, "draft");

    for readback in [
        ListingRecord {
            price_minor: 1_300,
            ..changed_readback()
        },
        ListingRecord {
            currency: "USD".into(),
            ..changed_readback()
        },
        ListingRecord {
            channel_id: "channel-2".into(),
            ..changed_readback()
        },
        ListingRecord {
            product_id: "product-2".into(),
            ..changed_readback()
        },
        ListingRecord {
            created_by: "other".into(),
            ..changed_readback()
        },
        ListingRecord {
            created_at: 5,
            ..changed_readback()
        },
        ListingRecord {
            reviewed_by: Some("reviewer".into()),
            ..changed_readback()
        },
        ListingRecord {
            status: "published".into(),
            ..changed_readback()
        },
        ListingRecord {
            title: "Unrecorded title".into(),
            ..changed_readback()
        },
        ListingRecord {
            version: 3,
            ..changed_readback()
        },
        ListingRecord {
            updated_at: 9,
            ..changed_readback()
        },
        ListingRecord {
            company_id: "foreign".into(),
            ..changed_readback()
        },
    ] {
        let mut probe = ListingProbe {
            source: Some(changed_source()),
            readback: Some(readback),
            ..ListingProbe::ready()
        };
        assert_eq!(
            run_change(&mut probe, &actor(), &change(KEY))
                .unwrap_err()
                .code,
            "ECOMMERCE_SOURCE_INVALID"
        );
    }
}

#[test]
fn change_listing_replays_and_rejects_a_stale_commit_or_invalid_input() {
    let prior = json!({"id":"listing-1","status":"draft","version":2}).to_string();
    let mut probe = ListingProbe {
        receipt: Some(CommandReceipt {
            request_hash: "a".repeat(64),
            result_json: prior,
            operation: Some("ecommerce.listing.change".into()),
            actor_id: Some("user".into()),
            record_id: Some("listing-1".into()),
            authority_json: None,
        }),
        ..ListingProbe::ready()
    };
    let receipt = run_change(&mut probe, &actor(), &change(KEY)).unwrap();
    assert_eq!(receipt.version, 2);
    assert_eq!(probe.calls, ["digest", "receipt"]);

    let mut stale = ListingProbe {
        source: Some(changed_source()),
        readback: Some(changed_readback()),
        commit: false,
        ..ListingProbe::ready()
    };
    assert_eq!(
        run_change(&mut stale, &actor(), &change(KEY))
            .unwrap_err()
            .code,
        "ECOMMERCE_SOURCE_CHANGED"
    );
    assert!(!stale.calls.contains(&"audit".to_string()));

    for command in [
        ChangeListing {
            expected_version: 0,
            ..change(KEY)
        },
        ChangeListing {
            title: " ".into(),
            ..change(KEY)
        },
        ChangeListing {
            title: "line\nbreak".into(),
            ..change(KEY)
        },
    ] {
        assert_eq!(
            command.validate().unwrap_err().code,
            "ECOMMERCE_REQUEST_INVALID"
        );
    }
}

fn http_target(
    backend: Arc<dyn EcommerceListingHttpBackend>,
) -> EcommerceListingHttpTarget {
    EcommerceListingHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

fn headers(overrides: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("host", "acme.example.test"),
        ("content-type", "application/json"),
        ("idempotency-key", KEY),
        ("origin", "https://acme.example.test"),
        ("cookie", "__Host-erpx-product=session"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    for (name, value) in overrides {
        headers.insert(*name, HeaderValue::from_static(value));
    }
    headers
}

fn create_body() -> Vec<u8> {
    br#"{"channelId":"channel-1","productId":"product-1","title":"  Online product  ","currency":"thb","price":"12.5"}"#.to_vec()
}

fn change_body() -> Vec<u8> {
    br#"{"expectedVersion":1,"title":"  Revised online product  "}"#.to_vec()
}

async fn reply(
    response: erpx_server_runtime::public::http_support::Response,
) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[derive(Default)]
struct HttpProbe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
    bad: bool,
}

impl EcommerceListingHttpBackend for HttpProbe {
    fn create_listing<'a>(
        &'a self,
        request: EcommerceListingCreateHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "create|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.key,
                request.channel_id,
                request.product_id,
                request.title,
                request.currency,
                request.price,
                request
                    .headers
                    .get("cookie")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(EcommerceHttpResponse {
                data: if self.bad {
                    json!({"id":LISTING_ID,"status":"draft"})
                } else {
                    json!({"id":LISTING_ID,"status":"draft","version":1})
                },
                context: json!({"userId":"user"}),
            })
        })
    }

    fn change_listing<'a>(
        &'a self,
        request: EcommerceListingChangeHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "change|{}|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                request.key,
                request.id,
                request.expected_version,
                request.title
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(EcommerceHttpResponse {
                data: if self.bad {
                    json!({"id":"listing-1","status":"draft"})
                } else {
                    json!({"id":"listing-1","status":"draft","version":2})
                },
                context: json!({"userId":"user"}),
            })
        })
    }
}

#[test]
fn listing_http_routes_forward_validated_commands() {
    sqlx::test_block_on(async {
        let probe = Arc::new(HttpProbe::default());
        let state = EcommerceListingHttpState::new(vec![http_target(probe.clone())]).unwrap();

        let response = state
            .create_listing(&headers(&[]), "acme", None, &create_body())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("cache-control")
                .unwrap()
                .to_str()
                .unwrap(),
            "private, no-store"
        );
        let (_, value) = reply(response).await;
        assert_eq!(value["data"], json!({"id":LISTING_ID,"status":"draft","version":1}));
        assert_eq!(value["context"], json!({"userId":"user"}));

        let response = state
            .change_listing(&headers(&[]), "acme", "listing-1", None, &change_body())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let (_, value) = reply(response).await;
        assert_eq!(value["data"]["version"], 2);

        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            [
                format!(
                    "create|acme.example.test|acme|{}|channel-1|product-1|Online product|THB|12.5|__Host-erpx-product=session",
                    KEY
                ),
                format!("change|acme.example.test|acme|{}|listing-1|1|Revised online product", KEY),
            ]
        );
    });
}

#[test]
fn listing_http_rejects_targets_origins_keys_bodies_and_bad_receipts() {
    sqlx::test_block_on(async {
        let probe = Arc::new(HttpProbe::default());
        let state = EcommerceListingHttpState::new(vec![http_target(probe.clone())]).unwrap();
        for (headers, expected) in [
            (
                headers(&[("host", "evil.example.test")]),
                StatusCode::NOT_FOUND,
            ),
            (
                headers(&[("origin", "https://evil.example.test")]),
                StatusCode::FORBIDDEN,
            ),
            (
                {
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        "host",
                        HeaderValue::from_static("acme.example.test"),
                    );
                    headers
                },
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let response = state.create_listing(&headers, "acme", None, &create_body()).await;
            assert_eq!(response.status(), expected);
        }
        let no_key = headers(&[("idempotency-key", "")]);
        assert_eq!(
            state
                .create_listing(&no_key, "acme", None, &create_body())
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            state
                .create_listing(&headers(&[]), "acme", Some("x=1"), &create_body())
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        for body in [
            br#"{"channelId":"channel-1","productId":"product-1","title":"Online product","currency":"THB"}"#.to_vec(),
            br#"{"channelId":"channel-1","productId":"product-1","title":"Online product","currency":"THB","price":"12.5","extra":true}"#.to_vec(),
            br#"{"channelId":"channel-1","productId":"product-1","title":"Online product","currency":"THB","price":"12.345"}"#.to_vec(),
            br#"{"channelId":"bad\u0007id","productId":"product-1","title":"Online product","currency":"THB","price":"12.5"}"#.to_vec(),
            b"not-json".to_vec(),
        ] {
            assert_eq!(
                state.create_listing(&headers(&[]), "acme", None, &body).await.status(),
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
        for body in [
            br#"{"expectedVersion":1}"#.to_vec(),
            br#"{"expectedVersion":1,"title":"Revised","extra":true}"#.to_vec(),
            br#"{"expectedVersion":0,"title":"Revised"}"#.to_vec(),
            br#"{"expectedVersion":1,"title":"a"}"#.to_vec(),
        ] {
            assert_eq!(
                state
                    .change_listing(&headers(&[]), "acme", "listing-1", None, &body)
                    .await
                    .status(),
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
        assert_eq!(
            state
                .change_listing(
                    &headers(&[]),
                    "acme",
                    "bad\u{7}id",
                    None,
                    &change_body()
                )
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert!(probe.seen.lock().unwrap().is_empty());

        let bad = EcommerceListingHttpState::new(vec![http_target(Arc::new(HttpProbe {
            bad: true,
            ..HttpProbe::default()
        }))])
        .unwrap();
        assert_eq!(
            bad.create_listing(&headers(&[]), "acme", None, &create_body())
                .await
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            bad.change_listing(&headers(&[]), "acme", "listing-1", None, &change_body())
                .await
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );

        let failing = EcommerceListingHttpState::new(vec![http_target(Arc::new(HttpProbe {
            fail: Some(BusinessError::new(
                FailureKind::Conflict,
                "ECOMMERCE_LISTING_AUTHOR_REQUIRED",
                "Only the draft author can change it.",
            )),
            ..HttpProbe::default()
        }))])
        .unwrap();
        let (status, value) = reply(
            failing
                .change_listing(&headers(&[]), "acme", "listing-1", None, &change_body())
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(value["error"]["code"], "ECOMMERCE_LISTING_AUTHOR_REQUIRED");
    });
}

#[test]
fn listing_http_configuration_requires_distinct_https_targets() {
    let backend: Arc<dyn EcommerceListingHttpBackend> = Arc::new(HttpProbe::default());
    for targets in [
        vec![http_target(backend.clone()), http_target(backend.clone())],
        vec![EcommerceListingHttpTarget {
            origin: "http://acme.example.test".into(),
            ..http_target(backend.clone())
        }],
        vec![EcommerceListingHttpTarget {
            holding_slug: "Acme".into(),
            ..http_target(backend.clone())
        }],
        vec![EcommerceListingHttpTarget {
            host: String::new(),
            ..http_target(backend)
        }],
    ] {
        let error = EcommerceListingHttpState::new(targets).err().unwrap();
        assert_eq!(error.code, "ECOMMERCE_HTTP_CONFIGURATION_INVALID");
    }
}
