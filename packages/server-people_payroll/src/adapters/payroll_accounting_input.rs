//! Lexical outer request reader. Snapshot and result documents remain untouched.
use crate::domain::{
    payroll_accounting::{
        PayrollProjectionAccounts, PayrollProjectionItem, PayrollProjectionOutcome,
    },
    payroll_accounting_operand::{Fault, Operand, R},
};
fn read(
    items_json: &str,
    accounts_json: &str,
) -> R<(Vec<PayrollProjectionItem>, PayrollProjectionAccounts)> {
    let items = Operand::raw(items_json)?
        .array()?
        .ok_or_else(|| Fault::gap("P1", "request", "items"))?;
    let accounts = Operand::raw(accounts_json)?;
    if !accounts.token().is_some_and(|s| s.starts_with('{')) {
        return Err(Fault::gap("P1", "request", "accounts"));
    }
    Ok((
        items.into_iter().map(PayrollProjectionItem).collect(),
        PayrollProjectionAccounts(accounts),
    ))
}
pub fn read_payroll_projection_inputs(
    items_json: &str,
    accounts_json: &str,
) -> PayrollProjectionOutcome<(Vec<PayrollProjectionItem>, PayrollProjectionAccounts)> {
    read(items_json, accounts_json).into()
}
