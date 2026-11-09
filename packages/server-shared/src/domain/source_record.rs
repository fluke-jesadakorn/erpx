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
        if version == Some(0) || id.as_str().encode_utf16().count() > 180 {
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

// Portable record-source facts exchanged through owner-owned ports.
impl SourceRecordKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "commerce_order" => Ok(Self::CommerceOrder),
            "support_ticket" => Ok(Self::SupportTicket),
            "field_service_order" => Ok(Self::FieldServiceOrder),
            "staff_task" => Ok(Self::StaffTask),
            "pos_sale" => Ok(Self::PosSale),
            "pms_record" => Ok(Self::PmsRecord),
            "asset" => Ok(Self::Asset),
            "maintenance_plan" => Ok(Self::MaintenancePlan),
            "maintenance_work" => Ok(Self::MaintenanceWork),
            _ => Err(BusinessError::invalid(
                "SOURCE_RECORD_INVALID",
                "Unsupported source record.",
            )),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CommerceOrder => "commerce_order",
            Self::SupportTicket => "support_ticket",
            Self::FieldServiceOrder => "field_service_order",
            Self::StaffTask => "staff_task",
            Self::PosSale => "pos_sale",
            Self::PmsRecord => "pms_record",
            Self::Asset => "asset",
            Self::MaintenancePlan => "maintenance_plan",
            Self::MaintenanceWork => "maintenance_work",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordSourceReference {
    pub kind: SourceRecordKind,
    pub id: String,
    pub expected_version: Option<i64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordSourceSnapshot {
    pub kind: SourceRecordKind,
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
    pub title: String,
    pub status: String,
}
