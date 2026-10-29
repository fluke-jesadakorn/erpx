//! Validated Commerce facts for creation of Service stock masters.
use erpx_server_shared::public::{CommandKey, CompanyId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceStockMaster {
    Warehouse {
        branch_id: String,
        code: String,
        name: String,
        status: String,
    },
    Product {
        sku: String,
        name: String,
        unit: String,
        tracking_mode: String,
        price_minor: i64,
        tax_bps: i64,
        status: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceStockMasterIntent {
    pub company: CompanyId,
    pub command_id: CommandKey,
    pub master: ServiceStockMaster,
}
impl ServiceStockMasterIntent {
    pub fn id(&self) -> String {
        let kind = match &self.master {
            ServiceStockMaster::Warehouse { .. } => "warehouses",
            ServiceStockMaster::Product { .. } => "products",
        };
        format!("{kind}-{}", self.command_id.as_str())
    }
    pub fn valid(&self) -> bool {
        fn sized(s: &str, min: usize, max: usize) -> bool {
            let n = s.encode_utf16().count();
            n >= min && n <= max
        }
        let valid = match &self.master {
            ServiceStockMaster::Warehouse {
                branch_id,
                code,
                name,
                status,
            } => {
                !branch_id.is_empty()
                    && sized(code, 2, 40)
                    && sized(name, 2, 160)
                    && matches!(status.as_str(), "active" | "archived")
            }
            ServiceStockMaster::Product {
                sku,
                name,
                unit,
                tracking_mode,
                price_minor,
                tax_bps,
                status,
            } => {
                sized(sku, 2, 40)
                    && sized(name, 2, 160)
                    && sized(unit, 1, 30)
                    && matches!(tracking_mode.as_str(), "none" | "lot" | "serial")
                    && (0..=9_007_199_254_740_991).contains(price_minor)
                    && (0..=10_000).contains(tax_bps)
                    && matches!(status.as_str(), "active" | "archived")
            }
        };
        let bytes = match &self.master {
            ServiceStockMaster::Warehouse {
                branch_id,
                code,
                name,
                status,
            } => branch_id
                .len()
                .saturating_add(code.len())
                .saturating_add(name.len())
                .saturating_add(status.len()),
            ServiceStockMaster::Product {
                sku,
                name,
                unit,
                tracking_mode,
                status,
                ..
            } => sku
                .len()
                .saturating_add(name.len())
                .saturating_add(unit.len())
                .saturating_add(tracking_mode.len())
                .saturating_add(status.len()),
        };
        valid
            && bytes
                .saturating_add(self.company.as_str().len())
                .saturating_add(self.command_id.as_str().len())
                <= 2 * 1024 * 1024
    }
}

/// Pure Service Stock discovery policy over independently witnessed facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceStockWorkspacePolicy {
    pub available: bool,
    pub can_create: bool,
    pub unavailable_reason: Option<&'static str>,
}
pub fn service_stock_workspace_policy(
    history: bool,
    new_work: bool,
    schema: bool,
    source: bool,
) -> ServiceStockWorkspacePolicy {
    if !history {
        return ServiceStockWorkspacePolicy {
            available: false,
            can_create: false,
            unavailable_reason: Some("purchase-required"),
        };
    }
    ServiceStockWorkspacePolicy {
        available: true,
        can_create: new_work && schema && source,
        unavailable_reason: if !schema {
            Some("schema-required")
        } else if !source {
            Some("service-posting-required")
        } else {
            None
        },
    }
}
