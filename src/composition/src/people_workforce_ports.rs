//! Composition implementations of the People workforce owner ports using the
//! Organization and Identity owner adapters. Each port runs inside the caller's
//! serializable Holding transaction; the account-link port also receives the
//! Control database pool for the verified account row.

use erpx_server_organization::public::people_staff_placement::{
    StaffPlacementRequest as OrganizationPlacementRequest, apply_staff_placement,
    into_business as placement_error,
};
use erpx_server_people_payroll::public::workforce::{
    PersonAccountLinkPort, PersonAccountLinkRequest, StaffPlacementPort,
    StaffPlacementRequest as PeoplePlacementRequest, WorkforceCommandFuture,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
use std::sync::Arc;

/// Identity account link/access revocation against the Control `"user"` row
/// and the Holding `assignments` rows.
pub struct IdentityAccountLinkPort {
    control: Arc<DatabasePool>,
}

impl IdentityAccountLinkPort {
    pub fn new(control: Arc<DatabasePool>) -> Self {
        Self { control }
    }
}

impl PersonAccountLinkPort for IdentityAccountLinkPort {
    fn verify<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: &'a PersonAccountLinkRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        let control = self.control.clone();
        Box::pin(async move {
            erpx_server_identity_access::public::people_account_link::verify_person_account_link(
                &control,
                connection,
                request.company_id.as_str(),
                request.user_id,
                request.email,
                request.person_id,
                request.now,
            )
            .await
            .map_err(erpx_server_identity_access::public::people_account_link::into_business)
        })
    }

    fn revoke_company_access<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        company_id: &'a erpx_server_shared::public::CompanyId,
        user_id: &'a str,
        person_id: &'a str,
        now: i64,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        let _ = person_id;
        Box::pin(async move {
            erpx_server_identity_access::public::people_account_link::revoke_company_access(
                connection,
                company_id.as_str(),
                user_id,
                now,
            )
            .await
            .map_err(erpx_server_identity_access::public::people_account_link::into_business)
        })
    }
}

/// Organization placement validation/history inside the caller transaction.
#[derive(Default)]
pub struct OrganizationStaffPlacementPort;

impl StaffPlacementPort for OrganizationStaffPlacementPort {
    fn apply<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: &'a PeoplePlacementRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let request = OrganizationPlacementRequest {
                company_id: request.company_id.as_str(),
                command_key: request.command_key,
                person_id: request.person_id,
                engagement_id: request.engagement_id,
                branch_id: request.branch_id,
                department_id: request.department_id,
                manager_person_id: request.manager_person_id,
                level_id: request.level_id,
                position_id: request.position_id,
                reason: request.reason,
                now: request.now,
            };
            apply_staff_placement(connection, &request)
                .await
                .map_err(placement_error)
        })
    }
}
