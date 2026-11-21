//! Native released Benefits payroll-input read port for People payroll.
pub use crate::adapters::payroll_source::PgBenefitsPayrollSource;
pub use crate::application::payroll_source::{
    BenefitsPayrollSourceRepository, payroll_benefit_inputs,
};
pub use crate::domain::payroll_source::{
    BenefitPayrollInputRow, BenefitsPayrollQuery, BenefitsPayrollScope, PAYROLL_SOURCE_LIMIT,
    released_payroll_count, released_payroll_inputs, released_payroll_matches,
};
