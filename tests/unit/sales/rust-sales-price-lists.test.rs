//! Native Sales price-list domain/application tests.
//!
//! These use a recording owner repository. They prove permission/scope ordering,
//! source checks before write, exact idempotency replay, expected-version CAS,
//! independent activation, receipt/audit/outbox sequencing and fail-closed
//! readback. Raw PostgreSQL behavior is covered by the separate postgres target.
#![allow(dead_code)]
use erpx_server_sales::public::sales_price_lists::{
    AuditEntry, OutboxEntry, PriceListActor, PriceListCommandReceipt, PriceListCommandRepository,
    PriceListDecision, PriceListDecisionAction, PriceListDocument, PriceListFuture, PriceListInput,
    PriceListItemInput, PriceListItemRow, PriceListProductSource, PriceListRow, PriceListStatus,
    PriceListUpdate, PriceListWritePlan, ReceiptEntry, create_price_list, decide_price_list,
    update_price_list,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis, UserId,
};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_800_000_000_000;
const COMPANY: &str = "company";
const CREATOR: &str = "creator";
const REVIEWER: &str = "reviewer";

fn actor(user: &str, permissions: &[&str], branch: Option<&str>) -> PriceListActor {
    PriceListActor {
        user_id: UserId::new(user).unwrap(),
        company_id: CompanyId::new(COMPANY).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new(format!("assignment-{user}")).unwrap(),
        role: Some(if user == REVIEWER {
            "sales_manager".into()
        } else {
            "sales_operator".into()
        }),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["sales".into()],
        expires_at: NOW + 60_000,
    }
}

fn item(product: &str, price: i64) -> PriceListItemInput {
    PriceListItemInput {
        product_id: product.into(),
        price_minor: price,
        tax_bps: 700,
    }
}

fn input(name: &str) -> PriceListInput {
    PriceListInput {
        name: name.into(),
        currency: "THB".into(),
        customer_id: Some("customer".into()),
        valid_from: "2026-11-01".into(),
        valid_until: "2026-11-30".into(),
        max_auto_discount_bps: 500,
        items: vec![item("product-1", 10_000), item("product-2", 20_000)],
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn draft_list(id: &str, version: i64) -> PriceListRow {
    PriceListRow {
        id: id.into(),
        company_id: COMPANY.into(),
        name: "Standard pricing".into(),
        currency: "THB".into(),
        customer_id: Some("customer".into()),
        valid_from: "2026-11-01".into(),
        valid_until: "2026-11-30".into(),
        max_auto_discount_bps: 500,
        status: PriceListStatus::Draft.as_str().into(),
        version,
        created_by: CREATOR.into(),
        prepared_by: CREATOR.into(),
        approved_by: None,
        approval_reason: None,
        activated_at: None,
        created_at: NOW - 10,
        updated_at: NOW - 10,
    }
}

fn draft_items(id: &str, version: i64) -> Vec<PriceListItemRow> {
    input("Standard pricing")
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| PriceListItemRow {
            id: format!("{id}-v{version}-item-{index}"),
            price_list_id: id.into(),
            product_id: item.product_id.clone(),
            price_minor: item.price_minor,
            tax_bps: item.tax_bps,
            ordinal: index as i64,
        })
        .collect()
}

#[derive(Default)]
struct State {
    authority: bool,
    authority_checks: Vec<bool>,
    currency: Option<String>,
    customer_ok: bool,
    products: HashMap<String, PriceListProductSource>,
    product_drift: bool,
    currency_calls: usize,
    customer_calls: usize,
    product_calls: usize,
    locks: usize,
    writes: Vec<&'static str>,
    receipts: HashMap<String, PriceListCommandReceipt>,
    list: Option<PriceListRow>,
    items: Vec<PriceListItemRow>,
    readback_override: Option<PriceListRow>,
    drop_items_on_readback: bool,
}

#[derive(Clone, Default)]
struct Fake {
    state: Arc<Mutex<State>>,
}

impl Fake {
    fn ready() -> Self {
        let mut state = State {
            authority: true,
            currency: Some("THB".into()),
            customer_ok: true,
            ..State::default()
        };
        for product in ["product-1", "product-2"] {
            state.products.insert(
                product.into(),
                PriceListProductSource {
                    id: product.into(),
                    version: 3,
                },
            );
        }
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn snapshot(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    fn seed(&self, list: PriceListRow, items: Vec<PriceListItemRow>) {
        let mut state = self.snapshot();
        state.list = Some(list);
        state.items = items;
    }
}

fn hash(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:064x}", hasher.finish())
}

impl PriceListCommandRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a PriceListActor,
        approve: bool,
        _: TimestampMillis,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            state.authority_checks.push(approve);
            Ok(state.authority)
        })
    }

    fn lock_command<'a>(&'a mut self, _: &'a str) -> PriceListFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.state.lock().unwrap().locks += 1;
            Ok(())
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> PriceListFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok(hash(value)) })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListCommandReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.state.lock().unwrap().receipts.get(key).cloned()) })
    }

    fn currency<'a>(
        &'a mut self,
        _: &'a CompanyId,
    ) -> PriceListFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            state.currency_calls += 1;
            Ok(state.currency.clone())
        })
    }

    fn customer_valid<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            state.customer_calls += 1;
            Ok(state.customer_ok)
        })
    }

    fn product<'a>(
        &'a mut self,
        _: &'a CompanyId,
        product_id: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListProductSource>, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            state.product_calls += 1;
            Ok(state.products.get(product_id).cloned())
        })
    }

    fn product_unchanged<'a>(
        &'a mut self,
        _: &'a CompanyId,
        product_id: &'a str,
        version: i64,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let state = self.state.lock().unwrap();
            Ok(!state.product_drift
                && state
                    .products
                    .get(product_id)
                    .is_some_and(|product| product.version == version))
        })
    }

    fn list<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
        _: bool,
    ) -> PriceListFuture<'a, Option<PriceListRow>, Self::Error> {
        Box::pin(async move {
            let state = self.state.lock().unwrap();
            Ok(state.list.as_ref().filter(|list| list.id == id).cloned())
        })
    }

    fn items<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
    ) -> PriceListFuture<'a, Vec<PriceListItemRow>, Self::Error> {
        Box::pin(async move {
            let state = self.state.lock().unwrap();
            Ok(state
                .items
                .iter()
                .filter(|item| item.price_list_id == id)
                .cloned()
                .collect())
        })
    }

    fn insert_list<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            if state
                .list
                .as_ref()
                .is_some_and(|list| list.id == plan.row.id)
            {
                return Ok(false);
            }
            state.writes.push("insert_list");
            state.list = Some(plan.row.clone());
            Ok(true)
        })
    }

    fn update_list<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            let Some(list) = state.list.clone() else {
                return Ok(false);
            };
            if list.version != plan.row.version - 1 || list.status() != Some(PriceListStatus::Draft)
            {
                return Ok(false);
            }
            state.writes.push("update_list");
            state.list = Some(plan.row.clone());
            Ok(true)
        })
    }

    fn replace_items<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            state.writes.push("replace_items");
            state.items = plan.items.to_vec();
            Ok(true)
        })
    }

    fn decide_list<'a>(
        &'a mut self,
        row: &'a PriceListRow,
        expected_version: i64,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            let Some(list) = state.list.clone() else {
                return Ok(false);
            };
            if list.version != expected_version || list.status() != Some(PriceListStatus::Draft) {
                return Ok(false);
            }
            state.writes.push("decide_list");
            state.list = Some(row.clone());
            Ok(true)
        })
    }

    fn readback<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListDocument>, Self::Error> {
        Box::pin(async move {
            let state = self.state.lock().unwrap();
            let Some(list) = state
                .readback_override
                .clone()
                .or_else(|| state.list.clone())
                .filter(|list| list.id == id)
            else {
                return Ok(None);
            };
            let items = if state.drop_items_on_readback {
                Vec::new()
            } else {
                state.items.clone()
            };
            Ok(Some(PriceListDocument { list, items }))
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        _: TimestampMillis,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            if state.receipts.contains_key(entry.key) {
                return Ok(false);
            }
            state.receipts.insert(
                entry.key.into(),
                PriceListCommandReceipt {
                    request_hash: entry.request_hash.into(),
                    result_json: entry.result_json.into(),
                    operation: Some(entry.operation.into()),
                    actor_id: Some(entry.actor.user_id.as_str().into()),
                    record_id: Some(entry.record_id.into()),
                    authority_json: Some(entry.actor.authority_json()),
                },
            );
            state.writes.push("receipt");
            Ok(true)
        })
    }

    fn audit<'a>(
        &'a mut self,
        _: &'a PriceListActor,
        _: &'a AuditEntry<'a>,
        _: TimestampMillis,
    ) -> PriceListFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.state.lock().unwrap().writes.push("audit");
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        _: &'a OutboxEntry<'a>,
        _: TimestampMillis,
    ) -> PriceListFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.state.lock().unwrap().writes.push("outbox");
            Ok(())
        })
    }
}

fn requirements() -> [&'static str; 2] {
    ["commerce.read", "commerce.sales"]
}

#[tokio::test]
async fn create_checks_authority_before_sources_and_replays_exactly() {
    let mut repo = Fake::ready();
    repo.snapshot().authority = false;
    let operator = actor(CREATOR, &requirements(), None);
    let raw = input("Standard pricing");
    let error = create_price_list(
        &operator,
        raw.clone(),
        &key("price-list-create-0001"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_PRICING_PERMISSION_REQUIRED");
    {
        let state = repo.snapshot();
        assert!(state.writes.is_empty());
        assert_eq!(state.currency_calls, 0);
        assert_eq!(state.authority_checks, vec![false]);
    }

    let mut repo = Fake::ready();
    let receipt = create_price_list(
        &operator,
        raw.clone(),
        &key("price-list-create-0002"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "price-list-price-list-create-0002");
    assert_eq!(receipt.status, "draft");
    assert_eq!(receipt.version, 1);
    {
        let state = repo.snapshot();
        assert_eq!(state.locks, 1);
        assert_eq!(state.currency_calls, 2);
        assert_eq!(state.customer_calls, 2);
        assert_eq!(state.product_calls, 2);
        assert_eq!(
            state.writes,
            vec!["insert_list", "replace_items", "receipt", "audit", "outbox"]
        );
        assert_eq!(state.list.as_ref().unwrap().prepared_by, CREATOR);
        assert_eq!(state.items.len(), 2);
        assert_eq!(state.items[0].id, format!("{}-item-0", receipt.id));
    }

    let replay = create_price_list(
        &operator,
        raw.clone(),
        &key("price-list-create-0002"),
        &mut repo,
        TimestampMillis::new(NOW + 1),
    )
    .await
    .unwrap();
    assert_eq!(replay, receipt);
    {
        let state = repo.snapshot();
        assert_eq!(state.writes.len(), 5);
        assert_eq!(state.authority_checks, vec![false]);
        assert_eq!(state.currency_calls, 2);
    }

    let mut changed = raw;
    changed.name = "Different pricing".into();
    let error = create_price_list(
        &operator,
        changed,
        &key("price-list-create-0002"),
        &mut repo,
        TimestampMillis::new(NOW + 2),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn create_rejects_invalid_sources_and_inputs_before_writing() {
    let operator = actor(CREATOR, &requirements(), None);

    let mut repo = Fake::ready();
    repo.snapshot().currency = Some("USD".into());
    let error = create_price_list(
        &operator,
        input("Standard pricing"),
        &key("price-list-source-0001"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_CURRENCY_INVALID");
    assert!(repo.snapshot().writes.is_empty());

    let mut repo = Fake::ready();
    repo.snapshot().customer_ok = false;
    let error = create_price_list(
        &operator,
        input("Standard pricing"),
        &key("price-list-source-0002"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_CUSTOMER_INVALID");
    assert!(repo.snapshot().writes.is_empty());

    let mut repo = Fake::ready();
    repo.snapshot().products.remove("product-2");
    let error = create_price_list(
        &operator,
        input("Standard pricing"),
        &key("price-list-source-0003"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_PRODUCT_INVALID");
    assert!(repo.snapshot().writes.is_empty());

    let mut duplicate = input("Standard pricing");
    duplicate.items[1].product_id = "product-1".into();
    let mut repo = Fake::ready();
    let error = create_price_list(
        &operator,
        duplicate,
        &key("price-list-source-0004"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_PRODUCT_DUPLICATE");
    {
        let state = repo.snapshot();
        assert_eq!(state.locks, 0);
        assert!(state.writes.is_empty());
    }

    let mut reversed = input("Standard pricing");
    reversed.valid_from = "2026-11-30".into();
    reversed.valid_until = "2026-11-01".into();
    let mut repo = Fake::ready();
    let error = create_price_list(
        &operator,
        reversed,
        &key("price-list-source-0005"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_DATES_INVALID");
    assert_eq!(repo.snapshot().locks, 0);

    let mut empty = input("Standard pricing");
    empty.items.clear();
    let mut repo = Fake::ready();
    let error = create_price_list(
        &operator,
        empty,
        &key("price-list-source-0006"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_REQUEST_INVALID");
    assert_eq!(repo.snapshot().locks, 0);
}

#[tokio::test]
async fn update_enforces_expected_version_and_replaces_draft_items() {
    let id = "price-list-update-0001";
    let mut repo = Fake::ready();
    repo.seed(draft_list(id, 1), draft_items(id, 1));
    let operator = actor(CREATOR, &requirements(), None);
    let receipt = update_price_list(
        &operator,
        id,
        PriceListUpdate {
            input: input("Updated pricing"),
            expected_version: 1,
        },
        &key("price-list-update-key-0001"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap();
    assert_eq!(receipt.status, "draft");
    assert_eq!(receipt.version, 2);
    {
        let state = repo.snapshot();
        assert_eq!(state.list.as_ref().unwrap().name, "Updated pricing");
        assert_eq!(state.list.as_ref().unwrap().version, 2);
        assert_eq!(state.list.as_ref().unwrap().prepared_by, CREATOR);
        assert_eq!(state.items[0].id, format!("{id}-v2-item-0"));
        assert_eq!(
            state.writes,
            vec!["update_list", "replace_items", "receipt", "audit", "outbox"]
        );
    }

    let error = update_price_list(
        &operator,
        id,
        PriceListUpdate {
            input: input("Stale pricing"),
            expected_version: 1,
        },
        &key("price-list-update-key-0002"),
        &mut repo,
        TimestampMillis::new(NOW + 1),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_CHANGED");

    let active = PriceListRow {
        status: PriceListStatus::Active.as_str().into(),
        version: 3,
        approved_by: Some(REVIEWER.into()),
        approval_reason: Some("Independent review".into()),
        activated_at: Some(NOW),
        ..draft_list(id, 3)
    };
    let mut repo = Fake::ready();
    repo.seed(active, draft_items(id, 3));
    let error = update_price_list(
        &operator,
        id,
        PriceListUpdate {
            input: input("Locked pricing"),
            expected_version: 3,
        },
        &key("price-list-update-key-0003"),
        &mut repo,
        TimestampMillis::new(NOW + 2),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_LOCKED");
    assert!(repo.snapshot().writes.is_empty());

    let error = update_price_list(
        &operator,
        "missing-list",
        PriceListUpdate {
            input: input("Missing pricing"),
            expected_version: 1,
        },
        &key("price-list-update-key-0004"),
        &mut repo,
        TimestampMillis::new(NOW + 3),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_NOT_FOUND");
}

#[tokio::test]
async fn decision_requires_approval_and_independent_activation() {
    let id = "price-list-decision-0001";
    let creator = actor(
        CREATOR,
        &["commerce.read", "commerce.sales", "commerce.approve"],
        None,
    );
    let reviewer = actor(
        REVIEWER,
        &["commerce.read", "commerce.sales", "commerce.approve"],
        None,
    );
    let manager_without_approval = actor(REVIEWER, &requirements(), None);

    let mut repo = Fake::ready();
    repo.seed(draft_list(id, 1), draft_items(id, 1));
    let error = decide_price_list(
        &creator,
        id,
        PriceListDecision {
            expected_version: 1,
            action: PriceListDecisionAction::Activate,
            reason: "Independent review".into(),
        },
        &key("price-list-decision-key-0001"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "INDEPENDENT_PRICING_APPROVER_REQUIRED");
    assert!(repo.snapshot().writes.is_empty());
    let locks_before = repo.snapshot().locks;

    let error = decide_price_list(
        &manager_without_approval,
        id,
        PriceListDecision {
            expected_version: 1,
            action: PriceListDecisionAction::Activate,
            reason: "Independent review".into(),
        },
        &key("price-list-decision-key-0002"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_PRICING_APPROVAL_REQUIRED");
    assert_eq!(repo.snapshot().locks, locks_before);

    let receipt = decide_price_list(
        &reviewer,
        id,
        PriceListDecision {
            expected_version: 1,
            action: PriceListDecisionAction::Activate,
            reason: "Independent review".into(),
        },
        &key("price-list-decision-key-0003"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap();
    assert_eq!(receipt.status, "active");
    assert_eq!(receipt.version, 2);
    {
        let state = repo.snapshot();
        let list = state.list.as_ref().unwrap();
        assert_eq!(list.approved_by.as_deref(), Some(REVIEWER));
        assert_eq!(list.approval_reason.as_deref(), Some("Independent review"));
        assert_eq!(list.activated_at, Some(NOW));
        assert_eq!(list.prepared_by, CREATOR);
        assert_eq!(
            state.writes,
            vec!["decide_list", "receipt", "audit", "outbox"]
        );
        assert_eq!(state.authority_checks.last(), Some(&true));
    }

    let replay = decide_price_list(
        &reviewer,
        id,
        PriceListDecision {
            expected_version: 1,
            action: PriceListDecisionAction::Activate,
            reason: "Independent review".into(),
        },
        &key("price-list-decision-key-0003"),
        &mut repo,
        TimestampMillis::new(NOW + 1),
    )
    .await
    .unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(repo.snapshot().writes.len(), 4);

    let mut repo = Fake::ready();
    repo.seed(draft_list(id, 1), draft_items(id, 1));
    let cancelled = decide_price_list(
        &reviewer,
        id,
        PriceListDecision {
            expected_version: 1,
            action: PriceListDecisionAction::Cancel,
            reason: "Superseded before activation".into(),
        },
        &key("price-list-decision-key-0004"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let list = repo.snapshot().list.clone().unwrap();
    assert_eq!(list.approved_by, None);
    assert_eq!(list.activated_at, None);

    let error = decide_price_list(
        &reviewer,
        id,
        PriceListDecision {
            expected_version: 1,
            action: PriceListDecisionAction::Activate,
            reason: "Independent review".into(),
        },
        &key("price-list-decision-key-0005"),
        &mut repo,
        TimestampMillis::new(NOW + 1),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_LOCKED");

    let error = decide_price_list(
        &reviewer,
        id,
        PriceListDecision {
            expected_version: 2,
            action: PriceListDecisionAction::Cancel,
            reason: "no".into(),
        },
        &key("price-list-decision-key-0006"),
        &mut repo,
        TimestampMillis::new(NOW + 2),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_REQUEST_INVALID");
}

#[tokio::test]
async fn product_drift_and_readback_mismatch_fail_closed_after_write() {
    let id = "price-list-readback-0001";
    let operator = actor(CREATOR, &requirements(), None);
    let mut repo = Fake::ready();
    repo.snapshot().product_drift = true;
    let error = create_price_list(
        &operator,
        input("Standard pricing"),
        &key("price-list-readback-key-0001"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PRICE_LIST_PRODUCT_INVALID");
    {
        let state = repo.snapshot();
        assert_eq!(
            state.writes,
            vec!["insert_list", "replace_items"],
            "no receipt/audit/outbox is written after a late source drift"
        );
    }

    let mut repo = Fake::ready();
    repo.snapshot().drop_items_on_readback = true;
    let error = create_price_list(
        &operator,
        input("Standard pricing"),
        &key("price-list-readback-key-0002"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_PRICING_READBACK_INVALID");
    assert!(!repo.snapshot().writes.contains(&"receipt"));

    let mut repo = Fake::ready();
    let mut wrong = draft_list(id, 1);
    wrong.name = "Other pricing".into();
    repo.snapshot().readback_override = Some(wrong);
    let error = create_price_list(
        &operator,
        input("Standard pricing"),
        &key("price-list-readback-key-0003"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_PRICING_READBACK_INVALID");
}

#[tokio::test]
async fn module_expiry_and_company_scope_fail_before_repository_writes() {
    let id = "price-list-scope-0001";
    let mut repo = Fake::ready();
    repo.seed(draft_list(id, 1), draft_items(id, 1));

    let branch = actor(CREATOR, &requirements(), Some("branch"));
    let error = update_price_list(
        &branch,
        id,
        PriceListUpdate {
            input: input("Company pricing"),
            expected_version: 1,
        },
        &key("price-list-scope-key-0001"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "COMPANY_PRICING_SCOPE_REQUIRED");

    let mut no_module = actor(CREATOR, &requirements(), None);
    no_module.modules.clear();
    let error = update_price_list(
        &no_module,
        id,
        PriceListUpdate {
            input: input("Company pricing"),
            expected_version: 1,
        },
        &key("price-list-scope-key-0002"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SALES_PRICING_MODULE_FORBIDDEN");

    let mut expired = actor(CREATOR, &requirements(), None);
    expired.expires_at = NOW;
    let error = update_price_list(
        &expired,
        id,
        PriceListUpdate {
            input: input("Company pricing"),
            expected_version: 1,
        },
        &key("price-list-scope-key-0003"),
        &mut repo,
        TimestampMillis::new(NOW),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert!(repo.snapshot().writes.is_empty());
    assert_eq!(repo.snapshot().locks, 0);
}
