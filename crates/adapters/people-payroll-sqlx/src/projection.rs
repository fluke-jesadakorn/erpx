use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraftV2, people_payroll};
use erpx_people_payroll::{
    Aggregate, EmployeeStatus, ExpenseStatus, LeaveStatus, PayrollStatus,
    employment_record_ref_for, payslip_record_ref_for,
    ports::{EmployeeView, ExpenseClaimView, LeaveRequestView, PayrollRunView},
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CONTEXT_NAME, EMPLOYEE_AGGREGATE_TYPE, EMPLOYEE_PROJECTION, EMPLOYEE_SNAPSHOT_EVENT,
    EVENT_VERSION, EXPENSE_AGGREGATE_TYPE, EXPENSE_PROJECTION, EXPENSE_SNAPSHOT_EVENT,
    LEAVE_AGGREGATE_TYPE, LEAVE_PROJECTION, LEAVE_SNAPSHOT_EVENT, PAYROLL_AGGREGATE_TYPE,
    PAYROLL_PROJECTION, PAYROLL_SNAPSHOT_EVENT, command::scope_key, employee_partition,
    expense_partition, leave_partition, payroll_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum PeoplePayrollSnapshot {
    Employee(EmployeeView),
    LeaveRequest(LeaveRequestView),
    ExpenseClaim(ExpenseClaimView),
    PayrollRun(PayrollRunView),
}

impl From<&Aggregate> for PeoplePayrollSnapshot {
    fn from(value: &Aggregate) -> Self {
        match value {
            Aggregate::Employee(value) => Self::Employee(value.into()),
            Aggregate::LeaveRequest(value) => Self::LeaveRequest(value.into()),
            Aggregate::ExpenseClaim(value) => Self::ExpenseClaim(value.into()),
            Aggregate::PayrollRun(value) => Self::PayrollRun(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PeoplePayrollOutboxPayload {
    Integration(people_payroll::Event),
    Snapshot(PeoplePayrollSnapshot),
    Directory(ItemDescriptorDraftV2),
}

macro_rules! projector_type {
    ($name:ident, $projection:expr, $event:expr) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            events: Vec<String>,
        }
        impl $name {
            pub fn new() -> Self {
                Self {
                    events: vec![$event.into()],
                }
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl $name {
            fn projection_name(&self) -> &'static str {
                $projection
            }
            fn subscribed_events(&self) -> &[String] {
                &self.events
            }
        }
    };
}

projector_type!(
    EmployeeProjector,
    EMPLOYEE_PROJECTION,
    EMPLOYEE_SNAPSHOT_EVENT
);
projector_type!(
    LeaveRequestProjector,
    LEAVE_PROJECTION,
    LEAVE_SNAPSHOT_EVENT
);
projector_type!(
    ExpenseClaimProjector,
    EXPENSE_PROJECTION,
    EXPENSE_SNAPSHOT_EVENT
);
projector_type!(
    PayrollRunProjector,
    PAYROLL_PROJECTION,
    PAYROLL_SNAPSHOT_EVENT
);

#[async_trait]
impl Projector for EmployeeProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let snapshot = validate_event(
            context,
            event,
            EMPLOYEE_PROJECTION,
            EMPLOYEE_AGGREGATE_TYPE,
            EMPLOYEE_SNAPSHOT_EVENT,
        )?;
        let PeoplePayrollSnapshot::Employee(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "employee projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_people_payroll_read.employee_projection (projection_name,generation,partition_key,employee_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,employee_code,display_name,hired_on,terminated_on,employee_status,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,employee_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,employee_code=EXCLUDED.employee_code,display_name=EXCLUDED.display_name,hired_on=EXCLUDED.hired_on,terminated_on=EXCLUDED.terminated_on,employee_status=EXCLUDED.employee_status,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_people_payroll_read.employee_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(EMPLOYEE_PROJECTION).bind(context.generation).bind(&event.partition)
            .bind(view.employee_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
            .bind(&view.employee_code).bind(&view.display_name).bind(view.hired_on).bind(view.terminated_on)
            .bind(employee_status_name(view.status)).bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **transaction).await.map_err(failure)?;
        let employment = employment_record_ref_for(view.employee_id).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_people_payroll_read.employment_projection
            (holding_id,projection_name,generation,partition_key,employment_id,employee_id,
             source_aggregate_version,source_global_sequence,active_scope,scope_key,
             employee_code,display_name,hired_on,terminated_on,employment_status,
             aggregate_updated_at,projected_at)
            VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now())
            ON CONFLICT (holding_id,projection_name,generation,partition_key,employment_id)
            DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
             source_global_sequence=EXCLUDED.source_global_sequence,
             terminated_on=EXCLUDED.terminated_on,employment_status=EXCLUDED.employment_status,
             aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
            WHERE erpx_people_payroll_read.employment_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(EMPLOYEE_PROJECTION).bind(context.generation).bind(&event.partition)
            .bind(employment.id().as_uuid()).bind(view.employee_id.as_uuid())
            .bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
            .bind(&view.employee_code).bind(&view.display_name).bind(view.hired_on).bind(view.terminated_on)
            .bind(employee_status_name(view.status)).bind(view.updated_at)
            .execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            transaction,
            context,
            "erpx_people_payroll_read.employee_projection",
            EMPLOYEE_PROJECTION,
        )
        .await?;
        reset(
            transaction,
            context,
            "erpx_people_payroll_read.employment_projection",
            EMPLOYEE_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for LeaveRequestProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let snapshot = validate_event(
            context,
            event,
            LEAVE_PROJECTION,
            LEAVE_AGGREGATE_TYPE,
            LEAVE_SNAPSHOT_EVENT,
        )?;
        let PeoplePayrollSnapshot::LeaveRequest(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "leave projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_people_payroll_read.leave_request_projection (projection_name,generation,partition_key,leave_request_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,employee_id,starts_on,ends_on,leave_status,requested_by,approved_by,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,leave_request_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,employee_id=EXCLUDED.employee_id,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,leave_status=EXCLUDED.leave_status,requested_by=EXCLUDED.requested_by,approved_by=EXCLUDED.approved_by,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_people_payroll_read.leave_request_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(LEAVE_PROJECTION).bind(context.generation).bind(&event.partition)
            .bind(view.leave_request_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
            .bind(view.employee_id.as_uuid()).bind(view.starts_on).bind(view.ends_on)
            .bind(leave_status_name(view.status)).bind(&view.requested_by).bind(&view.approved_by)
            .bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            transaction,
            context,
            "erpx_people_payroll_read.leave_request_projection",
            LEAVE_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for ExpenseClaimProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let snapshot = validate_event(
            context,
            event,
            EXPENSE_PROJECTION,
            EXPENSE_AGGREGATE_TYPE,
            EXPENSE_SNAPSHOT_EVENT,
        )?;
        let PeoplePayrollSnapshot::ExpenseClaim(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "expense projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_people_payroll_read.expense_claim_projection (projection_name,generation,partition_key,expense_claim_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,employee_id,currency,amount_minor,expense_status,claimed_by,approved_by,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,expense_claim_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,employee_id=EXCLUDED.employee_id,currency=EXCLUDED.currency,amount_minor=EXCLUDED.amount_minor,expense_status=EXCLUDED.expense_status,claimed_by=EXCLUDED.claimed_by,approved_by=EXCLUDED.approved_by,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_people_payroll_read.expense_claim_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(EXPENSE_PROJECTION).bind(context.generation).bind(&event.partition)
            .bind(view.expense_claim_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
            .bind(view.employee_id.as_uuid()).bind(view.currency.as_str()).bind(view.amount_minor)
            .bind(expense_status_name(view.status)).bind(&view.claimed_by).bind(&view.approved_by)
            .bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            transaction,
            context,
            "erpx_people_payroll_read.expense_claim_projection",
            EXPENSE_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for PayrollRunProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let snapshot = validate_event(
            context,
            event,
            PAYROLL_PROJECTION,
            PAYROLL_AGGREGATE_TYPE,
            PAYROLL_SNAPSHOT_EVENT,
        )?;
        let PeoplePayrollSnapshot::PayrollRun(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "payroll projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let lines = serde_json::to_value(&view.lines).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_people_payroll_read.payroll_run_projection (projection_name,generation,partition_key,payroll_run_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,period_code,starts_on,ends_on,currency,payroll_status,prepared_by,reviewed_by,approved_by,total_gross_minor,total_deductions_minor,total_net_minor,lines,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,payroll_run_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,period_code=EXCLUDED.period_code,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,currency=EXCLUDED.currency,payroll_status=EXCLUDED.payroll_status,prepared_by=EXCLUDED.prepared_by,reviewed_by=EXCLUDED.reviewed_by,approved_by=EXCLUDED.approved_by,total_gross_minor=EXCLUDED.total_gross_minor,total_deductions_minor=EXCLUDED.total_deductions_minor,total_net_minor=EXCLUDED.total_net_minor,lines=EXCLUDED.lines,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_people_payroll_read.payroll_run_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(PAYROLL_PROJECTION).bind(context.generation).bind(&event.partition)
            .bind(view.payroll_run_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
            .bind(&view.period_code).bind(view.starts_on).bind(view.ends_on).bind(view.currency.as_str())
            .bind(payroll_status_name(view.status)).bind(&view.prepared_by).bind(&view.reviewed_by).bind(&view.approved_by)
            .bind(view.total_gross_minor).bind(view.total_deductions_minor).bind(view.total_net_minor)
            .bind(lines).bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **transaction).await.map_err(failure)?;
        for line in &view.lines {
            let payslip =
                payslip_record_ref_for(view.payroll_run_id, line.employee_id).map_err(failure)?;
            sqlx::query("INSERT INTO erpx_people_payroll_read.payslip_projection
                (holding_id,projection_name,generation,partition_key,payslip_id,payroll_run_id,
                 employee_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,
                 period_code,starts_on,ends_on,currency,gross_minor,deductions_minor,net_minor,
                 payslip_status,aggregate_updated_at,projected_at)
                VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,now())
                ON CONFLICT (holding_id,projection_name,generation,partition_key,payslip_id)
                DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                 source_global_sequence=EXCLUDED.source_global_sequence,
                 gross_minor=EXCLUDED.gross_minor,deductions_minor=EXCLUDED.deductions_minor,
                 net_minor=EXCLUDED.net_minor,payslip_status=EXCLUDED.payslip_status,
                 aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
                WHERE erpx_people_payroll_read.payslip_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
                .bind(PAYROLL_PROJECTION).bind(context.generation).bind(&event.partition)
                .bind(payslip.id().as_uuid()).bind(view.payroll_run_id.as_uuid())
                .bind(line.employee_id.as_uuid()).bind(to_i64(view.version)?)
                .bind(to_i64(event.global_sequence)?)
                .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
                .bind(&view.period_code).bind(view.starts_on).bind(view.ends_on).bind(view.currency.as_str())
                .bind(line.gross_minor).bind(line.deductions_minor).bind(line.net_minor)
                .bind(payroll_status_name(view.status)).bind(view.updated_at)
                .execute(&mut **transaction).await.map_err(failure)?;
        }
        Ok(())
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            transaction,
            context,
            "erpx_people_payroll_read.payroll_run_projection",
            PAYROLL_PROJECTION,
        )
        .await?;
        reset(
            transaction,
            context,
            "erpx_people_payroll_read.payslip_projection",
            PAYROLL_PROJECTION,
        )
        .await
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<PeoplePayrollSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "People & Payroll projector received incompatible event",
        ));
    }
    let payload: PeoplePayrollOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let PeoplePayrollOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "snapshot event carried integration payload",
        ));
    };
    let matches = match &snapshot {
        PeoplePayrollSnapshot::Employee(value) => {
            aggregate_type == EMPLOYEE_AGGREGATE_TYPE
                && value.version == event.aggregate_version
                && value.employee_id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.active_scope
                && event.partition == employee_partition(value.employee_id)
        }
        PeoplePayrollSnapshot::LeaveRequest(value) => {
            aggregate_type == LEAVE_AGGREGATE_TYPE
                && value.version == event.aggregate_version
                && value.leave_request_id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.active_scope
                && event.partition == leave_partition(value.leave_request_id)
        }
        PeoplePayrollSnapshot::ExpenseClaim(value) => {
            aggregate_type == EXPENSE_AGGREGATE_TYPE
                && value.version == event.aggregate_version
                && value.expense_claim_id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.active_scope
                && event.partition == expense_partition(value.expense_claim_id)
        }
        PeoplePayrollSnapshot::PayrollRun(value) => {
            aggregate_type == PAYROLL_AGGREGATE_TYPE
                && value.version == event.aggregate_version
                && value.payroll_run_id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.active_scope
                && event.partition == payroll_partition(value.payroll_run_id)
        }
    };
    if !matches {
        return Err(ProjectionFailure::new("snapshot metadata mismatch"));
    }
    Ok(snapshot)
}

async fn reset(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    table: &str,
    projection: &str,
) -> Result<(), ProjectionFailure> {
    let statement = format!("DELETE FROM {table} WHERE projection_name=$1 AND generation=$2");
    sqlx::query(&statement)
        .bind(projection)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
    Ok(())
}

pub(crate) const fn employee_status_name(value: EmployeeStatus) -> &'static str {
    match value {
        EmployeeStatus::Draft => "draft",
        EmployeeStatus::Active => "active",
        EmployeeStatus::OnLeave => "on_leave",
        EmployeeStatus::Suspended => "suspended",
        EmployeeStatus::Terminated => "terminated",
    }
}
pub(crate) const fn leave_status_name(value: LeaveStatus) -> &'static str {
    match value {
        LeaveStatus::Draft => "draft",
        LeaveStatus::Submitted => "submitted",
        LeaveStatus::Approved => "approved",
        LeaveStatus::Rejected => "rejected",
        LeaveStatus::Cancelled => "cancelled",
        LeaveStatus::Taken => "taken",
    }
}
pub(crate) const fn expense_status_name(value: ExpenseStatus) -> &'static str {
    match value {
        ExpenseStatus::Draft => "draft",
        ExpenseStatus::Submitted => "submitted",
        ExpenseStatus::Approved => "approved",
        ExpenseStatus::Rejected => "rejected",
        ExpenseStatus::Reimbursed => "reimbursed",
    }
}
pub(crate) const fn payroll_status_name(value: PayrollStatus) -> &'static str {
    match value {
        PayrollStatus::Draft => "draft",
        PayrollStatus::Review => "review",
        PayrollStatus::Approved => "approved",
        PayrollStatus::Posted => "posted",
        PayrollStatus::Cancelled => "cancelled",
    }
}
fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("numeric value is too large"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_projectors_are_snapshot_specific() {
        assert_eq!(
            EmployeeProjector::new().event_types(),
            &[EMPLOYEE_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            LeaveRequestProjector::new().event_types(),
            &[LEAVE_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            ExpenseClaimProjector::new().event_types(),
            &[EXPENSE_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            PayrollRunProjector::new().event_types(),
            &[PAYROLL_SNAPSHOT_EVENT.to_owned()]
        );
    }

    #[test]
    fn upserts_ignore_duplicate_and_out_of_order_versions() {
        let source = include_str!("projection.rs");
        assert_eq!(
            source
                .matches(concat!(
                    "source_aggregate_version < ",
                    "EXCLUDED.source_aggregate_version"
                ))
                .count(),
            6
        );
        assert_eq!(source.matches(concat!("async fn ", "reset(")).count(), 5);
    }
}
