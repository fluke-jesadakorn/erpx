use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, MoneyAmount, ReferenceCode};

owned_uuid_reference!(EmployeeRef);
owned_uuid_reference!(PayrollRunRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmployeeStatusFact {
    pub employee: EmployeeRef,
    pub employee_code: ReferenceCode,
    pub display_name: DisplayName,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollFinalizationFact {
    pub payroll_run: PayrollRunRef,
    pub period_code: ReferenceCode,
    pub finalized_at: DateTime<Utc>,
    pub gross_amount: MoneyAmount,
    pub deduction_amount: MoneyAmount,
    pub net_amount: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    EmployeeStatusChanged(EmployeeStatusFact),
    PayrollFinalized(PayrollFinalizationFact),
}

impl_integration_event!(Event, BoundedContext::PeoplePayroll, {
    Self::EmployeeStatusChanged(_) => "people_payroll.employee_status_changed",
    Self::PayrollFinalized(_) => "people_payroll.payroll_finalized",
});
