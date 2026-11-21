//! Per-Company AI quota reservation/finish decisions.
use erpx_server_agent::public::quota::*;
use erpx_server_shared::public::BusinessError;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Default)]
struct Fake {
    reserve_result: bool,
    finish_result: bool,
    reservations: Vec<QuotaReservation>,
    finishes: Vec<QuotaFinish>,
}

impl CompanyQuotaPorts for Fake {
    type Error = BusinessError;

    async fn reserve(&mut self, reservation: &QuotaReservation) -> Result<bool, Self::Error> {
        self.reservations.push(reservation.clone());
        Ok(self.reserve_result)
    }
    async fn finish(&mut self, finish: &QuotaFinish) -> Result<bool, Self::Error> {
        self.finishes.push(finish.clone());
        Ok(self.finish_result)
    }
}

fn reservation() -> QuotaReservation {
    QuotaReservation::new(
        "request-1",
        "usage-1",
        "holding-1",
        "company-1",
        "openai",
        "model-1",
        "finance_proposal",
        10,
        0,
        1_000,
        500,
    )
    .unwrap()
}

#[test]
fn reservation_requires_bounded_scope_window_and_limit() {
    let valid = reservation();
    assert_eq!(valid.funding_source, FundingSource::Erpx);
    assert_eq!(valid.reservation_satang, 0);
    assert_eq!(
        QuotaReservation::new(
            "",
            "usage-1",
            "holding-1",
            "company-1",
            "openai",
            "model-1",
            "feature",
            10,
            0,
            1_000,
            500,
        )
        .unwrap_err()
        .code,
        "AI_QUOTA_INPUT_INVALID"
    );
    assert_eq!(
        QuotaReservation::new(
            "request-1",
            "usage-1",
            "holding-1",
            "company-1",
            "openai",
            "model-1",
            "feature",
            0,
            0,
            1_000,
            500,
        )
        .unwrap_err()
        .code,
        "AI_QUOTA_INPUT_INVALID"
    );
    assert_eq!(
        QuotaReservation::new(
            "request-1",
            "usage-1",
            "holding-1",
            "company-1",
            "openai",
            "model-1",
            "feature",
            10,
            1_000,
            1_000,
            500,
        )
        .unwrap_err()
        .code,
        "AI_QUOTA_INPUT_INVALID"
    );
}

#[tokio::test]
async fn reservations_map_exhaustion_to_the_retained_conflict() {
    let mut ports = Fake::default();
    let error = reserve_company_quota(&mut ports, &reservation())
        .await
        .unwrap_err();
    assert_eq!(error.code, "AI_QUOTA_EXHAUSTED");
    assert_eq!(ports.reservations.len(), 1);

    ports.reserve_result = true;
    let result = reserve_company_quota(&mut ports, &reservation())
        .await
        .unwrap();
    assert_eq!(
        result,
        json!({"id": "request-1", "status": "reserved", "reservationSatang": 0})
    );
}

#[tokio::test]
async fn finish_is_exactly_once_and_never_creates_a_reservation() {
    let finish = QuotaFinish {
        request_id: "request-1".into(),
        company_id: "company-1".into(),
        outcome: QuotaOutcome::Succeeded,
        input_tokens: 100,
        output_tokens: 50,
        units: BTreeMap::from([("image".into(), 1)]),
        now: 1_000,
    };
    let mut ports = Fake::default();
    assert_eq!(
        finish_company_quota(&mut ports, &finish)
            .await
            .unwrap_err()
            .code,
        "AI_QUOTA_RESERVATION_UNAVAILABLE"
    );
    ports.finish_result = true;
    let result = finish_company_quota(&mut ports, &finish).await.unwrap();
    assert_eq!(
        result,
        json!({
            "id": "request-1",
            "status": "succeeded",
            "inputTokens": 100,
            "outputTokens": 50,
        })
    );
    assert!(ports.reservations.is_empty());
    assert_eq!(ports.finishes.len(), 2);
}
