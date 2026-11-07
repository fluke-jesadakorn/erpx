use erpx_server_shared::public::{
    CommandKey, ConsumptionLot, DecimalAmount, RecordId, SourceRecord, SourceRecordKind,
};

#[test]
fn exact_money_roundtrips_above_javascript_integer_precision() {
    let value = DecimalAmount::parse("9007199254740993.01", 2).unwrap();
    assert_eq!(value.units(), 900_719_925_474_099_301);
    assert_eq!(value.decimal_string(), "9007199254740993.01");
    assert_eq!(
        DecimalAmount::parse("-0.01", 2).unwrap().decimal_string(),
        "-0.01"
    );
    assert_eq!(
        DecimalAmount::parse("1.2300", 2).unwrap().decimal_string(),
        "1.23"
    );
    assert_eq!(
        DecimalAmount::parse("0", 12).unwrap().decimal_string(),
        "0.000000000000"
    );
}

#[test]
fn exact_money_preserves_full_signed_boundary_without_floating_point() {
    for units in [i128::MIN, i128::MAX, -1, 0, 1] {
        for scale in [0, 2, 12] {
            let value = DecimalAmount::from_units(units, scale).unwrap();
            assert_eq!(
                DecimalAmount::parse(&value.decimal_string(), scale).unwrap(),
                value
            );
        }
    }
}

#[test]
fn money_rejects_overflow_unsupported_precision_and_mismatched_scales() {
    for input in [
        "", ".50", "1.", "1e3", "NaN", "+1", "1.2.3", "-", "12,000", "1.001",
    ] {
        assert!(
            DecimalAmount::parse(input, 2).is_err(),
            "accepted {input:?}"
        );
    }
    assert!(DecimalAmount::parse("1", 13).is_err());
    assert!(DecimalAmount::parse("170141183460469231731687303715884105728", 0).is_err());
    assert!(
        DecimalAmount::from_units(i128::MAX, 0)
            .unwrap()
            .checked_add(DecimalAmount::from_units(1, 0).unwrap())
            .is_err()
    );
    assert!(
        DecimalAmount::from_units(100, 2)
            .unwrap()
            .checked_add(DecimalAmount::from_units(1, 0).unwrap())
            .is_err()
    );
    assert_eq!(
        DecimalAmount::parse("1.25", 2)
            .unwrap()
            .checked_add(DecimalAmount::parse("-0.25", 2).unwrap())
            .unwrap()
            .decimal_string(),
        "1.00"
    );
}

#[test]
fn command_key_and_id_deserialization_cannot_bypass_validation() {
    for key in [
        "",
        "short",
        "space is forbidden",
        "slash/is/forbidden",
        "nonascii-ก",
    ] {
        assert!(CommandKey::new(key).is_err());
        assert!(serde_json::from_str::<CommandKey>(&serde_json::to_string(key).unwrap()).is_err());
    }
    let key: CommandKey = serde_json::from_str("\"finance:post_01-ok\"").unwrap();
    assert_eq!(key.as_str(), "finance:post_01-ok");
    assert!(RecordId::new("").is_err());
    assert!(RecordId::new("line\nbreak").is_err());
    assert!(RecordId::new("x".repeat(201)).is_err());
    assert!(serde_json::from_str::<RecordId>("\"\"").is_err());
}

#[test]
fn source_and_consumption_evidence_require_positive_current_versions() {
    let id = RecordId::new("source-1").unwrap();
    assert!(SourceRecord::new(SourceRecordKind::PmsRecord, id.clone(), Some(0)).is_err());
    let source = SourceRecord::new(SourceRecordKind::PmsRecord, id.clone(), Some(7)).unwrap();
    assert_eq!(source.version(), Some(7));
    assert_eq!(source.id(), &id);
    assert_eq!(
        serde_json::to_string(&source.kind()).unwrap(),
        "\"pms_record\""
    );
    assert!(ConsumptionLot::new(id.clone(), 0, 1).is_err());
    assert!(ConsumptionLot::new(id.clone(), 1, 0).is_err());
    assert!(ConsumptionLot::new(id.clone(), 9_007_199_254_740_992, 1).is_err());
    let lot = ConsumptionLot::new(id, 1500, 9).unwrap();
    assert_eq!(lot.quantity_milli(), 1500);
    assert_eq!(lot.balance_version(), 9);
}
