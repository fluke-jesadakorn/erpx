//! Projects Postgres adapter for the handoff-view read.
//!
//! The SQL is executed here, in the owner adapter only: the lane 27
//! `PgProjectActualsRead::handoffs` statement selects the bounded
//! `staff_project_handoffs` rows for one Company and one Project, and
//! `read_visible_project` selects the current visible owner row. This module
//! maps those rows into the pure owner facts and applies no payload shaping.

use crate::adapters::native_lane_27::{
    ActualsScope, HandoffScope, PgProjectActualsRead, ProjectActualsRead, read_handoffs,
    read_visible_project,
};
use crate::application::project_handoffs::{HandoffViewFuture, HandoffViewRepository};
use crate::domain::project_handoffs::HandoffRow;
use erpx_server_runtime::public::postgres::DatabasePool;
use std::sync::Arc;

pub struct PgHandoffViews {
    reader: Arc<dyn ProjectActualsRead>,
}

impl PgHandoffViews {
    pub fn new(reader: Arc<dyn ProjectActualsRead>) -> Self {
        Self { reader }
    }

    pub fn from_pool(pool: DatabasePool) -> Self {
        Self::new(Arc::new(PgProjectActualsRead::new(pool)))
    }
}

impl HandoffViewRepository for PgHandoffViews {
    fn handoffs<'a>(
        &'a self,
        company: &'a str,
        project: &'a str,
    ) -> HandoffViewFuture<'a, Vec<HandoffRow>> {
        let reader = self.reader.clone();
        Box::pin(async move {
            let scope = HandoffScope {
                company: company.to_owned(),
                user: String::new(),
                project: project.to_owned(),
                handoff: String::new(),
            };
            let rows = read_handoffs(reader.as_ref(), &scope).await?;
            Ok(rows
                .into_iter()
                .map(|row| HandoffRow {
                    id: row.id,
                    company_id: row.company_id,
                    project_id: row.project_id,
                    source_project_version: row.source_project_version,
                    recipient_id: row.recipient_id,
                    status: row.status,
                    version: row.version,
                    snapshot_json: row.snapshot_json,
                    snapshot_sha256: row.snapshot_sha256,
                    evidence: row.evidence,
                    prepared_by: row.prepared_by,
                    prepared_at: row.prepared_at,
                    decided_by: row.decided_by,
                    decided_at: row.decided_at,
                    decision_reason: row.decision_reason,
                })
                .collect())
        })
    }

    fn project_owner<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        project: &'a str,
    ) -> HandoffViewFuture<'a, Option<String>> {
        let reader = self.reader.clone();
        Box::pin(async move {
            let scope = ActualsScope {
                company: company.to_owned(),
                user: user.to_owned(),
                project: project.to_owned(),
            };
            Ok(read_visible_project(reader.as_ref(), &scope)
                .await?
                .map(|row| row.owner_user_id))
        })
    }
}
