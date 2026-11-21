//! Pure validation for the native sandbox email delivery reservation. No IO,
//! no SQL and no encryption happen here; the caller supplies bounded facts.
use erpx_server_shared::public::BusinessError;

pub const MAX_FIELD: usize = 4_096;
pub const MAX_BODY: usize = 128 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmailDeliveryInput {
    pub id: String,
    pub delivery_key: String,
    pub recipient: String,
    pub subject: String,
    pub body_text: String,
    pub body_ciphertext: Option<String>,
    pub provider_id: String,
    pub created_at: i64,
    pub metadata_json: String,
}

impl EmailDeliveryInput {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let bounded = |value: &str, max: usize| !value.trim().is_empty() && value.len() <= max;
        let valid = bounded(&self.id, MAX_FIELD)
            && bounded(&self.delivery_key, MAX_FIELD)
            && bounded(&self.recipient, 320)
            && !self.subject.is_empty()
            && self.subject.len() <= MAX_FIELD
            && self.body_text.len() <= MAX_BODY
            && self.body_ciphertext.as_deref().is_none_or(|value| {
                !value.is_empty() && value.len() <= MAX_BODY && value.starts_with("v1.")
            })
            && bounded(&self.provider_id, MAX_FIELD)
            && self.created_at > 0
            && self.metadata_json.starts_with('{')
            && self.metadata_json.len() <= MAX_FIELD;
        if valid {
            Ok(())
        } else {
            Err(BusinessError::invalid(
                "EMAIL_DELIVERY_INPUT_INVALID",
                "Provide bounded sandbox email delivery facts.",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> EmailDeliveryInput {
        EmailDeliveryInput {
            id: "email-1".into(),
            delivery_key: "key-1".into(),
            recipient: "to@example.test".into(),
            subject: "Subject".into(),
            body_text: "body".into(),
            body_ciphertext: None,
            provider_id: "sandbox-key-1".into(),
            created_at: 1,
            metadata_json: "{\"adapter\":\"sandbox\"}".into(),
        }
    }

    #[test]
    fn accepts_bounded_delivery() {
        assert!(input().validate().is_ok());
        let mut encrypted = input();
        encrypted.body_ciphertext = Some("v1.abc.def.xyz".into());
        assert!(encrypted.validate().is_ok());
    }

    #[test]
    fn rejects_bad_key_empty_recipient_and_bad_ciphertext() {
        let mut bad = input();
        bad.delivery_key.clear();
        assert!(bad.validate().is_err());
        let mut bad = input();
        bad.recipient.clear();
        assert!(bad.validate().is_err());
        let mut bad = input();
        bad.body_ciphertext = Some("garbage".into());
        assert!(bad.validate().is_err());
    }
}
