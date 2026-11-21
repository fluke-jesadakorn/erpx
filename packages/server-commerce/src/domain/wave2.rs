//! Pure domain planner for the native Commerce wave2 warehouse-master write.
//!
//! No IO, no transaction and no SQL lives here: the application turns the typed
//! plan into a repository descriptor and the owner adapter supplies the only
//! SQL. The retained TypeScript adapter no longer writes these rows itself.
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

pub const WAREHOUSE_SAVE_OPERATION: &str = "commerce.wave2.warehouse.save";
pub const WAREHOUSE_RECORD_TYPE: &str = "commerce_warehouses";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WarehouseSaveInput {
    /// Existing warehouse id for an update; `None` creates a new record.
    pub id: Option<String>,
    pub record: String,
    pub company_id: String,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub status: String,
    pub now: i64,
    pub actor_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarehouseSavePlan {
    Create,
    Update,
}

impl WarehouseSaveInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let id = match raw.get("id") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if !value.is_empty() && value.len() <= 200 => {
                Some(value.clone())
            }
            _ => return Err(invalid()),
        };
        let status = text(raw, "status")?;
        if status != "active" && status != "archived" {
            return Err(invalid());
        }
        Ok(Self {
            id,
            record: text(raw, "record")?,
            company_id: text(raw, "companyId")?,
            branch_id: text(raw, "branchId")?,
            code: text(raw, "code")?,
            name: text(raw, "name")?,
            status,
            now: match raw.get("now") {
                Some(Value::Number(number)) => number.as_i64().ok_or_else(invalid)?,
                _ => return Err(invalid()),
            },
            actor_id: text(raw, "actorId")?,
        })
    }

    pub fn plan(&self) -> WarehouseSavePlan {
        if self.id.is_some() {
            WarehouseSavePlan::Update
        } else {
            WarehouseSavePlan::Create
        }
    }

    pub fn record_id(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.record)
    }
}

fn text(raw: &Value, field: &str) -> Result<String, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= 200 => Ok(value.clone()),
        _ => Err(invalid()),
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_WAREHOUSE_SAVE_INVALID",
        "Review the warehouse master write.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "COMMERCE_WAVE2_UNAVAILABLE",
        "The native Commerce service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw() -> Value {
        json!({"id":null,"record":"warehouse-k","companyId":"c","branchId":"b","code":"WH1","name":"Main","status":"active","now":1000,"actorId":"u"})
    }

    #[test]
    fn plans_create_and_update() {
        let create = WarehouseSaveInput::parse(&raw()).unwrap();
        assert_eq!(create.plan(), WarehouseSavePlan::Create);
        let mut value = raw();
        value["id"] = json!("warehouse-1");
        let update = WarehouseSaveInput::parse(&value).unwrap();
        assert_eq!(update.plan(), WarehouseSavePlan::Update);
        assert_eq!(update.record_id(), "warehouse-1");
    }

    #[test]
    fn rejects_invalid_status() {
        let mut value = raw();
        value["status"] = json!("bogus");
        assert!(WarehouseSaveInput::parse(&value).is_err());
    }
}
