//! Social rich-document validation. This derives facts only: it grants no
//! permission to mention a target or bind a Files attachment.
use erpx_platform_shared_server::public::BusinessError;
use serde::Serialize;
use serde_json::{Map, Value};
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RichMention {
    pub kind: String,
    pub target_id: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RichDocumentFacts {
    pub content: Value,
    pub body: String,
    pub mentions: Vec<RichMention>,
    pub image_attachment_ids: Vec<Option<String>>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid("RICH_CONTENT_INVALID", "Invalid rich content.")
}
fn utf16(value: &str) -> usize {
    value.encode_utf16().count()
}
fn object(value: &Value) -> Result<&Map<String, Value>, BusinessError> {
    value.as_object().ok_or_else(invalid)
}
fn field<'a>(obj: &'a Map<String, Value>, name: &str) -> Result<&'a str, BusinessError> {
    obj.get(name).and_then(Value::as_str).ok_or_else(invalid)
}
fn attrs(obj: &Map<String, Value>) -> Result<Option<&Map<String, Value>>, BusinessError> {
    match obj.get("attrs") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(object(value)?)),
    }
}
fn allowed(obj: &Map<String, Value>, names: &[&str]) -> Result<(), BusinessError> {
    if obj.keys().any(|key| !names.contains(&key.as_str())) {
        return Err(invalid());
    }
    Ok(())
}
fn int_range(value: Option<&Value>, min: i64, max: i64) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|v| v.is_finite() && v.fract() == 0.0 && v >= min as f64 && v <= max as f64)
}
fn optional_string_one_of(
    attrs: &Map<String, Value>,
    key: &str,
    permitted: &[&str],
) -> Result<(), BusinessError> {
    if let Some(value) = attrs.get(key).filter(|v| !v.is_null()) {
        if !value.as_str().is_some_and(|s| permitted.contains(&s)) {
            return Err(invalid());
        }
    }
    Ok(())
}
fn optional_range(
    attrs: &Map<String, Value>,
    key: &str,
    min: i64,
    max: i64,
) -> Result<(), BusinessError> {
    if let Some(value) = attrs.get(key).filter(|v| !v.is_null()) {
        if !int_range(Some(value), min, max) {
            return Err(invalid());
        }
    }
    Ok(())
}
fn color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}
fn check_color(attrs: &Map<String, Value>, key: &str) -> Result<(), BusinessError> {
    if let Some(value) = attrs.get(key).filter(|v| !v.is_null()) {
        if !value.as_str().is_some_and(color) {
            return Err(invalid());
        }
    }
    Ok(())
}
fn validate_mark(mark: &Value) -> Result<(), BusinessError> {
    let mark = object(mark)?;
    let kind = field(mark, "type")?;
    let permitted: &[&str] = match kind {
        "bold" | "italic" | "underline" | "strike" | "code" => &[],
        "link" => &["href", "target", "rel", "class"],
        "textStyle" => &[
            "fontFamily",
            "fontSize",
            "color",
            "backgroundColor",
            "lineHeight",
        ],
        "highlight" => &["color"],
        "mention" => &["kind", "targetId", "label"],
        _ => return Err(invalid()),
    };
    let empty = Map::new();
    let a = attrs(mark)?.unwrap_or(&empty);
    allowed(a, permitted)?;
    match kind {
        "link" => {
            let href = field(a, "href")?;
            if utf16(href) > 2048
                || !Url::parse(href).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
            {
                return Err(invalid());
            }
        }
        "textStyle" => {
            optional_string_one_of(
                a,
                "fontFamily",
                &[
                    "Arial",
                    "Georgia",
                    "Times New Roman",
                    "Noto Sans Thai",
                    "Noto Serif Thai",
                    "Courier New",
                ],
            )?;
            if let Some(height) = a.get("lineHeight").filter(|v| !v.is_null()) {
                let valid = match height {
                    Value::String(value) => ["1", "1.15", "1.5", "2"].contains(&value.as_str()),
                    Value::Number(value) => value
                        .as_f64()
                        .is_some_and(|v| [1.0, 1.15, 1.5, 2.0].contains(&v)),
                    _ => false,
                };
                if !valid {
                    return Err(invalid());
                }
            }
            if let Some(size) = a.get("fontSize").filter(|v| !v.is_null()) {
                let size = size.as_str().ok_or_else(invalid)?;
                let number = size.strip_suffix("px").ok_or_else(invalid)?;
                if number.len() != 2 || !number.parse::<u32>().is_ok_and(|n| (10..=36).contains(&n))
                {
                    return Err(invalid());
                }
            }
            check_color(a, "color")?;
            check_color(a, "backgroundColor")?;
        }
        "mention" => {
            if !matches!(field(a, "kind")?, "agent" | "person" | "record") {
                return Err(invalid());
            }
            field(a, "targetId")?;
            field(a, "label")?;
        }
        _ => {}
    }
    Ok(())
}

fn visit_node(
    value: &Value,
    depth: usize,
    body: &mut String,
    mentions: &mut Vec<RichMention>,
) -> Result<(), BusinessError> {
    if depth > 20 {
        return Err(invalid());
    }
    let node = object(value)?;
    let kind = field(node, "type")?;
    let permitted: &[&str] = match kind {
        "doc" | "blockquote" | "bulletList" | "listItem" | "table" | "tableRow" | "hardBreak"
        | "text" | "horizontalRule" => &[],
        "paragraph" => &["textAlign"],
        "heading" => &["level", "textAlign"],
        "orderedList" => &["start", "type"],
        "tableCell" | "tableHeader" => &["colspan", "rowspan", "colwidth", "align", "background"],
        _ => return Err(invalid()),
    };
    let empty = Map::new();
    let a = attrs(node)?.unwrap_or(&empty);
    allowed(a, permitted)?;
    if kind == "text" {
        field(node, "text")?;
    } else if node.contains_key("text") {
        return Err(invalid());
    }
    if kind == "heading" && !int_range(a.get("level"), 1, 3) {
        return Err(invalid());
    }
    if matches!(kind, "paragraph" | "heading") {
        optional_string_one_of(a, "textAlign", &["left", "right", "center", "justify"])?;
    }
    if kind == "orderedList" {
        optional_range(a, "start", 1, 100_000)?;
    }
    if matches!(kind, "tableCell" | "tableHeader") {
        optional_string_one_of(a, "align", &["left", "center", "right"])?;
        optional_range(a, "colspan", 1, 10)?;
        optional_range(a, "rowspan", 1, 30)?;
        if let Some(widths) = a.get("colwidth").filter(|v| !v.is_null()) {
            if !widths
                .as_array()
                .is_some_and(|values| values.iter().all(|v| int_range(Some(v), 40, 1200)))
            {
                return Err(invalid());
            }
        }
        check_color(a, "background")?;
    }
    let marks = match node.get("marks") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_array().ok_or_else(invalid)?),
    };
    if let Some(marks) = marks {
        for mark in marks {
            validate_mark(mark)?;
        }
    }
    let children = match node.get("content") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let values = value.as_array().ok_or_else(invalid)?;
            if values.len() > 1000 {
                return Err(invalid());
            }
            Some(values)
        }
    };
    // Validate children even on leaf nodes, exactly as the legacy validator does.
    // Their text is not projected by legacy walk when this node itself is text/break.
    if kind == "text" || kind == "hardBreak" {
        if let Some(children) = children {
            for child in children {
                visit_node(child, depth + 1, &mut String::new(), &mut Vec::new())?;
            }
        }
        if kind == "hardBreak" {
            body.push('\n');
            return Ok(());
        }
        let start = utf16(body);
        body.push_str(field(node, "text")?);
        if let Some(mark) = marks.and_then(|marks| {
            marks
                .iter()
                .find(|m| m.get("type").and_then(Value::as_str) == Some("mention"))
        }) {
            let a = attrs(object(mark)?)?.ok_or_else(invalid)?;
            mentions.push(RichMention {
                kind: field(a, "kind")?.into(),
                target_id: field(a, "targetId")?.into(),
                label: field(a, "label")?.into(),
                start,
                end: utf16(body),
            });
        }
        return Ok(());
    }
    if let Some(children) = children {
        for (index, child) in children.iter().enumerate() {
            if index > 0
                && matches!(
                    kind,
                    "doc" | "blockquote" | "bulletList" | "orderedList" | "listItem" | "table"
                )
            {
                body.push('\n');
            }
            if index > 0 && kind == "tableRow" {
                body.push('\t');
            }
            visit_node(child, depth + 1, body, mentions)?;
        }
    }
    Ok(())
}

/// Validate the retained v1 document and derive exact JavaScript UTF-16 spans.
/// Current mention targets, uploads, content equality and authority must be
/// verified by their owner ports before this document can be persisted.
pub fn validate_rich_content(
    input: &Value,
    max_chars: usize,
) -> Result<RichDocumentFacts, BusinessError> {
    if serde_json::to_vec(input).map_err(|_| invalid())?.len() > 1_048_576 {
        return Err(invalid());
    }
    let doc = object(input)?;
    if doc.get("version").and_then(Value::as_f64) != Some(1.0) {
        return Err(invalid());
    }
    let blocks = doc
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if blocks.len() > 100 {
        return Err(invalid());
    }
    let mut body = String::new();
    let mut mentions = Vec::new();
    let mut images = Vec::new();
    let mut text_blocks = 0;
    for block in blocks {
        let block = object(block)?;
        match field(block, "type")? {
            "image" => {
                allowed(
                    block,
                    &["type", "slotId", "attachmentId", "name", "alt", "width"],
                )?;
                let slot = field(block, "slotId")?;
                let name = field(block, "name")?;
                if utf16(slot) == 0 || utf16(slot) > 160 || utf16(name) == 0 || utf16(name) > 255 {
                    return Err(invalid());
                }
                let attachment = match block.get("attachmentId") {
                    Some(Value::Null) => None,
                    Some(Value::String(value)) if utf16(value) <= 160 => Some(value.clone()),
                    _ => return Err(invalid()),
                };
                if let Some(alt) = block.get("alt") {
                    if !alt.as_str().is_some_and(|s| utf16(s) <= 500) {
                        return Err(invalid());
                    }
                }
                if block.contains_key("width") && !int_range(block.get("width"), 25, 100) {
                    return Err(invalid());
                }
                images.push(attachment);
            }
            "text" => {
                let content = block.get("doc").ok_or_else(invalid)?;
                if content.get("type").and_then(Value::as_str) != Some("doc") {
                    return Err(invalid());
                }
                if text_blocks > 0 {
                    body.push('\n');
                }
                text_blocks += 1;
                visit_node(content, 0, &mut body, &mut mentions)?;
            }
            _ => return Err(invalid()),
        }
    }
    if !images.is_empty() && crate::domain::collaboration_work::js_trim(&body).is_empty() {
        body.clear();
    }
    if utf16(&body) > max_chars {
        return Err(invalid());
    }
    Ok(RichDocumentFacts {
        content: input.clone(),
        body,
        mentions,
        image_attachment_ids: images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn derives_legacy_utf16_offsets_nested_lines_tables_and_image_facts() {
        let value = json!({"version":1,"blocks":[{"type":"text","doc":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"😀 "},{"type":"text","text":"@Ana","marks":[{"type":"mention","attrs":{"kind":"person","targetId":"u1","label":"Ana"}}]}]},{"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"A"}]}]},{"type":"tableCell","content":[{"type":"paragraph","content":[{"type":"text","text":"B"}]}]}]}]}]}},{"type":"image","slotId":"s","attachmentId":"a","name":"x.png"}]});
        let facts = validate_rich_content(&value, 100).unwrap();
        assert_eq!(facts.body, "😀 @Ana\nA\tB");
        assert_eq!((facts.mentions[0].start, facts.mentions[0].end), (3, 7));
        assert_eq!(facts.image_attachment_ids, vec![Some("a".into())]);
        assert_eq!(facts.content, value);
    }

    #[test]
    fn rejects_unsafe_urls_unsupported_attributes_and_excess_utf16() {
        let link = |href| json!({"version":1,"blocks":[{"type":"text","doc":{"type":"doc","content":[{"type":"text","text":"x","marks":[{"type":"link","attrs":{"href":href}}]}]}}]});
        assert!(validate_rich_content(&link("javascript:alert(1)"), 10).is_err());
        assert!(validate_rich_content(&link("https://example.org/a"), 10).is_ok());
        let mut bad = link("https://example.org");
        bad["blocks"][0]["doc"]["attrs"] = json!({"onload":"x"});
        assert!(validate_rich_content(&bad, 10).is_err());
        let emoji = json!({"version":1,"blocks":[{"type":"text","doc":{"type":"doc","content":[{"type":"text","text":"😀"}]}}]});
        assert!(validate_rich_content(&emoji, 1).is_err());
        assert!(validate_rich_content(&emoji, 2).is_ok());
    }

    #[test]
    fn matches_independently_checked_legacy_schema_examples() {
        let text = |node: Value| json!({"version":1,"blocks":[{"type":"text","doc":{"type":"doc","content":[node]}}]});
        let mark = |kind: &str, attrs: Value| {
            text(json!({"type":"text","text":"x","marks":[{"type":kind,"attrs":attrs}]}))
        };
        let cases = vec![
            (
                mark("link", json!({"href":"https://example.org/a"})),
                10,
                true,
            ),
            (
                mark("link", json!({"href":"javascript:alert(1)"})),
                10,
                false,
            ),
            (
                text(json!({"type":"paragraph","attrs":{"onload":"x"}})),
                10,
                false,
            ),
            (
                mark(
                    "textStyle",
                    json!({"fontSize":"36px","color":"#ffA000","fontFamily":"Noto Sans Thai","lineHeight":"1.5"}),
                ),
                10,
                true,
            ),
            (mark("textStyle", json!({"fontSize":"37px"})), 10, false),
            (mark("textStyle", json!({"lineHeight":1})), 10, true),
            (
                mark(
                    "mention",
                    json!({"kind":"agent","targetId":"helper","label":"Helper"}),
                ),
                10,
                true,
            ),
            (
                mark(
                    "mention",
                    json!({"kind":"record","targetId":"r","label":"Record"}),
                ),
                10,
                true,
            ),
            (
                mark(
                    "mention",
                    json!({"kind":"person","targetId":"u","label":"Person"}),
                ),
                10,
                true,
            ),
            (
                mark(
                    "mention",
                    json!({"kind":"robot","targetId":"u","label":"Robot"}),
                ),
                10,
                false,
            ),
            (text(json!({"type":"text","text":"😀"})), 1, false),
            (text(json!({"type":"text","text":"😀"})), 2, true),
            (
                json!({"version":1,"blocks":[{"type":"image","slotId":"x","attachmentId":"a","name":"x","width":24}]}),
                10,
                false,
            ),
            (
                json!({"version":1,"blocks":[{"type":"image","slotId":"x","attachmentId":"a","name":"x","width":25}]}),
                10,
                true,
            ),
            (
                text(json!({"type":"heading","attrs":{"level":4}})),
                10,
                false,
            ),
            (
                text(json!({"type":"tableCell","attrs":{"colwidth":[39]}})),
                10,
                false,
            ),
        ];
        for (value, limit, expected) in cases {
            assert_eq!(
                validate_rich_content(&value, limit).is_ok(),
                expected,
                "{value}"
            );
        }
    }
}
