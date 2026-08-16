use erpx_integration_contracts::{
    CurrencyCode, DisplayName, MoneyAmount, ReferenceCode, people_payroll,
};

use crate::domain::{DomainEvent, EmployeeStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidContractValue,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<people_payroll::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(
    event: &DomainEvent,
) -> Option<Result<people_payroll::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::EmployeeStatusChanged {
            employee_id,
            employee_code,
            display_name,
            status,
            at,
        } => {
            let _ = at;
            Some(
                people_payroll::EmployeeRef::try_from_uuid(employee_id.as_uuid())
                    .and_then(|employee| {
                        Ok(people_payroll::EmployeeStatusFact {
                            employee,
                            employee_code: ReferenceCode::try_new(employee_code.clone())?,
                            display_name: DisplayName::try_new(display_name.clone())?,
                            active: matches!(
                                status,
                                EmployeeStatus::Active | EmployeeStatus::OnLeave
                            ),
                        })
                    })
                    .map(people_payroll::Event::EmployeeStatusChanged)
                    .map_err(|_| IntegrationMappingError::InvalidContractValue),
            )
        }
        DomainEvent::PayrollFinalized {
            payroll_run_id,
            period_code,
            currency,
            total_gross_minor,
            total_deductions_minor,
            total_net_minor,
            at,
        } => Some(
            people_payroll::PayrollRunRef::try_from_uuid(payroll_run_id.as_uuid())
                .and_then(|payroll_run| {
                    let currency = CurrencyCode::try_new(currency.as_str())?;
                    Ok(people_payroll::PayrollFinalizationFact {
                        payroll_run,
                        period_code: ReferenceCode::try_new(period_code.clone())?,
                        finalized_at: *at,
                        gross_amount: MoneyAmount::new(*total_gross_minor, currency),
                        deduction_amount: MoneyAmount::new(*total_deductions_minor, currency),
                        net_amount: MoneyAmount::new(*total_net_minor, currency),
                    })
                })
                .map(people_payroll::Event::PayrollFinalized)
                .map_err(|_| IntegrationMappingError::InvalidContractValue),
        ),
        DomainEvent::LeaveStatusChanged {
            leave_request_id,
            status,
            at,
        } => {
            let _ = (leave_request_id, status, at);
            None
        }
        DomainEvent::ExpenseStatusChanged {
            expense_claim_id,
            status,
            at,
        } => {
            let _ = (expense_claim_id, status, at);
            None
        }
        DomainEvent::PayrollStatusChanged {
            payroll_run_id,
            status,
            at,
        } => {
            let _ = (payroll_run_id, status, at);
            None
        }
        DomainEvent::PayrollEvidencePinned => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{IntegrationEvent, people_payroll::Event};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{Currency, EmployeeId, PayrollRunId};

    #[test]
    fn finance_receives_only_a_reconciled_finalized_payroll_fact() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let event = DomainEvent::PayrollFinalized {
            payroll_run_id: PayrollRunId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            period_code: "2026-08".into(),
            currency: Currency::try_new("THB").unwrap(),
            total_gross_minor: 10_000,
            total_deductions_minor: 1_000,
            total_net_minor: 9_000,
            at,
        };
        let mapped = published_events(&[event]).unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [Event::PayrollFinalized(fact)]
                if fact.net_amount.minor_units() == 9_000 && fact.finalized_at == at
        ));
        assert!(mapped[0].validate().is_ok());
    }

    #[test]
    fn inactive_employee_fact_does_not_expose_the_employee_aggregate() {
        let mapped = published_events(&[DomainEvent::EmployeeStatusChanged {
            employee_id: EmployeeId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            employee_code: "EMP-002".into(),
            display_name: "Ari".into(),
            status: EmployeeStatus::Terminated,
            at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        }])
        .unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [Event::EmployeeStatusChanged(fact)] if !fact.active
        ));
    }
}
