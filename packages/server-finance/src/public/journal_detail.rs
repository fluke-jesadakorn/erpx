//! Read-only journal wire data. These serialized values are never authority.
use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[cfg_attr(feature="contracts",schema(value_type=i64,minimum=-9_007_199_254_740_991_i64,maximum=9_007_199_254_740_991_i64))]
pub struct FinanceReadInteger(#[cfg_attr(feature = "contracts", ts(type = "number"))] i64);
impl FinanceReadInteger {
    pub fn new(value: i64) -> Result<Self, BusinessError> {
        if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value) {
            return Err(BusinessError::invalid(
                "FINANCE_READ_INTEGER_UNSAFE",
                "The journal contains an unsupported integer.",
            ));
        }
        Ok(Self(value))
    }
    pub fn get(self) -> i64 {
        self.0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FinanceLedgerValue {
    Text(String),
    Integer(FinanceReadInteger),
    Null,
}
pub type FinanceLedgerRow = BTreeMap<String, FinanceLedgerValue>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceJournalDetail {
    pub journal: FinanceLedgerRow,
    pub lines: Vec<FinanceLedgerRow>,
    pub history: Vec<FinanceLedgerRow>,
}
pub use crate::adapters::postgres_journal_detail::{
    CurrentJournalDetail, lock_journal_detail_sources, read_journal_detail,
};
