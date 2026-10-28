//! Original Field Service updated notification data, never owner authority.
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, TimestampMillis,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceNotificationFacts {
    pub owner_key: CommandKey,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub order_id: String,
    pub technician_user_id: String,
    pub order_version_after: i64,
    pub updated_at_after: TimestampMillis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceNotification {
    pub(crate) facts: FieldServiceNotificationFacts,
    pub(crate) id: String,
}
impl FieldServiceNotification {
    pub fn facts(&self) -> &FieldServiceNotificationFacts {
        &self.facts
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn message(&self) -> &'static str {
        "Field service order updated: review current next action"
    }
    pub fn event_id(&self) -> String {
        format!("legacy:{}", self.id)
    }
}
pub fn derive_field_service_notification(
    facts: &FieldServiceNotificationFacts,
    key: &CommandKey,
) -> Result<FieldServiceNotification, BusinessError> {
    if &facts.owner_key != key
        || !(2..=9_007_199_254_740_992).contains(&facts.order_version_after)
        || !(0..=9_007_199_254_740_991).contains(&facts.updated_at_after.get())
    {
        return Err(BusinessError::invalid(
            "FIELD_SERVICE_NOTIFICATION_INPUT_INVALID",
            "Use the original bounded owner command and intended order after-image.",
        ));
    }
    // The valid max-length outer key derives 201 bytes; it is not a new RecordId.
    Ok(FieldServiceNotification {
        facts: facts.clone(),
        id: format!("service-order-notice-{}", key.as_str()),
    })
}
