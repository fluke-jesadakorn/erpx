pub mod expense;
pub mod identity_facts;
pub mod invitation_person_link;
pub mod leave;
pub mod roster_position_exit;
pub mod workforce;
pub mod workforce_reference;

pub mod native_command;
pub mod native_command_http;

pub mod finance_journal_names;

pub mod field_service_worker;

pub mod payroll_accounting;

pub mod payroll_run;

pub mod payroll_run_http;

pub mod payroll_schedule;

pub mod payroll_summary;

pub mod payroll_summary_http;

pub mod journal_flow_people;

pub mod benefit_person;
pub mod field_service_workspace;
pub mod learning_person;
pub mod operations_people;
pub mod service_count_flow_people;

pub mod staff_facts;

pub mod rfq_request_people;
pub mod workflow_projection_source;

/// Native People workforce directory command HTTP ingress.
pub mod workforce_http {
    pub use crate::adapters::workforce_http::{
        WorkforceHttpBackend, WorkforceHttpFuture, WorkforceHttpKind, WorkforceHttpRequest,
        WorkforceHttpResponse, WorkforceHttpTarget, workforce_router,
    };
}
pub mod native_lane_6;
pub mod native_lane_7;
pub mod native_lane_8;
pub mod native_lane_9;
