//! Compile-time English/Thai catalog for ERPX-authored console copy.
//!
//! Domain views keep their copy beside the component that owns it, while this
//! catalog covers the shared console families and provides one reviewable
//! inventory spanning every shipped presentation area.

use erpx_ui_core::{Locale, LocalizedText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleCopyGroup {
    Authentication,
    Shell,
    Customer360,
    Platform,
    Billing,
    Operations,
    Finance,
    Sales,
    Scm,
    Assets,
    Quality,
    Hr,
    Legal,
    Pos,
    Marketing,
    Crm,
    Support,
    CorporateIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleCopyKey {
    SignIn,
    ConsoleSurfaces,
    Customer360,
    PlatformOperations,
    BillingEntitlements,
    Operations,
    Finance,
    Sales,
    SupplyChain,
    AssetsMaintenance,
    Quality,
    HrPayroll,
    Legal,
    PointOfSale,
    Marketing,
    CustomerRelationshipManagement,
    Support,
    CorporateIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConsoleCatalogEntry {
    pub(crate) group: ConsoleCopyGroup,
    pub(crate) key: ConsoleCopyKey,
    pub(crate) copy: LocalizedText,
}

macro_rules! entry {
    ($group:ident, $key:ident, $en:literal, $th:literal) => {
        ConsoleCatalogEntry {
            group: ConsoleCopyGroup::$group,
            key: ConsoleCopyKey::$key,
            copy: LocalizedText::new($en, $th),
        }
    };
}

pub(crate) const CONSOLE_CATALOG: &[ConsoleCatalogEntry] = &[
    entry!(Authentication, SignIn, "Sign in", "เข้าสู่ระบบ"),
    entry!(
        Shell,
        ConsoleSurfaces,
        "Console surfaces",
        "พื้นที่ทำงานของคอนโซล"
    ),
    entry!(Customer360, Customer360, "Customer 360", "ลูกค้า 360"),
    entry!(
        Platform,
        PlatformOperations,
        "Platform Operations",
        "การปฏิบัติการแพลตฟอร์ม"
    ),
    entry!(
        Billing,
        BillingEntitlements,
        "Billing & entitlements",
        "การเรียกเก็บเงินและสิทธิ์ใช้งาน"
    ),
    entry!(Operations, Operations, "Operations", "การปฏิบัติการ"),
    entry!(Finance, Finance, "Finance", "การเงิน"),
    entry!(Sales, Sales, "Sales", "การขาย"),
    entry!(Scm, SupplyChain, "Supply chain", "ห่วงโซ่อุปทาน"),
    entry!(
        Assets,
        AssetsMaintenance,
        "Assets & maintenance",
        "สินทรัพย์และการบำรุงรักษา"
    ),
    entry!(Quality, Quality, "Quality", "คุณภาพ"),
    entry!(Hr, HrPayroll, "HR & payroll", "ทรัพยากรบุคคลและเงินเดือน"),
    entry!(Legal, Legal, "Legal", "กฎหมาย"),
    entry!(Pos, PointOfSale, "Point of sale", "จุดขาย"),
    entry!(Marketing, Marketing, "Marketing", "การตลาด"),
    entry!(
        Crm,
        CustomerRelationshipManagement,
        "Customer relationship management",
        "การบริหารความสัมพันธ์ลูกค้า"
    ),
    entry!(Support, Support, "Support", "การสนับสนุน"),
    entry!(
        CorporateIdentity,
        CorporateIdentity,
        "Corporate identity",
        "อัตลักษณ์องค์กร"
    ),
];

pub(crate) fn text(key: ConsoleCopyKey, locale: Locale) -> &'static str {
    CONSOLE_CATALOG
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.copy.text(locale))
        .unwrap_or("ERPX")
}

#[cfg(test)]
mod tests {
    use super::{CONSOLE_CATALOG, ConsoleCopyGroup};

    const PRESENTATION_SOURCES: &[(&str, &str)] = &[
        ("console", include_str!("lib.rs")),
        ("product completion", include_str!("product_completion.rs")),
        (
            "shared components",
            include_str!("../../../crates/ui-kit/src/components.rs"),
        ),
        (
            "support surfaces",
            include_str!("../../../crates/ui-domains/support/src/support_surfaces.rs"),
        ),
    ];

    // Intentional authored proper names and font-family choices. Machine values
    // (classes, roles, protocol attributes, identifiers, and interpolation-only
    // strings) are excluded structurally below rather than being allow-listed.
    const IDENTICAL_PRESENTATION_ALLOWLIST: &[&str] = &[
        "ERPX",
        "ERPX Sans",
        "Corporate Sans",
        "Thai Formal",
        "System UI",
        "A",
        "ID",
        "⌘K",
    ];

    #[test]
    fn catalog_is_complete_safe_and_intentionally_bilingual() {
        assert_eq!(CONSOLE_CATALOG.len(), 18);
        assert!(CONSOLE_CATALOG.iter().all(|entry| entry.copy.is_valid()));
        assert!(CONSOLE_CATALOG.iter().all(|entry| {
            entry.copy.english() != entry.copy.thai()
                && !entry.copy.english().chars().any(char::is_control)
                && !entry.copy.thai().chars().any(char::is_control)
        }));

        let mut groups = CONSOLE_CATALOG
            .iter()
            .map(|entry| entry.group)
            .collect::<Vec<_>>();
        groups.sort_by_key(|group| *group as u8);
        groups.dedup();
        assert_eq!(groups.len(), 18);
        assert!(groups.contains(&ConsoleCopyGroup::Authentication));
        assert!(groups.contains(&ConsoleCopyGroup::CorporateIdentity));
    }

    #[test]
    fn authored_presentation_copy_cannot_bypass_the_bilingual_boundary() {
        let mut violations = Vec::new();
        for (name, source) in PRESENTATION_SOURCES {
            let source = source
                .split("#[cfg(test)]\nmod tests")
                .next()
                .unwrap_or(source);
            for (index, line) in source.lines().enumerate() {
                if line.contains("preview_locale.read()") {
                    // The corporate-identity preview intentionally renders both
                    // branches so designers can review English and Thai at once.
                    continue;
                }
                let masked = mask_localized_calls(line);
                for (start, value, end) in string_literals(&masked) {
                    if !value.bytes().any(|byte| byte.is_ascii_alphabetic())
                        || IDENTICAL_PRESENTATION_ALLOWLIST.contains(&value.as_str())
                        || value.contains("localized_copy")
                    {
                        continue;
                    }
                    let outside_interpolation = remove_interpolations(&value);
                    if !outside_interpolation
                        .bytes()
                        .any(|byte| byte.is_ascii_alphabetic())
                    {
                        continue;
                    }
                    let before = &masked[..start];
                    let after = &masked[end..];
                    let is_presentation_property = [
                        "aria_label:",
                        "placeholder:",
                        "title:",
                        "message:",
                        "description:",
                        "label:",
                    ]
                    .iter()
                    .any(|property| before.trim_end().ends_with(property));
                    let is_machine_property = [
                        "class:",
                        "role:",
                        "aria_current:",
                        "aria_selected:",
                        "aria_live:",
                        "value:",
                        "id:",
                        "r#type:",
                        "inputmode:",
                        "autocomplete:",
                        "tone:",
                        "checked:",
                        "\"data-",
                    ]
                    .iter()
                    .any(|property| {
                        before
                            .rfind(property)
                            .is_some_and(|position| position > before.rfind(',').unwrap_or(0))
                    });
                    let before_token = before.trim_end().chars().last();
                    let after_trimmed = after.trim_start();
                    let opens_nested_element = after_trimmed.find('{').is_some_and(|position| {
                        !after_trimmed[..position].trim().is_empty()
                            && after_trimmed[..position]
                                .trim()
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    });
                    let is_child = matches!(before_token, Some('{') | Some(',') | Some('}'))
                        && (after_trimmed.starts_with('}') || opens_nested_element);
                    let machine_example = value == "finance"
                        || value == "UUID"
                        || value == "YYYY-MM"
                        || value.starts_with("2026-")
                        || value.contains("tenant-001")
                        || value.contains("holding-ref-001")
                        || value.contains("secret-manager/")
                        || value.contains("provider-key:")
                        || value.starts_with("v{")
                        || value.starts_with("ui-")
                        || (line.contains("\"data-") && matches!(value.as_str(), "true" | "false"))
                        || matches!(value.as_str(), "page" | "false")
                        || value == "bank_account or reconciliation"
                        || matches!(
                            value.as_str(),
                            "YYYY-MM-DD"
                                | "sales_order"
                                | "purchase_order"
                                | "company-wide"
                                | "default"
                        );
                    if !machine_example
                        && (is_presentation_property || (is_child && !is_machine_property))
                    {
                        violations.push(format!("{name}:{}: {value}", index + 1));
                    }
                }
            }
        }
        assert!(
            violations.is_empty(),
            "direct ERPX-authored presentation literals must use LocalizedText/TranslationKey:\n{}",
            violations.join("\n")
        );
    }

    fn mask_localized_calls(line: &str) -> String {
        let mut output = line.to_owned();
        for function in [
            "localized_copy",
            "product_copy",
            "console_text",
            "shared_text",
        ] {
            let mut search_from = 0;
            while let Some(relative) = output[search_from..].find(function) {
                let start = search_from + relative;
                let Some(open_relative) = output[start..].find('(') else {
                    break;
                };
                let open = start + open_relative;
                let mut quoted = false;
                let mut escaped = false;
                let mut depth = 0_u32;
                let mut close = None;
                for (relative, character) in output[open..].char_indices() {
                    if quoted {
                        if escaped {
                            escaped = false;
                        } else if character == '\\' {
                            escaped = true;
                        } else if character == '"' {
                            quoted = false;
                        }
                        continue;
                    }
                    match character {
                        '"' => quoted = true,
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                close = Some(open + relative + 1);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let Some(close) = close else { break };
                output.replace_range(start..close, &" ".repeat(close - start));
                search_from = close;
            }
        }
        output
    }

    fn string_literals(line: &str) -> Vec<(usize, String, usize)> {
        let mut values = Vec::new();
        let bytes = line.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'"' {
                index += 1;
                continue;
            }
            let start = index;
            index += 1;
            let value_start = index;
            let mut escaped = false;
            while index < bytes.len() {
                if escaped {
                    escaped = false;
                } else if bytes[index] == b'\\' {
                    escaped = true;
                } else if bytes[index] == b'"' {
                    values.push((start, line[value_start..index].to_owned(), index + 1));
                    index += 1;
                    break;
                }
                index += 1;
            }
        }
        values
    }

    fn remove_interpolations(value: &str) -> String {
        let mut output = String::new();
        let mut depth = 0_u32;
        for character in value.chars() {
            match character {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                _ if depth == 0 => output.push(character),
                _ => {}
            }
        }
        output
    }
}
