//! Finance-owned generic Money snapshot evidence for cross-owner consumers.
pub use crate::adapters::money_snapshot::{
    MoneyRateCandidate, MoneyRateSelector, MoneySnapshotEvidence, MoneySnapshotInput,
    PgMoneySnapshot, PgMoneySnapshotProducer, PreparedMoneySnapshot, RetainedMoneySnapshot,
    SelectedMoneyRate, lock_money_snapshot_sources, money_snapshot_scale, money_snapshot_usd,
    prepare_money_snapshot, require_money_snapshot_scale, validate_money_snapshot_input,
};
