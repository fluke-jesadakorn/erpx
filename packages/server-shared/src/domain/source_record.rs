use serde::{Deserialize, Serialize};

use super::{error::BusinessError, identity::RecordId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRecordKind {
    CommerceOrder,
    SupportTicket,
    FieldServiceOrder,
    StaffTask,
    PosSale,
    PmsRecord,
    Asset,
    MaintenancePlan,
    MaintenanceWork,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRecord {
    kind: SourceRecordKind,
    id: RecordId,
    version: Option<u64>,
}

impl SourceRecord {
    pub fn new(
        kind: SourceRecordKind,
        id: RecordId,
        version: Option<u64>,
    ) -> Result<Self, BusinessError> {
        if version == Some(0) || id.as_str().len() > 180 {
            return Err(BusinessError::invalid(
                "SOURCE_VERSION_REQUIRED",
                "Use a positive source version.",
            ));
        }
        Ok(Self { kind, id, version })
    }

    pub const fn kind(&self) -> SourceRecordKind {
        self.kind
    }
    pub fn id(&self) -> &RecordId {
        &self.id
    }
    pub const fn version(&self) -> Option<u64> {
        self.version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionLot {
    lot_id: RecordId,
    quantity_milli: u64,
    balance_version: u64,
}

impl ConsumptionLot {
    pub fn new(
        lot_id: RecordId,
        quantity_milli: u64,
        balance_version: u64,
    ) -> Result<Self, BusinessError> {
        if quantity_milli == 0 || quantity_milli > 9_007_199_254_740_991 || balance_version == 0 {
            return Err(BusinessError::invalid(
                "INVALID_CONSUMPTION_LOT",
                "Use a positive safe quantity and balance version.",
            ));
        }
        Ok(Self {
            lot_id,
            quantity_milli,
            balance_version,
        })
    }

    pub fn lot_id(&self) -> &RecordId {
        &self.lot_id
    }
    pub const fn quantity_milli(&self) -> u64 {
        self.quantity_milli
    }
    pub const fn balance_version(&self) -> u64 {
        self.balance_version
    }
}
