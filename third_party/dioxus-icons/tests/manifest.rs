use serde_json::Value;

const MANIFEST_JSON: &str = include_str!("../src/generated/manifest.json");
const STATIC_PICKER_HTML: &str = include_str!("../STATIC_PICKER.html");
const EXPECTED_ICON_COUNT: usize = 1_703;

fn manifest() -> Value {
    serde_json::from_str(MANIFEST_JSON).expect("generated manifest is valid JSON")
}

#[test]
fn generated_manifest_has_expected_shape_and_count() {
    let manifest = manifest();
    assert_eq!(manifest["version"], "1.14.0");
    assert_eq!(manifest["source"], "lucide");

    let icons = manifest["icons"]
        .as_array()
        .expect("manifest icons is an array");
    assert_eq!(icons.len(), EXPECTED_ICON_COUNT);

    for icon in icons {
        let name = icon["name"].as_str().expect("icon name is a string");
        let module = icon["module"].as_str().expect("icon module is a string");
        let tags = icon["tags"].as_array().expect("icon tags is an array");
        let categories = icon["categories"]
            .as_array()
            .expect("icon categories is an array");
        let svg = icon["svg"].as_str().expect("icon svg is a string");

        assert!(!name.is_empty());
        assert!(!module.is_empty());
        assert!(tags.iter().all(Value::is_string));
        assert!(categories.iter().all(Value::is_string));
        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains(r#"viewBox="0 0 24 24""#));
    }
}

#[test]
fn generated_manifest_order_and_known_metadata_are_stable() {
    let manifest = manifest();
    let icons = manifest["icons"].as_array().unwrap();

    let source_file_keys = icons
        .iter()
        .map(|icon| format!("{}.svg", icon["module"].as_str().unwrap().replace('_', "-")))
        .collect::<Vec<_>>();
    let mut sorted_source_file_keys = source_file_keys.clone();
    sorted_source_file_keys.sort_unstable();
    assert_eq!(source_file_keys, sorted_source_file_keys);

    let trash = icons
        .iter()
        .find(|icon| icon["name"] == "Trash")
        .expect("Trash icon is present");
    assert_eq!(trash["module"], "trash");
    assert!(
        trash["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "delete")
    );
    assert!(
        trash["categories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|category| category == "files")
    );
}

#[test]
fn generated_manifest_escapes_svg_payloads_in_json() {
    assert!(MANIFEST_JSON.contains(r#""svg": "\u003csvg "#));
    assert!(!MANIFEST_JSON.contains("<svg "));
    assert!(!MANIFEST_JSON.contains("</svg>"));
}

#[test]
fn generated_static_picker_links_to_manifest_icons() {
    let manifest = manifest();
    let icons = manifest["icons"].as_array().unwrap();

    for icon in icons {
        let name = icon["name"].as_str().unwrap();
        let expected_href = format!(r#"href="lucide/fn.{name}.html""#);
        assert!(
            STATIC_PICKER_HTML.contains(&expected_href),
            "static picker is missing link for {name}"
        );
    }
}
