//! Sandbox email delivery reservation use case. The caller owns the Control
//! transaction; this layer validates the bounded input and delegates the single
//! idempotent append to the owner port.
use crate::domain::email_delivery::EmailDeliveryInput;
use erpx_server_shared::public::BusinessError;
use serde_json::json;
use std::future::Future;

pub trait EmailDeliveryRepository: Send {
    type Error: From<BusinessError> + Send;

    fn reserve<'a>(
        &'a mut self,
        input: &'a EmailDeliveryInput,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

pub async fn reserve_email_delivery<R: EmailDeliveryRepository>(
    repository: &mut R,
    input: &EmailDeliveryInput,
) -> Result<serde_json::Value, R::Error> {
    input.validate().map_err(R::Error::from)?;
    repository.reserve(input).await?;
    Ok(json!({
        "deliveryKey": input.delivery_key,
        "providerId": input.provider_id,
        "createdAt": input.created_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_maps_receipt() {
        let input = EmailDeliveryInput {
            id: "email-1".into(),
            delivery_key: "key-1".into(),
            recipient: "to@example.test".into(),
            subject: "S".into(),
            body_text: "body".into(),
            body_ciphertext: None,
            provider_id: "sandbox-key-1".into(),
            created_at: 1,
            metadata_json: "{}".into(),
        };
        assert!(input.validate().is_ok());
    }
}
