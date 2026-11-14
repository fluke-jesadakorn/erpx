use erpx_server_people_payroll::public::payroll_accounting::{
    PayrollProjectionOutcome, payroll_accounting_lines, payroll_net_cost_shares,
    read_payroll_projection_inputs,
};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::fmt;
#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    items_json: String,
    accounts_json: String,
    accounting: Observation,
    shares: Vec<Share>,
}
#[derive(Deserialize)]
struct Share {
    index: usize,
    ok: bool,
    value: Option<Box<RawValue>>,
    error: Option<Failure>,
    #[serde(rename = "specialNumbers", default)]
    special_numbers: Vec<Special>,
}
#[derive(Deserialize)]
struct Observation {
    ok: bool,
    value: Option<Box<RawValue>>,
    error: Option<Failure>,
    #[serde(rename = "specialNumbers", default)]
    special_numbers: Vec<Special>,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Special {
    path: String,
    kind: String,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Failure {
    name: String,
    status: Option<u16>,
    code: Option<String>,
    message: String,
}
#[derive(Debug)]
enum Ordered {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Ordered>),
    Object(Vec<(String, Ordered)>),
}
impl PartialEq for Ordered {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => {
                if *a == 0.0 && *b == 0.0 {
                    a.is_sign_negative() == b.is_sign_negative()
                } else {
                    a == b
                }
            }
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => a == b,
            _ => false,
        }
    }
}
impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON value")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Ordered, E> {
                Ok(Ordered::Null)
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Ordered, E> {
                Ok(Ordered::Null)
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Ordered, E> {
                Ok(Ordered::Bool(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Ordered, E> {
                Ok(Ordered::Number(v as f64))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Ordered, E> {
                Ok(Ordered::Number(v as f64))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Ordered, E> {
                Ok(Ordered::Number(v))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Ordered, E> {
                Ok(Ordered::String(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Ordered, E> {
                Ok(Ordered::String(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Ordered, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = a.next_element()? {
                    v.push(x)
                }
                Ok(Ordered::Array(v))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Ordered, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = a.next_entry()? {
                    v.push(x)
                }
                Ok(Ordered::Object(v))
            }
        }
        d.deserialize_any(V)
    }
}
// Capture source numeric values before JSON can turn non-finite values into null.
#[derive(Debug)]
struct CaptureError(String);
impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CaptureError {}
impl serde::ser::Error for CaptureError {
    fn custom<T: fmt::Display>(v: T) -> Self {
        Self(v.to_string())
    }
}
struct Capture;
struct Seq(Vec<Ordered>);
struct Map {
    values: Vec<(String, Ordered)>,
    key: Option<String>,
    raw: bool,
}
impl serde::Serializer for Capture {
    type Ok = Ordered;
    type Error = CaptureError;
    type SerializeSeq = Seq;
    type SerializeTuple = serde::ser::Impossible<Ordered, CaptureError>;
    type SerializeTupleStruct = serde::ser::Impossible<Ordered, CaptureError>;
    type SerializeTupleVariant = serde::ser::Impossible<Ordered, CaptureError>;
    type SerializeMap = Map;
    type SerializeStruct = Map;
    type SerializeStructVariant = serde::ser::Impossible<Ordered, CaptureError>;
    fn serialize_bool(self, v: bool) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Ordered, CaptureError> {
        self.serialize_i64(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<Ordered, CaptureError> {
        self.serialize_i64(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<Ordered, CaptureError> {
        self.serialize_i64(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Number(v as f64))
    }
    fn serialize_u8(self, v: u8) -> Result<Ordered, CaptureError> {
        self.serialize_u64(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<Ordered, CaptureError> {
        self.serialize_u64(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<Ordered, CaptureError> {
        self.serialize_u64(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Number(v as f64))
    }
    fn serialize_f32(self, v: f32) -> Result<Ordered, CaptureError> {
        self.serialize_f64(v.into())
    }
    fn serialize_f64(self, v: f64) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Number(v))
    }
    fn serialize_char(self, v: char) -> Result<Ordered, CaptureError> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<Ordered, CaptureError> {
        Ok(Ordered::String(v.into()))
    }
    fn serialize_none(self) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Null)
    }
    fn serialize_some<T: serde::Serialize + ?Sized>(self, v: &T) -> Result<Ordered, CaptureError> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Null)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Seq, CaptureError> {
        Ok(Seq(Vec::new()))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Map, CaptureError> {
        Ok(Map {
            values: Vec::new(),
            key: None,
            raw: false,
        })
    }
    fn serialize_struct(self, name: &'static str, _: usize) -> Result<Map, CaptureError> {
        Ok(Map {
            values: Vec::new(),
            key: None,
            raw: name == "$serde_json::private::RawValue",
        })
    }
    fn serialize_newtype_struct<T: serde::Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<Ordered, CaptureError> {
        v.serialize(self)
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<Ordered, CaptureError> {
        Err(CaptureError("bytes".into()))
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Ordered, CaptureError> {
        Err(CaptureError("unit struct".into()))
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<Ordered, CaptureError> {
        Err(CaptureError("unit variant".into()))
    }
    fn serialize_newtype_variant<T: serde::Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Ordered, CaptureError> {
        Err(CaptureError("newtype variant".into()))
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, CaptureError> {
        Err(CaptureError("tuple".into()))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, CaptureError> {
        Err(CaptureError("tuple struct".into()))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, CaptureError> {
        Err(CaptureError("tuple variant".into()))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, CaptureError> {
        Err(CaptureError("struct variant".into()))
    }
}
impl serde::ser::SerializeSeq for Seq {
    type Ok = Ordered;
    type Error = CaptureError;
    fn serialize_element<T: serde::Serialize + ?Sized>(
        &mut self,
        v: &T,
    ) -> Result<(), CaptureError> {
        self.0.push(v.serialize(Capture)?);
        Ok(())
    }
    fn end(self) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Array(self.0))
    }
}
impl serde::ser::SerializeMap for Map {
    type Ok = Ordered;
    type Error = CaptureError;
    fn serialize_key<T: serde::Serialize + ?Sized>(&mut self, v: &T) -> Result<(), CaptureError> {
        match v.serialize(Capture)? {
            Ordered::String(s) => {
                self.key = Some(s);
                Ok(())
            }
            _ => Err(CaptureError("map key".into())),
        }
    }
    fn serialize_value<T: serde::Serialize + ?Sized>(&mut self, v: &T) -> Result<(), CaptureError> {
        self.values.push((
            self.key
                .take()
                .ok_or_else(|| CaptureError("missing key".into()))?,
            v.serialize(Capture)?,
        ));
        Ok(())
    }
    fn end(self) -> Result<Ordered, CaptureError> {
        Ok(Ordered::Object(self.values))
    }
}
impl serde::ser::SerializeStruct for Map {
    type Ok = Ordered;
    type Error = CaptureError;
    fn serialize_field<T: serde::Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), CaptureError> {
        self.values.push((k.into(), v.serialize(Capture)?));
        Ok(())
    }
    fn end(self) -> Result<Ordered, CaptureError> {
        if self.raw {
            match self.values.into_iter().next() {
                Some((_, Ordered::String(s))) => {
                    serde_json::from_str(&s).map_err(|e| CaptureError(e.to_string()))
                }
                _ => Err(CaptureError("raw JSON".into())),
            }
        } else {
            Ok(Ordered::Object(self.values))
        }
    }
}
fn collect_special(value: &Ordered, path: &str, out: &mut Vec<Special>) {
    match value {
        Ordered::Number(n) if !n.is_finite() || (*n == 0.0 && n.is_sign_negative()) => {
            out.push(Special {
                path: path.into(),
                kind: if n.is_nan() {
                    "NaN"
                } else if *n == f64::INFINITY {
                    "+Infinity"
                } else if *n == f64::NEG_INFINITY {
                    "-Infinity"
                } else {
                    "-0"
                }
                .into(),
            })
        }
        Ordered::Array(xs) => {
            for (i, v) in xs.iter().enumerate() {
                collect_special(v, &format!("{path}[{i}]"), out)
            }
        }
        Ordered::Object(xs) => {
            for (k, v) in xs {
                collect_special(v, &format!("{path}.{k}"), out)
            }
        }
        _ => {}
    }
}
#[test]
fn comparator_rejects_negative_zero_and_captures_before_json() {
    let bad = Ordered::Object(vec![(
        "categories".into(),
        Ordered::Object(vec![("bonus".into(), Ordered::Number(-0.0))]),
    )]);
    let good = Ordered::Object(vec![(
        "categories".into(),
        Ordered::Object(vec![("bonus".into(), Ordered::Number(0.0))]),
    )]);
    assert_ne!(bad, good);
    let mut special = Vec::new();
    collect_special(&bad, "$", &mut special);
    assert_eq!(
        special,
        vec![Special {
            path: "$.categories.bonus".into(),
            kind: "-0".into()
        }]
    );
    let captured = serde::Serialize::serialize(&f64::NAN, Capture).unwrap();
    let mut special = Vec::new();
    collect_special(&captured, "$", &mut special);
    assert_eq!(special[0].kind, "NaN");
}
fn compare<T: serde::Serialize>(
    id: &str,
    actual: PayrollProjectionOutcome<T>,
    expected: &Observation,
) {
    match actual {
        PayrollProjectionOutcome::Value(value) => {
            assert!(expected.ok, "{id}: unexpected value");
            let left = value.serialize(Capture).expect("capture original numbers");
            let mut special = Vec::new();
            collect_special(&left, "$", &mut special);
            assert_eq!(special, expected.special_numbers, "{id}: special numbers");
            let right: Ordered =
                serde_json::from_str(expected.value.as_ref().expect("value").get()).unwrap();
            assert_eq!(left, right, "{id}");
        }
        PayrollProjectionOutcome::SourceFailure(e) => {
            assert!(!expected.ok, "{id}: unexpected failure");
            let actual = Failure {
                name: e.name.into(),
                status: e.status,
                code: e.code.map(str::to_string),
                message: e.message,
            };
            assert_eq!(Some(&actual), expected.error.as_ref(), "{id}");
        }
        PayrollProjectionOutcome::CompatibilityGap {
            gate,
            operation,
            field_path,
        } => panic!("{id}: unexpected compatibility gap {gate} {operation} {field_path}"),
    }
}
#[test]
fn all_117_retained_observations() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/people_payroll/rust-accounting/projection.v1.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 117);
    for case in fixture.cases {
        let (items, accounts) =
            match read_payroll_projection_inputs(&case.items_json, &case.accounts_json) {
                PayrollProjectionOutcome::Value(v) => v,
                _ => panic!("{}: request input", case.id),
            };
        compare(
            &case.id,
            payroll_accounting_lines(&items, &accounts),
            &case.accounting,
        );
        assert_eq!(items.len(), case.shares.len());
        for share in case.shares {
            compare(
                &case.id,
                payroll_net_cost_shares(&items[share.index]),
                &Observation {
                    ok: share.ok,
                    value: share.value,
                    error: share.error,
                    special_numbers: share.special_numbers,
                },
            )
        }
    }
}
// Compile the actual private owner sources in this registered suite for their properties.
#[expect(
    dead_code,
    reason = "Actual private production modules included for property tests; library checked separately"
)]
#[path = "../../../packages/server-people_payroll/src/domain/payroll_accounting.rs"]
mod payroll_accounting;
#[path = "../../../packages/server-people_payroll/src/domain/payroll_accounting_collation.rs"]
mod payroll_accounting_collation;
#[expect(
    dead_code,
    reason = "Actual private production modules included for property tests; library checked separately"
)]
#[path = "../../../packages/server-people_payroll/src/domain/payroll_accounting_date.rs"]
mod payroll_accounting_date;
#[expect(
    dead_code,
    reason = "Actual private production modules included for property tests; library checked separately"
)]
#[path = "../../../packages/server-people_payroll/src/domain/payroll_accounting_operand.rs"]
mod payroll_accounting_operand;
mod allocation_properties {
    use super::payroll_accounting::*;
    use num_bigint::BigInt;
    use num_traits::{ToPrimitive, Zero};
    #[test]
    fn allocation_conservation_remainders_and_signed() {
        for len in 1..=3 {
            for mask in 0..6usize.pow(len as u32) {
                let mut x = mask;
                let mut weights = Vec::new();
                for _ in 0..len {
                    weights.push(BigInt::from(x % 6));
                    x /= 6
                }
                for amount in 0..=31 {
                    let outcome = allocate(amount as f64, &weights);
                    let sum: BigInt = weights.iter().sum();
                    if amount == 0 {
                        assert_eq!(outcome.unwrap(), vec![0.0; len]);
                        continue;
                    }
                    if sum.is_zero() {
                        assert!(outcome.is_err());
                        continue;
                    }
                    let result = outcome.unwrap();
                    assert_eq!(result.iter().sum::<f64>(), amount as f64);
                    let mut rank: Vec<(usize, BigInt)> = weights
                        .iter()
                        .enumerate()
                        .map(|(i, w)| (i, (BigInt::from(amount) * w) % &sum))
                        .collect();
                    rank.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                    let floor: Vec<f64> = weights
                        .iter()
                        .map(|w| ((BigInt::from(amount) * w) / &sum).to_f64().unwrap())
                        .collect();
                    let count = (amount as f64 - floor.iter().sum::<f64>()) as usize;
                    for (i, value) in result.iter().enumerate() {
                        assert_eq!(
                            *value,
                            floor[i]
                                + if rank[..count].iter().any(|v| v.0 == i) {
                                    1.0
                                } else {
                                    0.0
                                }
                        )
                    }
                }
            }
        }
        assert!(allocate(0.0, &[]).unwrap().is_empty());
        assert!(allocate(1.0, &[]).is_err());
        assert_eq!(
            allocate(1.0, &[(-1).into(), (-1).into(), 4.into()]).unwrap(),
            vec![0.0, 0.0, 2.0]
        );
        let huge: BigInt = BigInt::from(1u8) << 160usize;
        assert_eq!(
            allocate(17.0, &[huge.clone(), &huge * 2]).unwrap(),
            vec![6.0, 11.0]
        );
    }
}

mod operand_properties {
    use super::payroll_accounting_operand::*;
    #[test]
    fn source_round_and_lexical_numbers() {
        assert_eq!(js_round(0.5), 1.0);
        assert!(js_round(-0.5).is_sign_negative());
        assert_eq!(js_round(-1.5), -1.0);
        assert_eq!(js_round(f64::from_bits(0.5f64.to_bits() - 1)), 0.0);
        assert_eq!(js_round(f64::from_bits(0.5f64.to_bits() + 1)), 1.0);
        assert!(
            Operand::raw("1e400")
                .unwrap()
                .number()
                .unwrap()
                .is_infinite()
        );
        assert!(
            Operand::raw("-0")
                .unwrap()
                .number()
                .unwrap()
                .is_sign_negative()
        );
        let container = Operand::raw("{\"n\":1e400,\"z\":-0,\"s\":\"12\"}").unwrap();
        assert!(container.prop("n").unwrap().number().unwrap().is_infinite());
        assert!(
            container
                .prop("z")
                .unwrap()
                .number()
                .unwrap()
                .is_sign_negative()
        );
        let s = container.prop("s").unwrap();
        assert!(!s.actual_safe_integer());
        assert_eq!(s.number().unwrap(), 12.0);
        let t = Operand::raw("\"\\ud800\"").unwrap().string().unwrap();
        assert_eq!(t.json(), "\"\\ud800\"");
        assert_ne!(t, Operand::raw("\"\\ufffd\"").unwrap().string().unwrap());
        assert!(bigint(1.5).is_err());
        assert_eq!(
            bigint(2f64.powi(130)).unwrap(),
            num_bigint::BigInt::from(1u8) << 130
        );
    }
}

mod collation_properties {
    use super::payroll_accounting_collation::*;
    #[test]
    fn canonical_equal_retains_stable_order() {
        let c = collator().unwrap();
        assert_eq!(
            c.compare("[\"é\",null]", "[\"e\u{301}\",null]"),
            std::cmp::Ordering::Equal
        );
        let mut values = ["[\"é\",null]", "[\"e\u{301}\",null]"];
        values.sort_by(|a, b| c.compare(a, b));
        assert_eq!(values, ["[\"é\",null]", "[\"e\u{301}\",null]"]);
    }
}

#[test]
fn review_source_observations_and_explicit_profile_gaps() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/people_payroll/rust-accounting/review-probes.v1.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 8);
    for case in fixture.cases {
        let (items, accounts) =
            match read_payroll_projection_inputs(&case.items_json, &case.accounts_json) {
                PayrollProjectionOutcome::Value(v) => v,
                _ => panic!("{}: request input", case.id),
            };
        let accounting = payroll_accounting_lines(&items, &accounts);
        let shares = payroll_net_cost_shares(&items[0]);
        if case.id == "reached-noniterable-time-entries"
            || case.id == "non-json-whitespace-before-object"
        {
            for result in [
                matches!(
                    accounting,
                    PayrollProjectionOutcome::CompatibilityGap { gate: "P1", .. }
                ),
                matches!(
                    shares,
                    PayrollProjectionOutcome::CompatibilityGap { gate: "P1", .. }
                ),
            ] {
                assert!(result, "{}: expected explicit private P1 gap", case.id);
            }
        } else {
            compare(&case.id, accounting, &case.accounting);
            let share = case.shares.into_iter().next().unwrap();
            compare(
                &case.id,
                shares,
                &Observation {
                    ok: share.ok,
                    value: share.value,
                    error: share.error,
                    special_numbers: share.special_numbers,
                },
            );
        }
    }
}
