use erpx_server_shared::public::json_numbers::*;
use serde::Deserialize;
#[derive(Debug, Deserialize, PartialEq)]
struct Unsigned32(#[serde(deserialize_with = "deserialize_u32")] u32);
#[derive(Debug, Deserialize, PartialEq)]
struct Signed(#[serde(deserialize_with = "deserialize_safe_i64")] i64);
#[derive(Debug, Deserialize, PartialEq)]
struct Unsigned(#[serde(deserialize_with = "deserialize_safe_u64")] u64);
#[test]
fn integral_json_lexical_forms_match_javascript_structural_integer_input() {
    for value in ["1", "1.0", "1e0", "10e-1"] {
        assert_eq!(
            serde_json::from_str::<Unsigned32>(value).unwrap(),
            Unsigned32(1)
        );
        assert_eq!(serde_json::from_str::<Signed>(value).unwrap(), Signed(1));
        assert_eq!(
            serde_json::from_str::<Unsigned>(value).unwrap(),
            Unsigned(1)
        );
    }
    for value in ["0", "0.0", "-0.0"] {
        assert_eq!(
            serde_json::from_str::<Unsigned32>(value).unwrap(),
            Unsigned32(0)
        );
    }
}
#[test]
fn fractional_values_and_non_numeric_json_types_are_not_coerced() {
    for value in [
        "1.5", "-1.5", "1e-1", "\"1\"", "true", "false", "null", "{}", "[]",
    ] {
        assert!(
            serde_json::from_str::<Unsigned32>(value).is_err(),
            "accepted{value}"
        );
        assert!(
            serde_json::from_str::<Signed>(value).is_err(),
            "accepted{value}"
        );
        assert!(
            serde_json::from_str::<Unsigned>(value).is_err(),
            "accepted{value}"
        );
    }
}
#[test]
fn unsigned_bounds_and_negative_values_are_checked_after_numeric_decoding() {
    for value in ["4294967295", "4294967295.0"] {
        assert_eq!(
            serde_json::from_str::<Unsigned32>(value).unwrap().0,
            u32::MAX
        );
    }
    for value in ["4294967296", "4294967296.0", "-1", "-1.0", "-1e0"] {
        assert!(serde_json::from_str::<Unsigned32>(value).is_err());
    }
    for value in ["-1", "-1.0", "-1e0"] {
        assert_eq!(serde_json::from_str::<Signed>(value).unwrap(), Signed(-1));
        assert!(serde_json::from_str::<Unsigned>(value).is_err());
    }
}
#[test]
fn safe_integer_limits_do_not_accept_lossy_i64_or_u64_wire_values() {
    for value in [
        "9007199254740991",
        "9007199254740991.0",
        "9.007199254740991e15",
    ] {
        assert_eq!(
            serde_json::from_str::<Signed>(value).unwrap().0,
            MAX_SAFE_INTEGER
        );
        assert_eq!(
            serde_json::from_str::<Unsigned>(value).unwrap().0,
            MAX_SAFE_INTEGER as u64
        );
    }
    assert_eq!(
        serde_json::from_str::<Signed>("-9007199254740991")
            .unwrap()
            .0,
        -MAX_SAFE_INTEGER
    );
    for value in [
        "9007199254740992",
        "9007199254740992.0",
        "9007199254740993",
        "-9007199254740992",
        "9223372036854775807",
        "-9223372036854775808",
        "18446744073709551615",
        "1e100",
    ] {
        assert!(
            serde_json::from_str::<Signed>(value).is_err(),
            "accepted{value}"
        );
        assert!(
            serde_json::from_str::<Unsigned>(value).is_err(),
            "accepted{value}"
        );
    }
}
#[test]
fn nonfinite_deserializer_values_never_become_integer_casts() {
    use serde::de::value::{Error, F64Deserializer};
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(deserialize_u32(F64Deserializer::<Error>::new(value)).is_err());
        assert!(deserialize_safe_i64(F64Deserializer::<Error>::new(value)).is_err());
        assert!(deserialize_safe_u64(F64Deserializer::<Error>::new(value)).is_err());
    }
}
#[derive(Debug, Deserialize, PartialEq)]
struct OptionalInput {
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    small: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_safe_i64")]
    signed: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_optional_safe_u64")]
    unsigned: Option<u64>,
}
#[test]
fn optional_structural_integers_preserve_absence_null_and_numeric_forms() {
    let empty = OptionalInput {
        small: None,
        signed: None,
        unsigned: None,
    };
    assert_eq!(serde_json::from_str::<OptionalInput>("{}").unwrap(), empty);
    assert_eq!(
        serde_json::from_str::<OptionalInput>(r#"{"small":null,"signed":null,"unsigned":null}"#)
            .unwrap(),
        empty
    );
    assert_eq!(
        serde_json::from_str::<OptionalInput>(r#"{"small":1.0,"signed":-1e0,"unsigned":1e0}"#)
            .unwrap(),
        OptionalInput {
            small: Some(1),
            signed: Some(-1),
            unsigned: Some(1)
        }
    );
    assert!(serde_json::from_str::<OptionalInput>(r#"{"small":"1"}"#).is_err());
}
