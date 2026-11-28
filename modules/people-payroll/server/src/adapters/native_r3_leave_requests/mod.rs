//! Native R3 owner slice for `.../leave-requests` and its six subroutes.
//!
//! The paged own-leave read is new owner SQL (this module). The single-request
//! read, create/edit, preview, transition, cancellation review and approval
//! already have real SQLx in `adapters/leave.rs` behind `application/leave.rs`;
//! this module adds only the missing HTTP transport and composition ports, so
//! it does not duplicate that SQL. No TypeScript SQL or fallback is added.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    LeaveActor, LeaveFuture, LeaveRequestsBackend, LeaveRequestsRead,
};
pub use domain::{
    LeaveCommandKind, LeaveCursor, LeaveListEnvelope, LeaveListFilter, LeaveListRow,
};
pub use http::{LeaveRequestsTarget, router, router_with_reader};
pub use postgres::{PgLeaveRequestsRead, SQL_LEAVE_LIST_BASE, SQL_ORACLE_SHA256, list_sql};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_ports_and_dto_are_pure() {
        let filter = LeaveListFilter::parse(Some("closed")).unwrap();
        assert!(filter.clause().contains("NOT IN"));
        let cursor = LeaveCursor { at: 42, id: "leave-1".into() };
        assert_eq!(LeaveCursor::decode(&cursor.encode()).unwrap().id, "leave-1");
        let envelope = LeaveListEnvelope {
            data: Vec::<LeaveListRow>::new(),
            has_more: false,
            next_cursor: None,
        };
        assert_eq!(
            serde_json::to_string(&envelope).unwrap(),
            r#"{"data":[],"hasMore":false,"nextCursor":null}"#
        );
    }
}
