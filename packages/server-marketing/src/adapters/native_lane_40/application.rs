//! Owner-local read use case for lane 40. It owns validation, the closed read
//! kind dispatch and the response invariants. It contains no SQL and opens no
//! connection; the injected repository executes one exact owner-local query.
use crate::adapters::native_lane_40::domain::{
    AttributionRow, Lane40ReadInput, Lane40ReadResult, Lane40Request, RD_CAMPAIGN_SUMMARY,
    check_attribution, check_result, invalid, request,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type Lane40ReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner repository. Composition may inject any implementation; the only
/// production one is the sqlx-backed `PgLane40Reader`. No SQL crosses this
/// port, and the caller never supplies query text.
pub trait Lane40Reader: Send + Sync {
    fn read<'a>(&'a self, request: Lane40Request) -> Lane40ReadFuture<'a, Lane40ReadResult>;
}

/// One validated read, then the owner rechecks company scope and any calculation
/// invariant before the result is allowed to leave the owner crate.
pub async fn read<R: Lane40Reader + ?Sized>(
    reader: &R,
    input: &Lane40ReadInput,
    company_id: &str,
) -> Result<Lane40ReadResult, BusinessError> {
    let request = request(input, company_id)?;
    let result = reader.read(request).await?;
    check_result(company_id, &result)?;
    if result.kind == RD_CAMPAIGN_SUMMARY {
        for row in &result.rows {
            let attribution: AttributionRow =
                serde_json::from_value(row.clone()).map_err(|_| invalid())?;
            check_attribution(&attribution)?;
        }
    }
    Ok(result)
}

/// Pure helper used by the postgres adapter and by unit tests to pin the exact
/// aggregate projection shape without opening a connection.
pub fn attribution_json(
    company_id: &str,
    campaign: &str,
    source: &str,
    captured: i64,
    pending: i64,
    handed_off: i64,
    rejected: i64,
) -> Value {
    serde_json::json!({
        "company_id": company_id,
        "campaign": campaign,
        "source": source,
        "captured": captured,
        "pending": pending,
        "handed_off": handed_off,
        "rejected": rejected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_40::domain::{
        KINDS, RD_CAMPAIGN_SUMMARY, RD_CANDIDATE_DETAIL, UNSPECIFIED,
    };

    struct Fixed {
        rows: Vec<Value>,
    }
    impl Lane40Reader for Fixed {
        fn read<'a>(&'a self, request: Lane40Request) -> Lane40ReadFuture<'a, Lane40ReadResult> {
            let kind = request.kind;
            let company_id = request.company_id;
            let rows = self.rows.clone();
            Box::pin(async move {
                Ok(Lane40ReadResult {
                    kind,
                    company_id,
                    rows,
                })
            })
        }
    }

    fn input(kind: &str) -> Lane40ReadInput {
        Lane40ReadInput {
            kind: kind.to_owned(),
            branch_id: None,
            department_id: None,
            record_id: Some("marketing-candidate-1".into()),
            enforced: false,
            limit: None,
        }
    }

    #[tokio::test]
    async fn read_returns_the_owner_projection_for_every_kind() {
        for kind in KINDS {
            let row = if *kind == RD_CAMPAIGN_SUMMARY {
                attribution_json("company-1", UNSPECIFIED, UNSPECIFIED, 1, 1, 0, 0)
            } else {
                serde_json::json!({"id": "row-1", "company_id": "company-1"})
            };
            let reader = Fixed { rows: vec![row] };
            let result = read(&reader, &input(kind), "company-1").await.unwrap();
            assert_eq!(result.kind, *kind);
            assert_eq!(result.rows.len(), 1);
        }
    }

    #[tokio::test]
    async fn read_fails_closed_on_a_cross_company_row() {
        let reader = Fixed {
            rows: vec![serde_json::json!({"id": "row-1", "company_id": "company-2"})],
        };
        assert_eq!(
            read(&reader, &input(RD_CANDIDATE_DETAIL), "company-1")
                .await
                .unwrap_err()
                .code,
            "MARKETING_LANE40_READ_UNAVAILABLE"
        );
    }

    #[tokio::test]
    async fn read_fails_closed_on_a_broken_aggregate() {
        let reader = Fixed {
            rows: vec![attribution_json(
                "company-1",
                UNSPECIFIED,
                UNSPECIFIED,
                3,
                1,
                1,
                0,
            )],
        };
        assert_eq!(
            read(&reader, &input(RD_CAMPAIGN_SUMMARY), "company-1")
                .await
                .unwrap_err()
                .code,
            "MARKETING_LANE40_READ_UNAVAILABLE"
        );
    }
}
