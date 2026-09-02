use erpx_integration_contracts::{
    BoundedContext, CurrencyCode, MoneyAmount, ReferenceCode, finance,
};

use crate::domain::{DomainEvent, JournalSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidContractValue,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<finance::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<finance::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::JournalPosted(state) => Some((|| {
            let (source_context, source_reference) = source_identity(&state.source)?;
            let currency = CurrencyCode::try_new(&state.currency)
                .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
            Ok(finance::Event::JournalPosted(finance::JournalPostingFact {
                journal: finance::JournalRef::try_from_uuid(state.journal_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                journal_code: ReferenceCode::try_new(state.journal_code.clone())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                source_context,
                source_reference,
                debit_total: MoneyAmount::new(state.debit_total_minor, currency),
                credit_total: MoneyAmount::new(state.credit_total_minor, currency),
                posted_at: state.posted_at,
            }))
        })()),
        DomainEvent::ReceivableChanged(state) => Some((|| {
            let currency = CurrencyCode::try_new(&state.currency)
                .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
            Ok(finance::Event::ReceivableChanged(finance::ReceivableFact {
                receivable: finance::ReceivableRef::try_from_uuid(state.subledger_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                customer: state.customer,
                document_code: ReferenceCode::try_new(state.document_code.clone())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                outstanding_amount: MoneyAmount::new(state.outstanding_minor, currency),
            }))
        })()),
        DomainEvent::CustomerInvoicePosted(state) => Some((|| {
            let currency = CurrencyCode::try_new(&state.currency)
                .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
            Ok(finance::Event::CustomerInvoicePosted(
                finance::CustomerInvoicePostedFact {
                    invoice: finance::CustomerInvoiceRef::try_from_uuid(state.invoice_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    customer: state.customer,
                    invoice_code: ReferenceCode::try_new(state.invoice_code.clone())
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    source: state.source.clone(),
                    net_amount: MoneyAmount::new(state.net_minor, currency),
                    tax_amount: MoneyAmount::new(state.tax_minor, currency),
                    receivable_amount: MoneyAmount::new(state.receivable_minor, currency),
                    dimensions: state.dimensions.clone(),
                    posted_at: state.posted_at,
                },
            ))
        })()),
        DomainEvent::RevenueRecognized(state) => Some((|| {
            let currency = CurrencyCode::try_new(&state.currency)
                .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
            Ok(finance::Event::RevenueRecognized(
                finance::RecognizedRevenueFact {
                    schedule: finance::RevenueScheduleRef::try_from_uuid(
                        state.schedule_id.as_uuid(),
                    )
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    customer: state.customer,
                    source: state.source.clone(),
                    fulfillment: state.fulfillment,
                    obligation: state.obligation,
                    recognized_amount: MoneyAmount::new(state.recognized_minor, currency),
                    dimensions: state.dimensions.clone(),
                    recognized_at: state.recognized_at,
                },
            ))
        })()),
        DomainEvent::ChartChanged
        | DomainEvent::PeriodChanged
        | DomainEvent::JournalChanged
        | DomainEvent::SubledgerChanged
        | DomainEvent::CustomerInvoiceChanged
        | DomainEvent::RevenueScheduleChanged => None,
    }
}

fn source_identity(
    source: &JournalSource,
) -> Result<(BoundedContext, ReferenceCode), IntegrationMappingError> {
    let (context, reference) = match source {
        JournalSource::SalesOrder(fact) => (BoundedContext::Sales, fact.order_code.as_str()),
        JournalSource::InventoryMovement(fact) => {
            return Ok((
                BoundedContext::Inventory,
                ReferenceCode::try_new(fact.movement.as_uuid().to_string())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
            ));
        }
        JournalSource::Payroll(fact) => (BoundedContext::PeoplePayroll, fact.period_code.as_str()),
        JournalSource::PosSale(fact) => (BoundedContext::Pos, fact.receipt_code.as_str()),
        JournalSource::PosRefund(fact) => {
            return Ok((
                BoundedContext::Pos,
                ReferenceCode::try_new(fact.refund.as_uuid().to_string())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
            ));
        }
        JournalSource::BankStatement(fact) => (BoundedContext::Finance, fact.reference.as_str()),
        JournalSource::TaxAdjustment(fact) => (BoundedContext::Finance, fact.reference.as_str()),
        JournalSource::Manual { reference } => (BoundedContext::Finance, reference.as_str()),
    };
    Ok((
        context,
        ReferenceCode::try_new(reference.to_owned())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
    ))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{
        CostCenterRef, DimensionSet, IntegrationEvent, RecordRef, customer, organization, sales,
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuthorityScope, CommandId, HumanDecisionProof, SessionId,
    };
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        Account, AccountCategory, AccountId, AccountStatus, AccountingPeriod, Chart, ChartId,
        CustomerInvoice, CustomerInvoiceId, Journal, JournalId, JournalLine, JournalLineId,
        NormalBalance, PeriodId,
    };

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    #[test]
    fn only_posted_journal_is_public_and_source_owner_is_typed() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let scope = AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        };
        let human_decision = HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "finance-controller".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(id(3)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.clone(),
            crate::domain::journal_post_decision_ref(
                CommandId::try_from_uuid(id(90)).unwrap(),
                JournalId::try_from_uuid(id(30)).unwrap(),
            ),
            at,
        )
        .unwrap();
        let account = |value, code: &str, category| Account {
            account_id: AccountId::try_from_uuid(id(value)).unwrap(),
            code: code.into(),
            name: code.into(),
            category,
            normal_balance: if category == AccountCategory::Asset {
                NormalBalance::Debit
            } else {
                NormalBalance::Credit
            },
            parent_account_id: None,
            status: AccountStatus::Active,
        };
        let chart = Chart::create(
            ChartId::try_from_uuid(id(10)).unwrap(),
            scope.clone(),
            "MAIN".into(),
            "Main".into(),
            "THB".into(),
            vec![
                account(11, "1100", AccountCategory::Asset),
                account(12, "4100", AccountCategory::Revenue),
            ],
            at,
        )
        .unwrap();
        let period = AccountingPeriod::create(
            PeriodId::try_from_uuid(id(20)).unwrap(),
            scope.clone(),
            "2026-08".into(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
            at,
        )
        .unwrap();
        let mut journal = Journal::create(
            JournalId::try_from_uuid(id(30)).unwrap(),
            scope,
            chart.id(),
            period.id(),
            "J-30".into(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 5).unwrap(),
            "THB".into(),
            None,
            JournalSource::Manual {
                reference: "MAN-30".into(),
            },
            "posting".into(),
            vec![
                JournalLine {
                    line_id: JournalLineId::try_from_uuid(id(31)).unwrap(),
                    account_id: AccountId::try_from_uuid(id(11)).unwrap(),
                    debit_minor: 500,
                    credit_minor: 0,
                    currency: "THB".into(),
                    memo: "cash".into(),
                },
                JournalLine {
                    line_id: JournalLineId::try_from_uuid(id(32)).unwrap(),
                    account_id: AccountId::try_from_uuid(id(12)).unwrap(),
                    debit_minor: 0,
                    credit_minor: 500,
                    currency: "THB".into(),
                    memo: "revenue".into(),
                },
            ],
            at,
        )
        .unwrap();
        assert!(
            published_events(&journal.take_uncommitted_events())
                .unwrap()
                .is_empty()
        );
        journal
            .post(
                &chart,
                &period,
                &human_decision,
                CommandId::try_from_uuid(id(90)).unwrap(),
                at,
            )
            .unwrap();
        let events = published_events(&journal.take_uncommitted_events()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), "finance.journal_posted");
        let finance::Event::JournalPosted(fact) = &events[0] else {
            panic!("expected journal fact")
        };
        assert_eq!(fact.source_context, BoundedContext::Finance);
        assert_eq!(fact.debit_total.minor_units(), 500);
    }

    #[test]
    fn posted_customer_invoice_preserves_exact_sales_source_in_public_fact() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let scope = AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        };
        let mut dimensions =
            DimensionSet::for_legal_entity(organization::CompanyRef::try_from_uuid(id(2)).unwrap());
        dimensions.business_unit =
            Some(organization::BusinessUnitRef::try_from_uuid(id(30)).unwrap());
        dimensions.cost_center = Some(CostCenterRef::try_from_uuid(id(31)).unwrap());
        let source =
            RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 1).unwrap();
        let billing = sales::BillingEligibilityFact {
            billing_event: sales::BillingEventRef::try_from_uuid(id(39)).unwrap(),
            fulfillment: sales::FulfillmentRef::try_from_uuid(id(38)).unwrap(),
            arrangement: sales::RevenueArrangementRef::try_from_uuid(id(37)).unwrap(),
            sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
            order_code: ReferenceCode::try_new("SO-40").unwrap(),
            customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
            billable_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
            tax_amount: MoneyAmount::new(35, CurrencyCode::try_new("THB").unwrap()),
            eligible_on: chrono::NaiveDate::from_ymd_opt(2026, 8, 5).unwrap(),
            dimensions,
        };
        let mut invoice = CustomerInvoice::create_from_billing(
            CustomerInvoiceId::try_from_uuid(id(48)).unwrap(),
            scope.clone(),
            "INV-48".into(),
            source.clone(),
            billing,
            chrono::NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(),
            "preparer".into(),
            at,
        )
        .unwrap();
        assert!(
            published_events(&invoice.take_uncommitted_events())
                .unwrap()
                .is_empty()
        );
        let command_id = CommandId::try_from_uuid(id(100)).unwrap();
        let decision = HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "controller".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(id(3)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            command_id.to_string(),
            at,
        )
        .unwrap();
        invoice
            .post_with_decision(decision, command_id, &source, at)
            .unwrap();

        let events = published_events(&invoice.take_uncommitted_events()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), "finance.customer_invoice_posted");
        let finance::Event::CustomerInvoicePosted(fact) = &events[0] else {
            panic!("expected customer invoice fact")
        };
        assert_eq!(fact.source, source);
        assert_eq!(fact.posted_at, at);
        assert_eq!(fact.receivable_amount.minor_units(), 535);
    }
}
