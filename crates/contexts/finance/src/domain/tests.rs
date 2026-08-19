use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::{
    BoundedContext, CostCenterRef, CurrencyCode, DimensionSet, MoneyAmount, RecordRef,
    ReferenceCode, customer, finance, organization, procurement, sales,
};
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn account_id(value: u128) -> AccountId {
    AccountId::try_from_uuid(id(value)).unwrap()
}
fn account(
    value: u128,
    code: &str,
    category: AccountCategory,
    parent: Option<AccountId>,
) -> Account {
    Account {
        account_id: account_id(value),
        code: code.into(),
        name: code.into(),
        category,
        normal_balance: if matches!(category, AccountCategory::Asset | AccountCategory::Expense) {
            NormalBalance::Debit
        } else {
            NormalBalance::Credit
        },
        parent_account_id: parent,
        status: AccountStatus::Active,
    }
}
fn chart() -> Chart {
    Chart::create(
        ChartId::try_from_uuid(id(10)).unwrap(),
        scope(),
        "MAIN".into(),
        "Main".into(),
        "THB".into(),
        vec![
            account(11, "1100", AccountCategory::Asset, None),
            account(12, "4100", AccountCategory::Revenue, None),
        ],
        at(),
    )
    .unwrap()
}
fn period() -> AccountingPeriod {
    AccountingPeriod::create(
        PeriodId::try_from_uuid(id(20)).unwrap(),
        scope(),
        "2026-08".into(),
        date(1),
        date(31),
        at(),
    )
    .unwrap()
}
fn lines() -> Vec<JournalLine> {
    vec![
        JournalLine {
            line_id: JournalLineId::try_from_uuid(id(31)).unwrap(),
            account_id: account_id(11),
            debit_minor: 100,
            credit_minor: 0,
            currency: "THB".into(),
            memo: "cash".into(),
        },
        JournalLine {
            line_id: JournalLineId::try_from_uuid(id(32)).unwrap(),
            account_id: account_id(12),
            debit_minor: 0,
            credit_minor: 100,
            currency: "THB".into(),
            memo: "revenue".into(),
        },
    ]
}

#[test]
fn chart_rejects_cycles_and_duplicate_codes_without_partial_mutation() {
    let duplicate = Chart::create(
        ChartId::try_from_uuid(id(10)).unwrap(),
        scope(),
        "MAIN".into(),
        "Main".into(),
        "THB".into(),
        vec![
            account(11, "1100", AccountCategory::Asset, None),
            account(12, "1100", AccountCategory::Revenue, None),
        ],
        at(),
    );
    assert!(matches!(
        duplicate,
        Err(FinanceDomainError::InvalidAccountHierarchy)
    ));
    let mut value = chart();
    let before = value.clone();
    assert!(
        value
            .add_account(
                account(13, "1100", AccountCategory::Asset, None),
                at() + Duration::minutes(1)
            )
            .is_err()
    );
    assert_eq!(value, before);
}

#[test]
fn journal_requires_balanced_unique_lines_in_one_currency() {
    let mut invalid = lines();
    invalid[1].credit_minor = 99;
    assert_eq!(
        Journal::create(
            JournalId::try_from_uuid(id(30)).unwrap(),
            scope(),
            chart().id(),
            period().id(),
            "J-30".into(),
            date(5),
            "THB".into(),
            JournalSource::Manual {
                reference: "MAN-30".into()
            },
            "posting".into(),
            invalid,
            at()
        ),
        Err(FinanceDomainError::UnbalancedJournal)
    );
}

#[test]
fn posting_requires_open_bound_period_and_active_accounts() {
    let chart = chart();
    let period = period();
    let mut journal = Journal::create(
        JournalId::try_from_uuid(id(30)).unwrap(),
        scope(),
        chart.id(),
        period.id(),
        "J-30".into(),
        date(5),
        "THB".into(),
        JournalSource::Manual {
            reference: "MAN-30".into(),
        },
        "posting".into(),
        lines(),
        at(),
    )
    .unwrap();
    journal
        .post(&chart, &period, at() + Duration::minutes(1))
        .unwrap();
    assert_eq!(journal.status(), JournalStatus::Posted);
    assert_eq!(journal.debit_total_minor(), journal.credit_total_minor());
    assert_eq!(
        journal.post(&chart, &period, at() + Duration::minutes(2)),
        Err(FinanceDomainError::InvalidJournalTransition)
    );
}

#[test]
fn close_requires_distinct_requester_and_approver_and_reopen_actor() {
    let mut value = period();
    value
        .begin_close("requester", at() + Duration::minutes(1))
        .unwrap();
    let before = value.clone();
    assert_eq!(
        value.close("requester", at() + Duration::minutes(2)),
        Err(FinanceDomainError::CloseApprovalSeparation)
    );
    assert_eq!(value, before);
    value
        .close("approver", at() + Duration::minutes(2))
        .unwrap();
    assert_eq!(
        value.reopen("approver", at() + Duration::minutes(3)),
        Err(FinanceDomainError::InvalidPeriodTransition)
    );
    value
        .reopen("controller", at() + Duration::minutes(3))
        .unwrap();
    assert_eq!(value.status(), PeriodStatus::Open);
}

fn billing_fact() -> sales::BillingEligibilityFact {
    sales::BillingEligibilityFact {
        billing_event: sales::BillingEventRef::try_from_uuid(id(39)).unwrap(),
        fulfillment: sales::FulfillmentRef::try_from_uuid(id(38)).unwrap(),
        arrangement: sales::RevenueArrangementRef::try_from_uuid(id(37)).unwrap(),
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        order_code: ReferenceCode::try_new("SO-40").unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        billable_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        tax_amount: MoneyAmount::new(35, CurrencyCode::try_new("THB").unwrap()),
        eligible_on: date(5),
        dimensions: dimensions(),
    }
}

fn dimensions() -> DimensionSet {
    let mut value =
        DimensionSet::for_legal_entity(organization::CompanyRef::try_from_uuid(id(2)).unwrap());
    value.business_unit = Some(organization::BusinessUnitRef::try_from_uuid(id(30)).unwrap());
    value.cost_center = Some(CostCenterRef::try_from_uuid(id(31)).unwrap());
    value
}

fn invoice_fact() -> finance::CustomerInvoicePostedFact {
    finance::CustomerInvoicePostedFact {
        invoice: finance::CustomerInvoiceRef::try_from_uuid(id(49)).unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        invoice_code: ReferenceCode::try_new("INV-49").unwrap(),
        source: RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 2)
            .unwrap(),
        net_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        tax_amount: MoneyAmount::new(35, CurrencyCode::try_new("THB").unwrap()),
        receivable_amount: MoneyAmount::new(535, CurrencyCode::try_new("THB").unwrap()),
        dimensions: dimensions(),
        posted_at: at(),
    }
}

fn arrangement_fact() -> sales::RevenueArrangementFact {
    sales::RevenueArrangementFact {
        arrangement: sales::RevenueArrangementRef::try_from_uuid(id(37)).unwrap(),
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        legal_contract: None,
        order_code: ReferenceCode::try_new("SO-40").unwrap(),
        dimensions: dimensions(),
        obligations: vec![
            sales::PerformanceObligationFact {
                obligation: sales::PerformanceObligationRef::try_from_uuid(id(42)).unwrap(),
                line: sales::SalesOrderLineRef::try_from_uuid(id(43)).unwrap(),
                model: sales::RevenueModel::Goods,
                allocated_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
                service_starts_on: None,
                service_ends_on: None,
            },
            sales::PerformanceObligationFact {
                obligation: sales::PerformanceObligationRef::try_from_uuid(id(44)).unwrap(),
                line: sales::SalesOrderLineRef::try_from_uuid(id(45)).unwrap(),
                model: sales::RevenueModel::Recurring,
                allocated_amount: MoneyAmount::new(300, CurrencyCode::try_new("THB").unwrap()),
                service_starts_on: Some(date(1)),
                service_ends_on: Some(date(31)),
            },
        ],
    }
}

fn fulfillment(
    value: u128,
    obligation: u128,
    kind: sales::FulfillmentKind,
    amount: i64,
) -> sales::FulfillmentAcceptedFact {
    sales::FulfillmentAcceptedFact {
        fulfillment: sales::FulfillmentRef::try_from_uuid(id(value)).unwrap(),
        arrangement: sales::RevenueArrangementRef::try_from_uuid(id(37)).unwrap(),
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        obligation: sales::PerformanceObligationRef::try_from_uuid(id(obligation)).unwrap(),
        kind,
        accepted_amount: MoneyAmount::new(amount, CurrencyCode::try_new("THB").unwrap()),
        accepted_at: at(),
        dimensions: dimensions(),
    }
}

#[test]
fn receivable_uses_posted_invoice_fact_and_cannot_over_settle() {
    let mut item = SubledgerItem::receivable_from_invoice(
        SubledgerId::try_from_uuid(id(50)).unwrap(),
        scope(),
        invoice_fact(),
        at(),
    )
    .unwrap();
    let before = item.clone();
    assert_eq!(
        item.apply_settlement(536, at() + Duration::minutes(1)),
        Err(FinanceDomainError::InvalidSettlement)
    );
    assert_eq!(item, before);
    item.apply_settlement(200, at() + Duration::minutes(1))
        .unwrap();
    assert_eq!(item.outstanding_minor(), 335);
    assert_eq!(item.status(), SubledgerStatus::PartiallySettled);
}

#[test]
fn customer_invoice_requires_versioned_sales_source_and_maker_checker_posting() {
    let mut invoice = CustomerInvoice::create_from_billing(
        CustomerInvoiceId::try_from_uuid(id(48)).unwrap(),
        scope(),
        "INV-48".into(),
        RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 1).unwrap(),
        billing_fact(),
        date(30),
        "preparer".into(),
        at(),
    )
    .unwrap();
    assert!(matches!(
        invoice.take_uncommitted_events().as_slice(),
        [DomainEvent::CustomerInvoiceChanged]
    ));
    let before = invoice.clone();
    assert_eq!(
        invoice.post("preparer", at()),
        Err(FinanceDomainError::InvoiceApprovalSeparation)
    );
    assert_eq!(invoice, before);
    invoice.post("controller", at()).unwrap();
    assert_eq!(invoice.status(), CustomerInvoiceStatus::Posted);
    assert!(matches!(
        invoice.take_uncommitted_events().as_slice(),
        [DomainEvent::CustomerInvoicePosted(_)]
    ));
}

#[test]
fn revenue_schedule_recognizes_fulfillment_independently_from_invoice() {
    let mut schedule = RevenueSchedule::create_from_arrangement(
        RevenueScheduleId::try_from_uuid(id(70)).unwrap(),
        scope(),
        RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 1).unwrap(),
        arrangement_fact(),
        at(),
    )
    .unwrap();
    schedule.take_uncommitted_events();
    schedule
        .recognize_fulfillment(
            RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 2).unwrap(),
            fulfillment(71, 42, sales::FulfillmentKind::GoodsDelivery, 500),
            at() + Duration::minutes(1),
        )
        .unwrap();
    assert_eq!(schedule.recognized_total_minor().unwrap(), 500);
    assert_eq!(schedule.status(), RevenueScheduleStatus::Active);
    assert!(matches!(
        schedule.take_uncommitted_events().as_slice(),
        [DomainEvent::RevenueRecognized(_)]
    ));

    let before = schedule.clone();
    assert_eq!(
        schedule.recognize_fulfillment(
            RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 3).unwrap(),
            fulfillment(71, 42, sales::FulfillmentKind::GoodsDelivery, 1),
            at() + Duration::minutes(2),
        ),
        Err(FinanceDomainError::InvalidRevenueRecognition)
    );
    assert_eq!(schedule, before);

    schedule
        .recognize_fulfillment(
            RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 3).unwrap(),
            fulfillment(72, 44, sales::FulfillmentKind::RecurringPeriod, 300),
            at() + Duration::minutes(2),
        )
        .unwrap();
    assert_eq!(schedule.recognized_total_minor().unwrap(), 800);
    assert_eq!(schedule.status(), RevenueScheduleStatus::Completed);
    schedule.validate().unwrap();
}

#[test]
fn payable_totals_released_procurement_lines_without_importing_procurement_core() {
    let fact = procurement::ReleasedPurchaseOrderFact {
        purchase_order: procurement::PurchaseOrderRef::try_from_uuid(id(60)).unwrap(),
        supplier: procurement::SupplierRef::try_from_uuid(id(61)).unwrap(),
        order_code: ReferenceCode::try_new("PO-60").unwrap(),
        lines: vec![
            procurement::ReleasedPurchaseOrderLineFact {
                line: procurement::PurchaseOrderLineRef::try_from_uuid(id(62)).unwrap(),
                item_code: ReferenceCode::try_new("ITEM").unwrap(),
                ordered_quantity: erpx_integration_contracts::Quantity::try_new(2, 0).unwrap(),
                line_value: MoneyAmount::new(250, CurrencyCode::try_new("THB").unwrap()),
                destination_code: ReferenceCode::try_new("WH").unwrap(),
            },
            procurement::ReleasedPurchaseOrderLineFact {
                line: procurement::PurchaseOrderLineRef::try_from_uuid(id(63)).unwrap(),
                item_code: ReferenceCode::try_new("SHIP").unwrap(),
                ordered_quantity: erpx_integration_contracts::Quantity::try_new(1, 0).unwrap(),
                line_value: MoneyAmount::new(50, CurrencyCode::try_new("THB").unwrap()),
                destination_code: ReferenceCode::try_new("WH").unwrap(),
            },
        ],
    };
    let item = SubledgerItem::payable_from_procurement(
        SubledgerId::try_from_uuid(id(64)).unwrap(),
        scope(),
        fact,
        at(),
    )
    .unwrap();
    assert_eq!(item.original_minor(), 300);
    assert!(matches!(item.kind(), SubledgerKind::Payable(_)));
}
