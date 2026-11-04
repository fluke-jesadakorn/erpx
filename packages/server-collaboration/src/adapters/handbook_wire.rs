//! HTTP/command JSON normalization for the native Handbook surface.
//! This adapter grants no authority; the caller resolves current IAM/purchase.
use crate::domain::handbook::{
    ArticleAction, Category, Change, article_body, article_title, validate_editor_user,
    validate_expected_version,
};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};

pub struct DecodedHandbookCommand {
    pub change: Change,
    pub input: Value,
}

fn invalid() -> BusinessError {
    BusinessError::invalid("HANDBOOK_INPUT_INVALID", "Provide a valid handbook value.")
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, BusinessError> {
    value.get(key).and_then(Value::as_str).ok_or_else(invalid)
}

fn optional_string<'a>(
    value: &'a Value,
    key: &str,
    default: &'a str,
) -> Result<&'a str, BusinessError> {
    match value.get(key) {
        None => Ok(default),
        Some(Value::String(value)) => Ok(value),
        _ => Err(invalid()),
    }
}

fn positive(value: &Value, key: &str) -> Result<i64, BusinessError> {
    let number = value.get(key).and_then(Value::as_f64).ok_or_else(invalid)?;
    if !number.is_finite()
        || number.fract() != 0.0
        || !(1.0..=9_007_199_254_740_991.0).contains(&number)
    {
        return Err(invalid());
    }
    Ok(number as i64)
}

fn non_negative(value: &Value, key: &str) -> Result<i64, BusinessError> {
    let number = value.get(key).and_then(Value::as_f64).ok_or_else(invalid)?;
    if !number.is_finite()
        || number.fract() != 0.0
        || !(0.0..=9_007_199_254_740_991.0).contains(&number)
    {
        return Err(invalid());
    }
    Ok(number as i64)
}

fn article_fields(raw: &Value) -> Result<(String, Category, String), BusinessError> {
    let title = article_title(optional_string(raw, "title", "")?)?;
    let category = Category::parse(optional_string(raw, "category", "procedure")?)?;
    let body = article_body(optional_string(raw, "body", "")?)?;
    Ok((title, category, body))
}

/// `operation` is selected by the registered route; `id` comes from its path.
/// Unknown JSON fields are discarded, matching the existing object schemas.
pub fn decode_handbook_command(
    operation: &str,
    id: Option<&str>,
    raw: &Value,
) -> Result<DecodedHandbookCommand, BusinessError> {
    if !raw.is_object() {
        return Err(invalid());
    }
    match operation {
        "handbook.article.create" => {
            if id.is_some_and(|value| !value.is_empty()) {
                return Err(invalid());
            }
            let (title, category, body) = article_fields(raw)?;
            let expected_version = raw
                .get("expectedVersion")
                .map(|_| positive(raw, "expectedVersion"))
                .transpose()?
                .unwrap_or(1);
            Ok(DecodedHandbookCommand {
                change: Change::CreateArticle {
                    title: title.clone(),
                    category,
                    body: body.clone(),
                },
                input: json!({
                    "title": title,
                    "category": category.as_str(),
                    "body": body,
                    "expectedVersion": expected_version
                }),
            })
        }
        "handbook.article.update" => {
            let id = id.filter(|value| !value.is_empty()).ok_or_else(invalid)?;
            let (title, category, body) = article_fields(raw)?;
            let expected_version = positive(raw, "expectedVersion")?;
            Ok(DecodedHandbookCommand {
                change: Change::UpdateArticle {
                    id: id.into(),
                    expected_version,
                    title: title.clone(),
                    category,
                    body: body.clone(),
                },
                input: json!({
                    "title": title,
                    "category": category.as_str(),
                    "body": body,
                    "expectedVersion": expected_version
                }),
            })
        }
        "handbook.article.publish" | "handbook.article.archive" | "handbook.article.restore" => {
            let id = id.filter(|value| !value.is_empty()).ok_or_else(invalid)?;
            let action = string(raw, "action")?;
            if operation != format!("handbook.article.{action}") {
                return Err(invalid());
            }
            let action = ArticleAction::parse(action)?;
            let expected_version = positive(raw, "expectedVersion")?;
            Ok(DecodedHandbookCommand {
                change: Change::ArticleAction {
                    id: id.into(),
                    expected_version,
                    action,
                },
                input: json!({
                    "action": action.as_str(),
                    "expectedVersion": expected_version
                }),
            })
        }
        "handbook.editor.update" => {
            if id.is_some_and(|value| !value.is_empty()) {
                return Err(invalid());
            }
            let user_id = string(raw, "userId")?.to_owned();
            validate_editor_user(&user_id)?;
            let active = raw
                .get("active")
                .and_then(Value::as_bool)
                .ok_or_else(invalid)?;
            let expected_version = non_negative(raw, "expectedVersion")?;
            validate_expected_version(expected_version, true)?;
            Ok(DecodedHandbookCommand {
                change: Change::SetEditor {
                    user_id: user_id.clone(),
                    active,
                    expected_version,
                },
                input: json!({
                    "userId": user_id,
                    "active": active,
                    "expectedVersion": expected_version
                }),
            })
        }
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_normalization_matches_the_original_defaults_and_trim() {
        let decoded = decode_handbook_command(
            "handbook.article.create",
            None,
            &json!({"title":"  Policy  ","body":" body "}),
        )
        .unwrap();
        assert_eq!(
            decoded.input,
            json!({
                "title":"Policy",
                "category":"procedure",
                "body":" body ",
                "expectedVersion":1
            })
        );
        assert!(matches!(
            decoded.change,
            Change::CreateArticle {
                category: Category::Procedure,
                ..
            }
        ));
    }

    #[test]
    fn action_must_match_the_route_and_editor_version_can_be_zero() {
        assert!(
            decode_handbook_command(
                "handbook.article.publish",
                Some("article-1"),
                &json!({"action":"archive","expectedVersion":1}),
            )
            .is_err()
        );
        let decoded = decode_handbook_command(
            "handbook.editor.update",
            None,
            &json!({"userId":"user","active":true,"expectedVersion":0}),
        )
        .unwrap();
        assert!(matches!(
            decoded.change,
            Change::SetEditor {
                expected_version: 0,
                ..
            }
        ));
    }

    #[test]
    fn unknown_fields_are_discarded_and_bad_versions_fail() {
        let decoded = decode_handbook_command(
            "handbook.article.update",
            Some("article-1"),
            &json!({"title":"Policy","body":"Body","expectedVersion":3,"unknown":"ignored"}),
        )
        .unwrap();
        assert_eq!(decoded.input.get("unknown"), None);
        assert!(
            decode_handbook_command(
                "handbook.article.update",
                Some("article-1"),
                &json!({"title":"Policy","body":"Body","expectedVersion":0}),
            )
            .is_err()
        );
    }
}
