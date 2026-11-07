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
    pub(crate) created: bool,
}
impl FieldServiceNotification {
    pub fn facts(&self) -> &FieldServiceNotificationFacts {
        &self.facts
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn message(&self) -> &'static str {
        if self.created {
            "New field service order: review appointment and required context"
        } else {
            "Field service order updated: review current next action"
        }
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
        created: false,
    })
}

/// Creation has its own explicit owner path and exact initial version.
pub fn derive_field_service_created_notification(
    facts: &FieldServiceNotificationFacts,
    key: &CommandKey,
) -> Result<FieldServiceNotification, BusinessError> {
    if &facts.owner_key != key
        || facts.order_version_after != 1
        || !(0..=9_007_199_254_740_991).contains(&facts.updated_at_after.get())
    {
        return Err(BusinessError::invalid(
            "FIELD_SERVICE_NOTIFICATION_INPUT_INVALID",
            "Use the original bounded owner command and intended order after-image.",
        ));
    }
    Ok(FieldServiceNotification {
        facts: facts.clone(),
        id: format!("service-order-notice-{}", key.as_str()),
        created: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn facts(version: i64) -> FieldServiceNotificationFacts {
        FieldServiceNotificationFacts {
            owner_key: CommandKey::new("request-unit-key").unwrap(),
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            order_id: "order".into(),
            technician_user_id: "worker".into(),
            order_version_after: version,
            updated_at_after: TimestampMillis::new(100),
        }
    }
    #[test]
    fn creation_and_update_have_distinct_versioned_contracts() {
        let first = facts(1);
        let created = derive_field_service_created_notification(&first, &first.owner_key).unwrap();
        assert_eq!(
            created.message(),
            "New field service order: review appointment and required context"
        );
        assert!(derive_field_service_notification(&first, &first.owner_key).is_err());
        let later = facts(2);
        let updated = derive_field_service_notification(&later, &later.owner_key).unwrap();
        assert_eq!(
            updated.message(),
            "Field service order updated: review current next action"
        );
        assert!(derive_field_service_created_notification(&later, &later.owner_key).is_err());
    }
}
