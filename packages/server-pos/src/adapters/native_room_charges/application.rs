//! Application port for the native POS room-charge reversal queue read.
//!
//! No SQL and no HTTP here: domain validates the bounded request and the
//! composition decisions, then delegates to the injected owner reader. The
//! application applies the purchase-history gate and derives every response
//! field, so the transport cannot add or drop a payload member.

use crate::adapters::native_room_charges::domain::{
    RoomChargeQuery, RoomChargeRow, forbidden, validate,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type RoomChargeFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition supplies the owner reader; the router never builds SQL or
/// derives a Company/Branch/Property scope itself.
pub trait PosRoomChargeReader: Send + Sync {
    fn rows<'a>(&'a self, query: &'a RoomChargeQuery) -> RoomChargeFuture<'a, Vec<RoomChargeRow>>;
}

/// Validate authority, scope and package gate before any owner read, then
/// return the complete row payload. A denied caller never reaches the pool.
pub async fn read<R: PosRoomChargeReader + ?Sized>(
    reader: &R,
    query: &RoomChargeQuery,
) -> Result<Vec<Value>, BusinessError> {
    if query.actor.department {
        return Err(forbidden(
            "POS_BRANCH_REQUIRED",
            "Select a Company or Branch.",
        ));
    }
    if !query.actor.pos_approve && !query.actor.pms_approve {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Pos or PMS approval permission is required.",
        ));
    }
    validate(query)?;
    if query.package.enforced && !query.package.history {
        return Ok(Vec::new());
    }
    let rows = reader.rows(query).await?;
    let purchased_settlement = !query.package.enforced || query.package.settle_existing;
    Ok(rows
        .into_iter()
        .map(|row| {
            row.value(
                query.actor.pos_approve,
                query.actor.pms_approve,
                &query.actor.self_ids,
                purchased_settlement,
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_room_charges::domain::{
        ActorDecision, PackageDecision, ROW_KEYS,
    };
    use std::task::{Context, Poll, Waker};

    /// The crate intentionally declares no direct tokio dev-dependency, so
    /// tests use a std-only executor rather than widening Cargo.toml.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    struct Probe {
        result: Result<Vec<RoomChargeRow>, BusinessError>,
    }

    impl PosRoomChargeReader for Probe {
        fn rows<'a>(&'a self, _: &'a RoomChargeQuery) -> RoomChargeFuture<'a, Vec<RoomChargeRow>> {
            Box::pin(async move { self.result.clone() })
        }
    }

    fn query() -> RoomChargeQuery {
        RoomChargeQuery {
            company: "co".into(),
            branch: Some("br".into()),
            property: Some("prop".into()),
            actor: ActorDecision {
                pos_approve: true,
                pms_approve: false,
                department: false,
                self_ids: Vec::new(),
            },
            package: PackageDecision {
                enforced: false,
                history: false,
                settle_existing: false,
            },
        }
    }

    fn row() -> RoomChargeRow {
        RoomChargeRow {
            id: "rc1".into(),
            company_id: "co".into(),
            branch_id: "br".into(),
            sale_id: "sale".into(),
            property_id: "prop".into(),
            sale_version: 2,
            property_version: 3,
            restock_line_ids_json: None,
            number: "POS-1".into(),
            state: "requested".into(),
            version: 1,
            reason: "Wrong room".into(),
            amount_minor: 10_000,
            requested_by: "requester".into(),
            pos_approved_by: None,
            created_by: "cashier".into(),
            charged_by: None,
            updated_at: 1_700_000_000_000,
            source_current: true,
        }
    }

    #[test]
    fn read_returns_full_payload_without_a_reader_scope() {
        let reader = Probe {
            result: Ok(vec![row()]),
        };
        let rows = block_on(read(&reader, &query())).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].as_object().unwrap().len(), ROW_KEYS.len());
        assert_eq!(rows[0]["stage"], "pos");
    }

    #[test]
    fn denied_authority_and_missing_history_never_read() {
        let mut denied = query();
        denied.actor.department = true;
        let reader = Probe {
            result: Ok(vec![row()]),
        };
        assert_eq!(
            block_on(read(&reader, &denied)).unwrap_err().code,
            "POS_BRANCH_REQUIRED"
        );

        let mut denied = query();
        denied.actor.pos_approve = false;
        assert_eq!(
            block_on(read(&reader, &denied)).unwrap_err().code,
            "PERMISSION_REQUIRED"
        );

        let gated = RoomChargeQuery {
            package: PackageDecision {
                enforced: true,
                history: false,
                settle_existing: true,
            },
            ..query()
        };
        assert!(block_on(read(&reader, &gated)).unwrap().is_empty());
    }
}
