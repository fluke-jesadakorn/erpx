use erpx_server_identity_access::public::oidc_url_values as values;
use serde_json::Value;
use values::{encode_uri_component_utf16, set_authorization_parameters};
#[test]
fn pinned_node_url_set_bytes_and_ordered_tuples() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/identity_access/oidc/url-values.v1.json"
    ))
    .unwrap();
    for case in fixture["urls"].as_array().unwrap() {
        let url = url::Url::parse(case["endpoint"].as_str().unwrap()).unwrap();
        let actual = set_authorization_parameters(
            url,
            fixture["client"].as_str().unwrap(),
            fixture["redirect"].as_str().unwrap(),
            fixture["state"].as_str().unwrap(),
            fixture["nonce"].as_str().unwrap(),
            fixture["challenge"].as_str().unwrap(),
        );
        assert_eq!(
            actual.as_str(),
            case["url"].as_str().unwrap(),
            "{}",
            case["id"]
        );
        let pairs: Vec<(String, String)> = actual
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let expected: Vec<(String, String)> =
            serde_json::from_value(case["tuples"].clone()).unwrap();
        assert_eq!(pairs, expected, "{}", case["id"]);
    }
}
#[test]
fn pinned_node_graph_component_encoding_and_surrogate_errors() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/identity_access/oidc/url-values.v1.json"
    ))
    .unwrap();
    for case in fixture["graph"].as_array().unwrap() {
        let units: Vec<u16> = serde_json::from_value(case["units"].clone()).unwrap();
        let actual = encode_uri_component_utf16(&units);
        if case["error"].is_null() {
            assert_eq!(
                actual.unwrap(),
                case["output"].as_str().unwrap(),
                "{}",
                case["id"]
            );
        } else {
            assert_eq!(case["error"], "URIError");
            assert!(actual.is_err(), "{}", case["id"]);
        }
    }
}

#[test]
fn original_append_and_form_encoder_disagree_with_captured_node() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/identity_access/oidc/url-values.v1.json"
    ))
    .unwrap();
    let case = fixture["urls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "encoded-name")
        .unwrap();
    let mut old = url::Url::parse(case["endpoint"].as_str().unwrap()).unwrap();
    old.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", fixture["client"].as_str().unwrap())
        .append_pair("redirect_uri", fixture["redirect"].as_str().unwrap())
        .append_pair("scope", "openid profile email")
        .append_pair("state", fixture["state"].as_str().unwrap())
        .append_pair("nonce", fixture["nonce"].as_str().unwrap())
        .append_pair("code_challenge", fixture["challenge"].as_str().unwrap())
        .append_pair("code_challenge_method", "S256");
    assert_ne!(old.as_str(), case["url"].as_str().unwrap());
    assert_eq!(
        old.query_pairs().filter(|(key, _)| key == "state").count(),
        4
    );
    let input = " !'()~";
    let old_graph = url::form_urlencoded::byte_serialize(input.as_bytes()).collect::<String>();
    assert_eq!(old_graph, "+%21%27%28%29%7E");
    assert_eq!(values::encode_uri_component(input), "%20!'()~");
}

#[test]
fn url_search_params_replaces_unpaired_surrogates_instead_of_uri_error() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/identity_access/oidc/url-values.v1.json"
    ))
    .unwrap();
    let units: Vec<u16> = serde_json::from_value(fixture["usv"]["stateUnits"].clone()).unwrap();
    let state = String::from_utf16_lossy(&units);
    let url = set_authorization_parameters(
        url::Url::parse("https://issuer.example.test/authorize").unwrap(),
        fixture["client"].as_str().unwrap(),
        fixture["redirect"].as_str().unwrap(),
        &state,
        fixture["nonce"].as_str().unwrap(),
        fixture["challenge"].as_str().unwrap(),
    );
    assert_eq!(url.as_str(), fixture["usv"]["url"].as_str().unwrap());
    assert!(encode_uri_component_utf16(&units).is_err());
}
