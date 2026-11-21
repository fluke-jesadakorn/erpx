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

fn handover_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || b.iter()
            .enumerate()
            .any(|(i, c)| i != 4 && i != 7 && !c.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<usize>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    (1..=12).contains(&month) && day > 0 && day <= lengths[month - 1]
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
    // The native comment command does not yet preserve rich documents, mention
    // ranges or uploaded images. Refuse these inputs until the owner can commit
    // them together; accepting them here would silently discard user content.
    if matches!(operation, "social.comment.publish" | "social.comment.edit")
        && (raw.get("richContent").is_some()
            || raw.get("commentDraftId").is_some_and(|v| !v.is_null())
            || raw
                .get("mentions")
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    items.iter().any(|item| {
                        ["label", "start", "end"]
                            .iter()
                            .any(|key| item.get(key).is_some())
                    })
                }))
    {
        return Err(BusinessError::invalid(
            "SOCIAL_COMMENT_RICH_UNAVAILABLE",
            "Rich comments and uploaded images are not yet available.",
        ));
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
        "social.story.publish" => Change::CreateStory {
            title: text(raw, "title")?.to_owned(),
            body: text(raw, "body")?.to_owned(),
            theme: optional_text(raw, "theme")?.unwrap_or("sage").to_owned(),
            audience: Audience::parse(optional_text(raw, "audience")?.unwrap_or("holding"))?,
        },
        "social.story.archive" => Change::ArchiveStory {
            id: post(post_id)?,
            expected_version: positive(raw, "expectedVersion")?,
        },
        "social.comment.publish" => Change::PublishComment {
            post_id: post(post_id)?,
            body: text(raw, "body")?.to_owned(),
            parent_comment_id: optional_text(raw, "parentCommentId")?.map(str::to_owned),
            mentions: mentions(raw)?,
            comment_upload_id: optional_text(raw, "commentDraftId")?.map(str::to_owned),
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
            edit_upload_id: optional_text(raw, "editDraftId")?.map(str::to_owned),
        },
        "social.handover.publish" => {
            if raw.as_object().is_none_or(|obj| {
                obj.keys()
                    .any(|k| !["date", "body", "sources", "recipientIds"].contains(&k.as_str()))
            }) {
                return Err(invalid());
            }
            let date = text(raw, "date")?;
            if !handover_date(date) {
                return Err(invalid());
            }
            let recipient_ids = strings(raw, "recipientIds")?;
            if recipient_ids.is_empty()
                || recipient_ids.len() > 20
                || recipient_ids
                    .iter()
                    .any(|id| id.is_empty() || id.encode_utf16().count() > 160)
                || recipient_ids
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != recipient_ids.len()
            {
                return Err(invalid());
            }
            let sources = match raw.get("sources") {
                Some(Value::Array(items)) if !items.is_empty() && items.len() <= 5 => items
                    .iter()
                    .map(|item| {
                        let kind = match item.get("kind") {
                            None => "staff_task",
                            Some(Value::String(kind))
                                if matches!(kind.as_str(), "staff_task" | "pms_record") =>
                            {
                                kind
                            }
                            _ => return Err(invalid()),
                        };
                        let id = text(item, "id")?;
                        if id.encode_utf16().count() > 160 {
                            return Err(invalid());
                        }
                        Ok(crate::domain::social::HandoverSource {
                            kind: kind.to_owned(),
                            id: text(item, "id")?.to_owned(),
                            source_version: positive(item, "sourceVersion")?,
                        })
                    })
                    .collect::<Result<Vec<_>, BusinessError>>()?,
                _ => return Err(invalid()),
            };
            if sources
                .iter()
                .map(|s| &s.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != sources.len()
            {
                return Err(invalid());
            }
            let body = text(raw, "body")?;
            let body = crate::domain::collaboration_work::js_trim(body);
            if body.is_empty() || body.encode_utf16().count() > 3000 {
                return Err(invalid());
            }
            Change::PublishHandoverPost {
                date: date.to_owned(),
                body: body.to_owned(),
                recipient_ids,
                sources,
            }
        }
        "social.post.hide" => Change::ModeratePost { id: post(post_id)? },
        "social.post.retract" => Change::RetractPost {
            id: post(post_id)?,
            expected_version: positive(raw, "expectedVersion")?,
        },
        "social.comment.upload.create" => Change::CommentUploadCreate {
            post_id: post(post_id)?,
        },
        "social.comment.upload.cancel" => Change::CommentUploadCancel { id: post(post_id)? },
        "social.post.edit-upload.create" => Change::PostEditUploadCreate {
            post_id: post(post_id)?,
            expected_version: positive(raw, "expectedVersion")?,
        },
        "social.post.edit-upload.cancel" => Change::PostEditUploadCancel { id: post(post_id)? },
        "social.draft.publish" => Change::PublishDraft {
            id: post(post_id)?,
            version: positive(raw, "version")?,
        },
        "social.draft.save" => {
            let mut audience_kind = None;
            let mut audience_id = None;
            let mut participants = Vec::new();
            match raw.get("audience") {
                None | Some(Value::Null) => match raw.get("branchId") {
                    Some(Value::Null) => audience_kind = Some("company".to_owned()),
                    Some(Value::String(value)) if !value.is_empty() => {
                        audience_kind = Some("branch".to_owned());
                        audience_id = Some(value.clone());
                    }
                    None => {}
                    _ => return Err(invalid()),
                },
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
            if !matches!(
                purpose,
                "update" | "plan" | "blocker" | "handover" | "announcement"
            ) {
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
    fn handover_wire_retains_date_and_each_selected_version_for_authority_and_replay() {
        let raw = json!({"date":"2024-02-29","body":"  Handover  ","recipientIds":["u-1"],"sources":[{"id":"task-1","sourceVersion":4},{"kind":"pms_record","id":"room-1","sourceVersion":8}]});
        let decoded = decode_social_command("social.handover.publish", None, None, &raw).unwrap();
        match &decoded.change {
            Change::PublishHandoverPost {
                date,
                body,
                sources,
                ..
            } => {
                assert_eq!(date, "2024-02-29");
                assert_eq!(body, "Handover");
                assert_eq!(
                    (&sources[0].kind, sources[0].source_version),
                    (&"staff_task".to_string(), 4)
                );
                assert_eq!(
                    (&sources[1].kind, sources[1].source_version),
                    (&"pms_record".to_string(), 8)
                );
            }
            other => panic!("unexpected change {other:?}"),
        }
        let original = crate::adapters::command_ledger::fingerprint(
            "social.handover.publish",
            &decoded.change,
        );
        for (field, value) in [("date", json!("2024-03-01")), ("version", json!(5))] {
            let mut altered = raw.clone();
            if field == "date" {
                altered["date"] = value;
            } else {
                altered["sources"][0]["sourceVersion"] = value;
            }
            let next =
                decode_social_command("social.handover.publish", None, None, &altered).unwrap();
            assert_ne!(
                original,
                crate::adapters::command_ledger::fingerprint(
                    "social.handover.publish",
                    &next.change
                )
            );
        }
    }

    #[test]
    fn handover_wire_rejects_missing_or_duplicate_source_authority_inputs() {
        let valid = json!({"date":"2024-02-29","body":"Handover","recipientIds":["u-1"],"sources":[{"id":"task-1","sourceVersion":4}]});
        for mutation in 0..7 {
            let mut raw = valid.clone();
            match mutation {
                0 => raw["date"] = json!("2023-02-29"),
                1 => {
                    raw["sources"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("sourceVersion");
                }
                2 => raw["sources"][0]["sourceVersion"] = json!(0),
                3 => raw["sources"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"task-1","sourceVersion":4,"kind":"pms_record"})),
                4 => raw["recipientIds"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("u-1")),
                5 => raw["sources"][0]["kind"] = json!("staff_project"),
                _ => raw["unexpected"] = json!(true),
            }
            assert!(
                decode_social_command("social.handover.publish", None, None, &raw).is_err(),
                "mutation {mutation}"
            );
        }
    }

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
    fn upload_draft_identifiers_are_normalized_on_comment_and_post_edit() {
        let decoded = decode_social_command(
            "social.comment.publish",
            Some("post-1"),
            None,
            &json!({
                "body":"Reply",
                "commentDraftId":"comment-upload-1",
                "mentions":[]
            }),
        )
        .unwrap_err();
        assert_eq!(decoded.code, "SOCIAL_COMMENT_RICH_UNAVAILABLE");
        let decoded = decode_social_command(
            "social.post.edit",
            Some("post-1"),
            None,
            &json!({
                "body":"Edited",
                "expectedVersion":2,
                "editDraftId":"post-edit-upload-1",
                "mentions":[]
            }),
        )
        .unwrap();
        assert!(matches!(
            decoded.change,
            Change::EditPost {
                edit_upload_id: Some(id),
                ..
            } if id == "post-edit-upload-1"
        ));
    }

    #[test]
    fn rich_comment_inputs_fail_before_content_is_discarded() {
        for (operation, comment, input) in [
            (
                "social.comment.publish",
                None,
                json!({"body":"hello","richContent":{"version":1,"blocks":[]}}),
            ),
            (
                "social.comment.edit",
                Some("comment-1"),
                json!({"body":"hello","expectedVersion":1,"mentions":[{"kind":"person","targetId":"u","label":"User","start":0,"end":4}]}),
            ),
        ] {
            let error =
                decode_social_command(operation, Some("post-1"), comment, &input).unwrap_err();
            assert_eq!(error.code, "SOCIAL_COMMENT_RICH_UNAVAILABLE");
        }
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
