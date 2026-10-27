//! Original B0 pure data/control-flow oracle. Synthetic ports cannot establish
//! current worker/source/reservation authority or successful database effects.
use erpx_server_field_service::public::parts::*;
use serde_json::Value;
fn oracle() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/field_service/rust-parts/conformance.v1.json"
    )))
    .unwrap()
}
fn issues(failure: &FieldServiceWireFailure) -> Value {
    serde_json::json!(failure.issues.iter().map(|i|serde_json::json!({"code":i.code,"path":i.path.iter().map(|p|match p{FieldServiceIssuePath::Field(s)=>serde_json::json!(s),FieldServiceIssuePath::Index(n)=>serde_json::json!(n)}).collect::<Vec<_>>()})).collect::<Vec<_>>())
}
#[test]
fn exact_original_worker_reviewer_and_state_matrix() {
    let data = oracle();
    let rows = data["authority"].as_array().unwrap();
    assert_eq!(rows.len(), 288);
    for case in rows {
        let source = &case["source"];
        let command =
            normalize_field_service_parts(case["wire"].as_str().unwrap().as_bytes()).unwrap();
        let facts = FieldServicePartsAuthorityFacts {
            technician_user_id: source["technician_id"].as_str().unwrap(),
            actor_user_id: "actor",
            technician_eligible: source["technician_eligible"].as_i64().unwrap() > 0,
            reviewer_eligible: source["can_review"].as_i64().unwrap() > 0,
            selected_can_review: case["options"]["review"].as_bool().unwrap(),
            state: source["status"].as_str().unwrap(),
        };
        match require_parts_authority(&facts, command.input().action()) {
            Ok(decision) => assert_eq!(
                serde_json::json!({"own":decision.own,"manage":decision.manage}),
                case["decision"]["result"],
                "{}",
                case["id"]
            ),
            Err(error) => assert_eq!(
                error.code,
                case["decision"]["error"]["code"].as_str().unwrap(),
                "{}",
                case["id"]
            ),
        }
    }
}
#[test]
fn original_zod_parts_normalization_preserves_raw_receipt_identity_and_ordered_failures() {
    let data = oracle();
    let rows = data["wireCases"].as_array().unwrap();
    assert_eq!(rows.len(), 19);
    for case in rows {
        match normalize_field_service_parts(case["wire"].as_str().unwrap().as_bytes()) {
            Ok(value) => assert_eq!(
                value.receipt_input().as_str(),
                case["expected"]["normalized"].as_str().unwrap(),
                "{}",
                case["id"]
            ),
            Err(error) => {
                assert!(!error.invalid_json, "oracle wires are valid JSON");
                assert_eq!(
                    issues(&error),
                    case["expected"]["error"]["issues"],
                    "{}",
                    case["id"]
                );
            }
        }
    }
}
#[test]
fn original_parts_purchase_classification_is_closed_and_does_not_grant_finance_write() {
    let data = oracle();
    for case in data["purchase"].as_array().unwrap() {
        let action = if case["operation"] == "field.order.reserve-part" {
            FieldServicePartsActionKind::Reserve
        } else {
            FieldServicePartsActionKind::Issue
        };
        assert_eq!(action.operation(), case["operation"].as_str().unwrap());
        assert_eq!(
            parts_capabilities().as_slice(),
            case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["capability"].as_str().unwrap())
                .collect::<Vec<_>>()
                .as_slice()
        );
        assert!(
            case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .all(|x| x["purpose"] == "settle-existing")
        );
    }
    assert!(require_future_reservation(100, 100).is_err());
    require_future_reservation(101, 100).unwrap();
}

#[test]
fn original_new_command_preparation_keeps_reservation_quote_and_late_version_order() {
    use erpx_server_shared::public::{
        BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis,
    };
    use erpx_server_workflow::public::commands::CommandJson;
    use std::{
        future::Future,
        sync::Mutex,
        task::{Context, Poll, Waker},
    };
    struct Prepared {
        consumption: String,
        journal: String,
        value: i64,
    }
    struct Ports {
        options: Value,
        calls: Mutex<Vec<Value>>,
    }
    impl FieldServicePartsPreparationPorts for Ports {
        type Source = Value;
        type Reservation = String;
        type Consumption = Prepared;
        type Error = BusinessError;
        async fn require_action(
            &mut self,
            source: &Value,
            command: &FieldServicePartsCommand,
        ) -> Result<(), BusinessError> {
            require_parts_authority(
                &FieldServicePartsAuthorityFacts {
                    technician_user_id: source["technician_id"].as_str().unwrap(),
                    actor_user_id: "actor",
                    technician_eligible: source["technician_eligible"].as_i64().unwrap() > 0,
                    reviewer_eligible: source["can_review"].as_i64().unwrap() > 0,
                    selected_can_review: self.options["review"].as_bool().unwrap_or(false),
                    state: source["status"].as_str().unwrap(),
                },
                command.input().action(),
            )?;
            Ok(())
        }
        fn application_now(&self) -> TimestampMillis {
            self.calls
                .lock()
                .unwrap()
                .push(serde_json::json!(["clock"]));
            TimestampMillis::new(1_800_000_000_100)
        }
        async fn reservation(
            &mut self,
            source: &Value,
            id: &str,
        ) -> Result<Option<String>, BusinessError> {
            self.calls
                .lock()
                .unwrap()
                .push(serde_json::json!(["reservation", source["id"], id]));
            Ok((self.options["noReservation"] != true).then(|| id.into()))
        }
        async fn prepare_consumption(
            &mut self,
            source: &Value,
            reservation: &String,
            command: &FieldServicePartsCommand,
            at: TimestampMillis,
        ) -> Result<Prepared, BusinessError> {
            self.calls.lock().unwrap().push(serde_json::json!([
                "prepareConsumption",
                source["id"],
                reservation,
                command.receipt_input().as_str(),
                command.key().as_str(),
                at.get()
            ]));
            if let Some(value) = self.options["consumptionError"].as_str() {
                let code = match value {
                    "MANUAL_RATE_REQUIRED" => "MANUAL_RATE_REQUIRED",
                    "PART_STOCK_UNAVAILABLE" => "PART_STOCK_UNAVAILABLE",
                    _ => panic!("unknown original oracle failure"),
                };
                return Err(BusinessError::new(
                    if code == "MANUAL_RATE_REQUIRED" {
                        FailureKind::InvalidInput
                    } else {
                        FailureKind::Conflict
                    },
                    code,
                    "Synthetic original preparation refusal",
                ));
            }
            Ok(Prepared {
                consumption: "operational-part-original-key".into(),
                journal: "journal-original-key:parts".into(),
                value: 200,
            })
        }
        fn consumption_facts(value: &Prepared) -> FieldServicePreparedConsumptionFacts<'_> {
            FieldServicePreparedConsumptionFacts {
                consumption_id: &value.consumption,
                journal_id: &value.journal,
                value_minor: value.value,
            }
        }
    }
    let data = oracle();
    let mut observed = 0;
    for case in data["flow"].as_array().unwrap() {
        // Current auth, normalization, exact replay and absent scoped-source handling
        // belong to the real Service/Workflow boundary, not this new-only phase.
        if case["source"].is_null() {
            continue;
        }
        let command = normalize_field_service_parts(case["wire"].as_str().unwrap().as_bytes())
            .unwrap()
            .into_command(
                CommandKey::new("original-key").unwrap(),
                RecordId::new("order").unwrap(),
            );
        let mut ports = Ports {
            options: case["options"].clone(),
            calls: Mutex::new(vec![]),
        };
        let result = {
            let mut future = Box::pin(prepare_new_field_service_parts(
                &mut ports,
                &case["source"],
                &command,
            ));
            match future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(value) => value,
                Poll::Pending => panic!("synthetic pure ports must be immediately ready"),
            }
        };
        let expected = case["expected"]["calls"].as_array().unwrap();
        let first = expected.iter().position(|call| call[0] == "order").unwrap() + 1;
        let final_call = expected
            .iter()
            .position(|call| call[0] == "commit")
            .unwrap_or(expected.len());
        assert_eq!(
            ports.calls.into_inner().unwrap(),
            expected[first..final_call],
            "{}",
            case["id"]
        );
        match result {
            Ok(plan) => {
                assert!(case["expected"]["error"].is_null(), "{}", case["id"]);
                let transition =
                    CommandJson::parse(expected[final_call][5].as_str().unwrap()).unwrap();
                assert_eq!(
                    plan.details(),
                    &transition.property("details").unwrap().unwrap(),
                    "{}",
                    case["id"]
                );
                assert_eq!(plan.command().receipt_input(), command.receipt_input());
            }
            Err(error) => assert_eq!(
                error.code,
                case["expected"]["error"]["code"].as_str().unwrap(),
                "{}",
                case["id"]
            ),
        }
        observed += 1;
    }
    assert_eq!(observed, 7);
}
