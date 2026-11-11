//! Social HTTP/command JSON normalization. This adapter grants no mutation
//! authority; the composition backend resolves the current actor and owner unit.
use crate::domain::social::{
    Audience, Change, FollowTarget, LinkedRecordInput, MentionInput, MentionKind,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

pub const MAX_EXPECTED_VERSION: i64 = 9_007_199_254_740_991;

#[derive(Debug)]
pub struct DecodedSocialCommand {
    pub change: Change,
    /// Normalized input retained for the Workflow idempotency fingerprint.
    pub input: Value,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "SOCIAL_INPUT_INVALID",
        "Provide a valid collaboration value.",
    )
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)
}

fn optional_text<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>, BusinessError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        _ => Err(invalid()),
    }
}

fn positive(value: &Value, key: &str) -> Result<i64, BusinessError> {
    let n = value.get(key).and_then(Value::as_f64).ok_or_else(invalid)?;
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=MAX_EXPECTED_VERSION as f64).contains(&n) {
        return Err(invalid());
    }
    Ok(n as i64)
}

fn boolean(value: &Value, key: &str) -> Result<bool, BusinessError> {
    value.get(key).and_then(Value::as_bool).ok_or_else(invalid)
}

fn strings(value: &Value, key: &str) -> Result<Vec<String>, BusinessError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(invalid))
            .collect(),
        _ => Err(invalid()),
    }
}

fn mentions(value: &Value) -> Result<Vec<MentionInput>, BusinessError> {
    match value.get("mentions") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                Ok(MentionInput {
                    kind: MentionKind::parse(text(item, "kind")?)?,
                    target_id: text(item, "targetId")?.to_owned(),
                })
            })
            .collect(),
        _ => Err(invalid()),
    }
}

fn linked_record(value: &Value) -> Result<Option<LinkedRecordInput>, BusinessError> {
    match value.get("linkedRecord") {
        None | Some(Value::Null) => Ok(None),
        Some(record) => Ok(Some(LinkedRecordInput {
            module: text(record, "module")?.to_owned(),
            id: text(record, "id")?.to_owned(),
        })),
    }
}

/// `operation` is selected by the registered route. Path identifiers are only
/// accepted for the operations that own them.
pub fn decode_social_command(
    operation: &str,
    post_id: Option<&str>,
    comment_id: Option<&str>,
    raw: &Value,
) -> Result<DecodedSocialCommand, BusinessError> {
    if !raw.is_object() {
        return Err(invalid());
    }
    let post = |value: Option<&str>| value.map(str::to_owned).ok_or_else(invalid);
    let change = match operation {
        "social.post.publish" => Change::PublishPost {
            body: text(raw, "body")?.to_owned(),
            audience: Audience::parse(text(raw, "audience")?)?,
            tags: strings(raw, "tags")?,
            mentions: mentions(raw)?,
            linked_record: linked_record(raw)?,
        },
        "social.comment.publish" => Change::PublishComment {
            post_id: post(post_id)?,
            body: text(raw, "body")?.to_owned(),
            parent_comment_id: optional_text(raw, "parentCommentId")?.map(str::to_owned),
            mentions: mentions(raw)?,
        },
        "social.comment.edit" => Change::EditComment {
            post_id: post(post_id)?,
            id: comment_id.map(str::to_owned).ok_or_else(invalid)?,
            expected_version: positive(raw, "expectedVersion")?,
            body: text(raw, "body")?.to_owned(),
            mentions: mentions(raw)?,
        },
        "social.comment.remove" => Change::TombstoneComment {
            post_id: post(post_id)?,
            id: comment_id.map(str::to_owned).ok_or_else(invalid)?,
            expected_version: positive(raw, "expectedVersion")?,
        },
        "social.post.appreciate" => Change::SetReaction {
            post_id: post(post_id)?,
            active: boolean(raw, "active")?,
        },
        "social.post.follow" => Change::SetFollow {
            target: FollowTarget::Post,
            target_id: post(post_id)?,
            active: boolean(raw, "active")?,
        },
        "social.topic.follow" => Change::SetFollow {
            target: FollowTarget::Topic,
            target_id: post(post_id)?,
            active: boolean(raw, "active")?,
        },
        _ => return Err(invalid()),
    };
    Ok(DecodedSocialCommand {
        change,
        input: raw.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn post_and_comment_payloads_normalize_without_granting_authority() {
        let decoded = decode_social_command(
            "social.post.publish",
            None,
            None,
            &json!({
                "body":" Hello #Ops ",
                "audience":"branch",
                "tags":["Policy"],
                "mentions":[{"kind":"person","targetId":"user-1"}],
                "linkedRecord":{"module":"people_payroll","id":"leave-1"}
            }),
        )
        .unwrap();
        match decoded.change {
            Change::PublishPost {
                body,
                audience,
                tags,
                mentions,
                linked_record,
            } => {
                assert_eq!(body, " Hello #Ops ");
                assert_eq!(audience, Audience::Branch);
                assert_eq!(tags, vec!["Policy".to_owned()]);
                assert_eq!(mentions[0].target_id, "user-1");
                assert_eq!(linked_record.unwrap().module, "people_payroll");
            }
            other => panic!("unexpected change {other:?}"),
        }
        let decoded = decode_social_command(
            "social.comment.edit",
            Some("post-1"),
            Some("comment-1"),
            &json!({"body":"Reply","expectedVersion":2}),
        )
        .unwrap();
        assert!(matches!(
            decoded.change,
            Change::EditComment {
                expected_version: 2,
                ..
            }
        ));
    }

    #[test]
    fn missing_or_malformed_fields_are_rejected_before_the_owner() {
        for (operation, post, comment, value) in [
            (
                "social.post.publish",
                None,
                None,
                json!({"body":"x","audience":"private"}),
            ),
            ("social.comment.publish", None, None, json!({"body":"x"})),
            (
                "social.comment.edit",
                Some("p"),
                Some("c"),
                json!({"body":"x"}),
            ),
            (
                "social.comment.remove",
                Some("p"),
                Some("c"),
                json!({"expectedVersion":0}),
            ),
            (
                "social.post.appreciate",
                Some("p"),
                None,
                json!({"active":"yes"}),
            ),
            ("bogus", None, None, json!({})),
        ] {
            assert_eq!(
                decode_social_command(operation, post, comment, &value)
                    .unwrap_err()
                    .code,
                "SOCIAL_INPUT_INVALID"
            );
        }
    }
}
