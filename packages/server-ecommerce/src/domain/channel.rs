//! Native Ecommerce channel-draft command facts.
//!
//! A draft channel is an owner-local record. This command does not publish the
//! channel, create a listing, open a cart, reserve Inventory, create a Sales
//! order or post Finance. Review is a separate independent-approval command.
use super::actor::EcommerceActor;
use super::command::{
    CONFIGURATION_LIMIT, CommandReceipt, EcommerceReceipt, command_identity, idempotency_conflict,
    js_trim, parse_receipt_result, readback_invalid, request_invalid, source_invalid, utf16_len,
    valid_configuration, valid_hash, valid_text,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use serde::Serialize;
use serde_json::Value;

pub const OPERATION: &str = "ecommerce.channel.create";
const NAME_MIN: usize = 2;
const NAME_MAX: usize = 160;

#[derive(Clone, Debug)]
pub struct CreateChannel {
    pub key: CommandKey,
    pub name: String,
    pub channel_type: String,
    pub configuration: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedChannel {
    pub name: String,
    pub channel_type: String,
    pub configuration_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelDraft {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub configuration_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelReadback {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub channel_type: String,
    pub configuration_json: String,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

fn valid_channel_type(value: &str) -> bool {
    matches!(value, "web" | "marketplace" | "social")
}

impl CreateChannel {
    pub fn validate(&self) -> Result<ValidatedChannel, BusinessError> {
        let name = js_trim(&self.name);
        let name_len = utf16_len(name);
        if !(NAME_MIN..=NAME_MAX).contains(&name_len)
            || !valid_text(name, NAME_MAX)
            || !valid_channel_type(&self.channel_type)
            || !self.configuration.is_object()
        {
            return Err(request_invalid());
        }
        let configuration_json =
            serde_json::to_string(&self.configuration).map_err(|_| request_invalid())?;
        if configuration_json.len() > CONFIGURATION_LIMIT
            || !valid_configuration(&configuration_json)
        {
            return Err(request_invalid());
        }
        Ok(ValidatedChannel {
            name: name.to_owned(),
            channel_type: self.channel_type.clone(),
            configuration_json,
        })
    }

    pub fn draft(&self, channel: ValidatedChannel) -> ChannelDraft {
        ChannelDraft {
            id: format!("ecommerce-channel-{}", self.key.as_str()),
            name: channel.name,
            channel_type: channel.channel_type,
            configuration_json: channel.configuration_json,
        }
    }

    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

#[derive(Serialize)]
struct IdentityInput<'a> {
    id: &'a str,
    name: &'a str,
    #[serde(rename = "channelType")]
    channel_type: &'a str,
    configuration: Value,
}

impl ChannelDraft {
    pub fn identity(&self, actor: &EcommerceActor) -> String {
        command_identity(
            actor,
            OPERATION,
            &IdentityInput {
                id: &self.id,
                name: &self.name,
                channel_type: &self.channel_type,
                configuration: serde_json::from_str::<Value>(&self.configuration_json)
                    .unwrap_or(Value::Null),
            },
        )
    }

    pub fn validate_readback(
        &self,
        actor: &EcommerceActor,
        channel: &ChannelReadback,
    ) -> Result<(), BusinessError> {
        if channel.id != self.id
            || channel.company_id != actor.company_id.as_str()
            || channel.created_by != actor.user_id.as_str()
            || channel.name != self.name
            || channel.channel_type != self.channel_type
            || channel.configuration_json != self.configuration_json
            || channel.status != "draft"
            || channel.reviewed_by.is_some()
            || channel.version != 1
            || channel.created_at < 0
            || channel.updated_at != channel.created_at
        {
            return Err(readback_invalid());
        }
        Ok(())
    }
}

pub fn replay(
    receipt: Option<CommandReceipt>,
    request_hash: &str,
    id: &str,
) -> Result<Option<EcommerceReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    let parsed = parse_receipt_result(&receipt.result_json)?;
    if parsed.id != id || parsed.status != "draft" || parsed.version != 1 {
        return Err(source_invalid());
    }
    Ok(Some(parsed))
}

pub fn new_receipt(id: &str) -> EcommerceReceipt {
    EcommerceReceipt {
        id: id.to_owned(),
        status: "draft".into(),
        version: 1,
    }
}

pub fn validate_hash(value: &str) -> Result<(), BusinessError> {
    if !valid_hash(value) {
        return Err(source_invalid());
    }
    Ok(())
}
