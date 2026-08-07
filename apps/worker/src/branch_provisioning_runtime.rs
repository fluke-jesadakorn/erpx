use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use erpx_infrastructure::{
    ApprovedDatabaseProfileConnection, SqlxBranchProvisioningStore, SqlxControlPlane,
    TenantDatabaseProvisioner,
};
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const BRANCH_PROVISIONING_HANDLER_NAME: &str = "branch_provisioning";

#[derive(Debug, Clone)]
pub struct BranchProvisioningRuntimeHandler {
    jobs: SqlxBranchProvisioningStore,
    databases: TenantDatabaseProvisioner,
}

impl BranchProvisioningRuntimeHandler {
    pub fn from_env(control: Arc<SqlxControlPlane>) -> Result<Self, WorkerRuntimeError> {
        let databases = TenantDatabaseProvisioner::for_approved_profiles(control.clone());
        Ok(Self {
            jobs: SqlxBranchProvisioningStore::new(control),
            databases,
        })
    }

    async fn fail_claim(
        &self,
        claim: &erpx_infrastructure::BranchProvisioningClaim,
        category: &'static str,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.jobs.fail(claim, category, Utc::now()).await?;
        Ok(WorkerHandlerReport {
            claimed: 1,
            has_work: true,
            ..WorkerHandlerReport::default()
        })
    }
}

#[async_trait]
impl NamedWorkerHandler for BranchProvisioningRuntimeHandler {
    fn name(&self) -> &str {
        BRANCH_PROVISIONING_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let Some(claim) = self.jobs.claim_next(worker_id, Utc::now()).await? else {
            return Ok(WorkerHandlerReport::default());
        };
        let registration = match self
            .jobs
            .approved_profile(&claim.route.tenant_id, &claim.route.profile_key)
            .await
        {
            Ok(profile) => profile,
            Err(_) => {
                return self
                    .fail_claim(&claim, "database_profile_unavailable")
                    .await;
            }
        };
        let profile = match ApprovedDatabaseProfileConnection::from_environment(
            &registration.profile_key,
            &registration.credential_profile_key,
        ) {
            Ok(profile) => profile,
            Err(_) => {
                return self
                    .fail_claim(&claim, "database_profile_unavailable")
                    .await;
            }
        };
        if self
            .databases
            .provision_branch_databases_with_profile(&claim.route, &profile)
            .await
            .is_err()
        {
            return self
                .fail_claim(&claim, "database_provisioning_failed")
                .await;
        }
        if self
            .databases
            .initialize_branch_organization_with_profile(
                &claim.route,
                &claim.job,
                worker_id,
                &profile,
            )
            .await
            .is_err()
        {
            return self
                .fail_claim(&claim, "organization_initialization_failed")
                .await;
        }
        self.jobs.mark_verifying(&claim, Utc::now()).await?;
        self.jobs.complete(&claim, Utc::now()).await?;
        Ok(WorkerHandlerReport {
            claimed: 1,
            processed: 1,
            published: 1,
            has_work: true,
            ..WorkerHandlerReport::default()
        })
    }
}
