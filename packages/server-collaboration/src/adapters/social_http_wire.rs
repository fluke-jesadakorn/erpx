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
        "social.post.report" => Change::ReportPost {
            post_id: post(post_id)?,
            reason: text(raw, "reason")?.to_owned(),
        },
        "social.post.edit" => Change::EditPost {
            id: post(post_id)?,
            expected_version: positive(raw, "expectedVersion")?,
            body: raw
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            mentions: mentions(raw)?,
            content_json: raw
                .get("content")
                .filter(|value| !value.is_null())
                .map(serde_json::to_string)
                .transpose()
                .map_err(|_| invalid())?,
            rich_content_json: raw
                .get("richContent")
                .filter(|value| !value.is_null())
                .map(serde_json::to_string)
                .transpose()
                .map_err(|_| invalid())?,
        },
        "social.post.retract" => Change::RetractPost {
            id: post(post_id)?,
            expected_version: positive(raw, "expectedVersion")?,
        },
        "social.draft.publish" => Change::PublishDraft {
            id: post(post_id)?,
            version: positive(raw, "version")?,
        },
        "social.draft.save" => {            let mut audience_kind = None;
            let mut audience_id = None;
            let mut participants = Vec::new();
            match raw.get("audience") {
                None | Some(Value::Null) => {
                    match raw.get("branchId") {
                        Some(Value::Null) => audience_kind = Some("company".to_owned()),
                        Some(Value::String(value)) if !value.is_empty() => {
                            audience_kind = Some("branch".to_owned());
                            audience_id = Some(value.clone());
                        }
                        None => {}
                        _ => return Err(invalid()),
                    }
                }
                Some(Value::Object(audience)) => {
                    let kind = audience
                        .get("kind")
                        .and_then(Value::as_str)
                        .ok_or_else(invalid)?;
                    match kind {
                        "company" => audience_kind = Some("company".to_owned()),
                        "branch" | "department" | "team" | "project" => {
                            audience_kind = Some(kind.to_owned());
                            audience_id = Some(
                                audience
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .filter(|value| !value.is_empty())
                                    .ok_or_else(invalid)?
                                    .to_owned(),
                            );
                        }
                        "participants" => {
                            audience_kind = Some("participants".to_owned());
                            participants = audience
                                .get("userIds")
                                .and_then(Value::as_array)
                                .ok_or_else(invalid)?
                                .iter()
                                .map(|value| {
                                    value
                                        .as_str()
                                        .filter(|value| !value.is_empty())
                                        .map(str::to_owned)
                                        .ok_or_else(invalid)
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                        }
                        _ => return Err(invalid()),
                    }
                }
                _ => return Err(invalid()),
            }
            let sources = match raw.get("sources") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(items)) if items.len() <= 5 => items.clone(),
                _ => return Err(invalid()),
            };
            let purpose = optional_text(raw, "purpose")?.unwrap_or("update");
            if !matches!(purpose, "update" | "plan" | "blocker" | "handover" | "announcement") {
                return Err(invalid());
            }
            let expected_version = match raw.get("expectedVersion") {
                None | Some(Value::Null) => 1,
                Some(value) => value.as_i64().ok_or_else(invalid)?,
            };
            Change::SavePostDraft {
                id: optional_text(raw, "id")?.map(str::to_owned),
                body: raw
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                audience_kind,
                audience_id,
                participants,
                sources_json: serde_json::to_string(&sources).map_err(|_| invalid())?,
                purpose: purpose.to_owned(),
                mentions: mentions(raw)?,
                content_json: raw
                    .get("content")
                    .filter(|value| !value.is_null())
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|_| invalid())?,
                rich_content_json: raw
                    .get("richContent")
                    .filter(|value| !value.is_null())
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|_| invalid())?,
                expected_version,
            }
        }
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
