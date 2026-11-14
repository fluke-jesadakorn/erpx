//! Production auxiliary-read owner adapter.
//!
//! The composition HTTP backend resolves the current Account proof and selected
//! Support scope, then constructs this adapter. It runs the existing owner reads
//! on the caller connection and never opens a transaction or mints an actor.
#![allow(async_fn_in_trait)]

use crate::adapters::support_auxiliary::{
    SupportAlertSourceSnapshot, SupportAuxiliaryScope, SupportIncidentSourceSnapshot,
    SupportPolicySourceSnapshot, SupportSalesWorkspace, SupportSessionSourceSnapshot,
    SupportTicketSourceSnapshot, read_alert_source, read_incident_source, read_policy_source,
    read_sales_workspace, read_session_source, read_ticket_source,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_READ_UNAVAILABLE",
            "The Support record is unavailable.",
        ),
    }
}

pub trait SupportAuxiliaryReadPorts {
    async fn workspace(&mut self) -> Result<SupportSalesWorkspace, BusinessError>;
    async fn ticket(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportTicketSourceSnapshot>, BusinessError>;
    async fn policy(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportPolicySourceSnapshot>, BusinessError>;
    async fn alert(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportAlertSourceSnapshot>, BusinessError>;
    async fn incident(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportIncidentSourceSnapshot>, BusinessError>;
    async fn session(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportSessionSourceSnapshot>, BusinessError>;
}

pub struct PgSupportAuxiliaryReads<'a> {
    connection: &'a mut PgConnection,
    scope: SupportAuxiliaryScope,
}

impl<'a> PgSupportAuxiliaryReads<'a> {
    pub fn new(connection: &'a mut PgConnection, scope: SupportAuxiliaryScope) -> Self {
        Self { connection, scope }
    }

    pub fn scope(&self) -> &SupportAuxiliaryScope {
        &self.scope
    }
}

impl SupportAuxiliaryReadPorts for PgSupportAuxiliaryReads<'_> {
    async fn workspace(&mut self) -> Result<SupportSalesWorkspace, BusinessError> {
        read_sales_workspace(self.connection, &self.scope)
            .await
            .map_err(business)
    }

    async fn ticket(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportTicketSourceSnapshot>, BusinessError> {
        read_ticket_source(self.connection, &self.scope.read, id)
            .await
            .map_err(business)
    }

    async fn policy(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportPolicySourceSnapshot>, BusinessError> {
        read_policy_source(self.connection, self.scope.read.company_id.as_str(), id)
            .await
            .map_err(business)
    }

    async fn alert(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportAlertSourceSnapshot>, BusinessError> {
        read_alert_source(self.connection, &self.scope.read, id)
            .await
            .map_err(business)
    }

    async fn incident(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportIncidentSourceSnapshot>, BusinessError> {
        read_incident_source(self.connection, &self.scope.read, id)
            .await
            .map_err(business)
    }

    async fn session(
        &mut self,
        id: &str,
    ) -> Result<Option<SupportSessionSourceSnapshot>, BusinessError> {
        read_session_source(self.connection, &self.scope.read, id)
            .await
            .map_err(business)
    }
}
