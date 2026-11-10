//! Production Support ticket-portal owner adapter.
//!
//! Runs on the caller's serializable Holding connection. Current IAM/scope is
//! rechecked here; Commercial package purposes, replay and receipt/audit/outbox
//! retention run in this adapter without importing composition modules.
use crate::adapters::support_command_receipt::{
    SupportCommandReceipt, replay_support_receipt, retain_support_receipt, support_request_hash,
};
use crate::adapters::support_ticket_read::SupportReadScope;
use crate::adapters::ticket_portal::{
    acknowledgement_prior, apply_customer_acknowledgement, apply_plan, binding_for_staff,
    binding_image, current_response, customer_binding, decision_replay_proof, guard_binding_image,
    guard_customer_binding, lock_ticket_portal_sources, portal_installed,
    record_customer_projection, response_replay_proof, staff_workspace, ticket_for_staff,
    ticket_image,
};
use crate::application::ticket_portal::{
    PortalPurpose, PortalSourceImage, PriorAcknowledgement, TicketPortalPorts,
};
use crate::domain::ticket_portal::{
    PortalActor, PortalBindingRow, PortalCustomer, PortalPlan, PortalProjection, PortalResponse,
    PortalRights, PortalTicket, PortalWorkspace,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::PgConnection;
use std::collections::BTreeSet;

type Error = PgTransactionError<BusinessError>;

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_PORTAL_UNAVAILABLE",
            "Support Portal storage is temporarily unavailable.",
        ),
    }
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SupportCapability {
    Tickets,
    Portal,
}

impl SupportCapability {
    fn package(self) -> &'static str {
        match self {
            Self::Tickets => "support",
            Self::Portal => "support-portal",
        }
    }
}

pub struct PgTicketPortal<'a> {
    connection: &'a mut PgConnection,
    authority_json: String,
    now: i64,
}

impl<'a> PgTicketPortal<'a> {
    pub fn new(
        connection: &'a mut PgConnection,
        authority_json: impl Into<String>,
        now: i64,
    ) -> Self {
        Self {
            connection,
            authority_json: authority_json.into(),
            now,
        }
    }

    fn scope(&self, actor: &PortalActor) -> SupportReadScope {
        SupportReadScope {
            company_id: CompanyId::new(actor.company_id.clone()).expect("verified company"),
            branch_id: actor.branch_id.clone(),
            department_id: actor.department_id.clone(),
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            role: actor.role.clone(),
            support_read: true,
            records_read: true,
            support_create: false,
            support_work: false,
            support_manage: actor.support_manage,
            commerce_read: false,
        }
    }

    async fn active_assignment(
        &mut self,
        actor: &PortalActor,
        permission: &'static str,
    ) -> Result<bool, BusinessError> {
        let sql = format!(
            "SELECT 1 FROM assignments a WHERE a.id=$1 AND a.user_id=$2 \
             AND a.company_id=$3 \
             AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_role_allows(a.role,a.company_id,'{permission}') \
             AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$4) \
               OR (a.scope_kind='department' AND a.scope_node_id=$5))"
        );
        sqlx::query_scalar::<_, i32>(sqlx::AssertSqlSafe(sql))
            .bind(&actor.assignment_id)
            .bind(&actor.user_id)
            .bind(&actor.company_id)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await
            .map(|row| row.is_some())
            .map_err(Error::from)
            .map_err(business)
    }

    async fn package_allowed(
        &mut self,
        company_id: &str,
        capability: SupportCapability,
        purpose: PortalPurpose,
    ) -> Result<bool, BusinessError> {
        let package = capability.package();
        let dependency = if capability == SupportCapability::Portal {
            "AND EXISTS(SELECT 1 FROM sources parent WHERE parent.package_id='support' AND parent.package_version=1)"
        } else {
            ""
        };
        let sql = match purpose {
            PortalPurpose::NewWork => format!(
                "WITH sources AS (SELECT package_id,package_version FROM company_package_grants \
                 WHERE company_id=$1 AND starts_at<=$2 AND state='active' \
                   AND (ends_at IS NULL OR ends_at>$2)) \
                 SELECT EXISTS(SELECT 1 FROM sources purchase \
                   WHERE purchase.package_id=$3 AND purchase.package_version=1 {dependency})"
            ),
            PortalPurpose::SettleExisting => format!(
                "WITH sources AS (SELECT package_id,package_version FROM company_package_grants \
                 WHERE company_id=$1 AND starts_at<=$2 AND state<>'inactive') \
                 SELECT EXISTS(SELECT 1 FROM sources purchase \
                   WHERE purchase.package_id=$3 AND purchase.package_version=1 {dependency})"
            ),
            PortalPurpose::History => format!(
                "WITH sources AS (SELECT package_id,package_version FROM company_package_grants \
                 WHERE company_id=$1 AND starts_at<=$2 AND state<>'inactive' \
                 UNION SELECT package_id,package_version FROM company_package_history_rights \
                 WHERE company_id=$1 AND retained_at<=$2 AND retained_at>=0 \
                   AND retained_at<=9007199254740991 AND source_grant_version>0 \
                   AND source_event_id<>'') \
                 SELECT EXISTS(SELECT 1 FROM sources purchase \
                   WHERE purchase.package_id=$3 AND purchase.package_version=1 {dependency})"
            ),
        };
        sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(sql))
            .bind(company_id)
            .bind(self.now)
            .bind(package)
            .fetch_one(&mut *self.connection)
            .await
            .map_err(Error::from)
            .map_err(business)
    }

    async fn require_capability(
        &mut self,
        company_id: &str,
        purpose: PortalPurpose,
    ) -> Result<(), BusinessError> {
        for capability in [SupportCapability::Tickets, SupportCapability::Portal] {
            if !self
                .package_allowed(company_id, capability, purpose)
                .await?
            {
                return Err(forbidden(
                    "PACKAGE_REQUIRED",
                    "The Company purchase does not permit this Support Portal capability.",
                ));
            }
        }
        Ok(())
    }

    async fn enforce_staff(
        &mut self,
        actor: &PortalActor,
        manage: bool,
    ) -> Result<(), BusinessError> {
        if !self.active_assignment(actor, "support.read").await? {
            return Err(forbidden(
                "SUPPORT_READ_REQUIRED",
                "Support read access is required.",
            ));
        }
        if manage && !self.active_assignment(actor, "support.manage").await? {
            return Err(forbidden(
                "SUPPORT_MANAGE_REQUIRED",
                "A current Support manager is required.",
            ));
        }
        Ok(())
    }

    pub async fn lock_sources(&mut self) -> Result<(), BusinessError> {
        lock_ticket_portal_sources(self.connection)
            .await
            .map_err(business)
    }

    pub async fn staff_workspace(
        &mut self,
        actor: &PortalActor,
        ticket_id: Option<&str>,
        binding_id: Option<&str>,
        rights: &PortalRights,
    ) -> Result<PortalWorkspace, BusinessError> {
        self.enforce_staff(actor, false).await?;
        self.require_capability(&actor.company_id, PortalPurpose::History)
            .await?;
        let scope = self.scope(actor);
        staff_workspace(
            self.connection,
            &scope,
            actor,
            ticket_id,
            binding_id,
            rights,
            self.now,
        )
        .await
        .map_err(business)
    }

    pub async fn projection(
        &mut self,
        customer: &PortalCustomer,
    ) -> Result<PortalProjection, BusinessError> {
        let companies: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT company_id FROM support_ticket_portal_bindings \
             WHERE email=$1 AND status='approved' AND expires_at>$2",
        )
        .bind(customer.normalized_email())
        .bind(self.now)
        .fetch_all(&mut *self.connection)
        .await
        .map_err(Error::from)
        .map_err(business)?;
        let mut allowed = BTreeSet::new();
        for company in companies {
            if self
                .package_allowed(&company, SupportCapability::Tickets, PortalPurpose::History)
                .await?
                && self
                    .package_allowed(&company, SupportCapability::Portal, PortalPurpose::History)
                    .await?
            {
                allowed.insert(company);
            }
        }
        record_customer_projection(self.connection, customer, self.now, |company| {
            allowed.contains(company)
        })
        .await
        .map_err(business)
    }

    pub async fn acknowledgement(
        &mut self,
        customer: &PortalCustomer,
        id: &str,
        raw: &Value,
        key: &str,
    ) -> Result<Value, BusinessError> {
        crate::application::ticket_portal::acknowledge(self, customer, id, raw, key).await
    }

    fn request_hash(
        &self,
        actor: &PortalActor,
        operation: &str,
        record_id: &str,
        input: &Value,
    ) -> String {
        support_request_hash(
            &actor.user_id,
            &actor.assignment_id,
            &actor.role,
            &actor.company_id,
            actor.branch_id.as_deref(),
            actor.department_id.as_deref(),
            operation,
            record_id,
            input,
        )
    }
}

impl TicketPortalPorts for PgTicketPortal<'_> {
    fn now(&self) -> i64 {
        self.now
    }

    async fn authorize_staff(
        &mut self,
        actor: &PortalActor,
        manage: bool,
    ) -> Result<(), BusinessError> {
        self.enforce_staff(actor, manage).await
    }

    async fn authorize_capability(
        &mut self,
        company_id: &str,
        purpose: PortalPurpose,
    ) -> Result<(), BusinessError> {
        self.require_capability(company_id, purpose).await
    }

    async fn rights(&mut self, actor: &PortalActor) -> Result<PortalRights, BusinessError> {
        let tickets_new = self
            .package_allowed(
                &actor.company_id,
                SupportCapability::Tickets,
                PortalPurpose::NewWork,
            )
            .await?;
        let portal_new = self
            .package_allowed(
                &actor.company_id,
                SupportCapability::Portal,
                PortalPurpose::NewWork,
            )
            .await?;
        let tickets_settle = self
            .package_allowed(
                &actor.company_id,
                SupportCapability::Tickets,
                PortalPurpose::SettleExisting,
            )
            .await?;
        let portal_settle = self
            .package_allowed(
                &actor.company_id,
                SupportCapability::Portal,
                PortalPurpose::SettleExisting,
            )
            .await?;
        Ok(PortalRights {
            new_work: tickets_new && portal_new,
            settle_existing: tickets_settle && portal_settle,
        })
    }

    async fn installed(&mut self) -> Result<bool, BusinessError> {
        portal_installed(self.connection).await.map_err(business)
    }

    async fn replay(
        &mut self,
        actor: &PortalActor,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &Value,
    ) -> Result<Option<Value>, BusinessError> {
        let hash = self.request_hash(actor, operation, record_id, input);
        replay_support_receipt(
            self.connection,
            key,
            operation,
            &actor.user_id,
            record_id,
            &hash,
        )
        .await
        .map_err(business)
    }

    async fn replay_proof(
        &mut self,
        actor: &PortalActor,
        key: &str,
        binding_id: &str,
        action: &str,
        result: &Value,
    ) -> Result<(), BusinessError> {
        if action == "respond" {
            response_replay_proof(self.connection, actor, binding_id, key, result)
                .await
                .map_err(business)
        } else {
            decision_replay_proof(self.connection, actor, binding_id, key, action, result)
                .await
                .map_err(business)
        }
    }

    async fn ticket(
        &mut self,
        actor: &PortalActor,
        id: &str,
    ) -> Result<PortalTicket, BusinessError> {
        let scope = self.scope(actor);
        ticket_for_staff(self.connection, &scope, id)
            .await
            .map_err(business)?
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "SUPPORT_PORTAL_TICKET_NOT_FOUND",
                    "Support ticket is unavailable.",
                )
            })
    }

    async fn binding(
        &mut self,
        actor: &PortalActor,
        id: &str,
    ) -> Result<Option<PortalBindingRow>, BusinessError> {
        let scope = self.scope(actor);
        binding_for_staff(self.connection, &scope, id)
            .await
            .map_err(business)
    }

    async fn current_response(
        &mut self,
        binding_id: &str,
    ) -> Result<Option<PortalResponse>, BusinessError> {
        current_response(self.connection, binding_id)
            .await
            .map_err(business)
    }

    async fn snapshot(
        &mut self,
        binding_ids: &[String],
        ticket_id: Option<&str>,
    ) -> Result<PortalSourceImage, BusinessError> {
        let bindings = binding_image(self.connection, binding_ids)
            .await
            .map_err(business)?;
        let ticket = match ticket_id {
            Some(ticket_id) => ticket_image(self.connection, ticket_id)
                .await
                .map_err(business)?,
            None => None,
        };
        Ok(PortalSourceImage { bindings, ticket })
    }

    async fn apply(
        &mut self,
        actor: &PortalActor,
        key: &str,
        operation: &str,
        plan: &PortalPlan,
        input: &Value,
        result: &Value,
    ) -> Result<(), BusinessError> {
        let (record_id, version, state) = match plan {
            PortalPlan::Prepare {
                binding, result, ..
            } => (binding.id.as_str(), result.version, "draft"),
            PortalPlan::Decide { event, result, .. } => (
                event.binding_id.as_str(),
                result.version,
                result.status.as_str(),
            ),
            PortalPlan::Respond {
                response, result, ..
            } => (response.binding_id.as_str(), result.version, "published"),
            PortalPlan::Acknowledge { .. } => {
                return apply_customer_acknowledgement(self.connection, plan)
                    .await
                    .map_err(business);
            }
        };
        apply_plan(self.connection, actor, plan)
            .await
            .map_err(business)?;
        let receipt = SupportCommandReceipt {
            key,
            actor_id: &actor.user_id,
            assignment_id: &actor.assignment_id,
            role: &actor.role,
            company_id: &actor.company_id,
            branch_id: actor.branch_id.as_deref(),
            department_id: actor.department_id.as_deref(),
            operation,
            record_id,
            record_type: "support_ticket_portal",
            aggregate_type: "support_ticket_portal",
            version,
            state,
            input,
            result,
            authority_json: &self.authority_json,
            created_at: self.now,
        };
        retain_support_receipt(self.connection, &receipt)
            .await
            .map_err(business)
    }

    async fn guard(
        &mut self,
        actor: &PortalActor,
        binding_ids: &[String],
        ticket_id: Option<&str>,
        purpose: PortalPurpose,
        expected: &PortalSourceImage,
    ) -> Result<(), BusinessError> {
        self.enforce_staff(actor, true).await?;
        self.require_capability(&actor.company_id, purpose).await?;
        let scope = self.scope(actor);
        for binding_id in binding_ids {
            if binding_for_staff(self.connection, &scope, binding_id)
                .await
                .map_err(business)?
                .is_none()
            {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_PORTAL_SOURCE_CHANGED",
                    "The original ticket or verified customer access changed before completion.",
                ));
            }
        }
        guard_binding_image(
            self.connection,
            binding_ids,
            ticket_id,
            &(expected.bindings.clone(), expected.ticket.clone()),
        )
        .await
        .map_err(business)
    }

    async fn customer_binding(
        &mut self,
        customer: &PortalCustomer,
        id: &str,
    ) -> Result<PortalBindingRow, BusinessError> {
        let binding = customer_binding(self.connection, customer, id, self.now)
            .await
            .map_err(business)?;
        self.require_capability(&binding.company_id, PortalPurpose::History)
            .await?;
        Ok(binding)
    }

    async fn acknowledgement_prior(
        &mut self,
        key: &str,
    ) -> Result<Option<PriorAcknowledgement>, BusinessError> {
        acknowledgement_prior(self.connection, key)
            .await
            .map_err(business)
            .map(|value| {
                value.map(
                    |(binding_id, user_id, request_hash, result)| PriorAcknowledgement {
                        binding_id,
                        user_id,
                        request_hash,
                        result,
                    },
                )
            })
    }

    async fn apply_acknowledgement(
        &mut self,
        _customer: &PortalCustomer,
        _key: &str,
        plan: &PortalPlan,
        _input: &Value,
        _result: &Value,
    ) -> Result<(), BusinessError> {
        apply_customer_acknowledgement(self.connection, plan)
            .await
            .map_err(business)
    }

    async fn guard_acknowledgement(
        &mut self,
        customer: &PortalCustomer,
        binding_id: &str,
        response_id: &str,
    ) -> Result<(), BusinessError> {
        guard_customer_binding(self.connection, customer, binding_id, response_id, self.now)
            .await
            .map_err(business)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_portal_requires_both_package_capabilities() {
        assert_eq!(SupportCapability::Tickets.package(), "support");
        assert_eq!(SupportCapability::Portal.package(), "support-portal");
    }
}
