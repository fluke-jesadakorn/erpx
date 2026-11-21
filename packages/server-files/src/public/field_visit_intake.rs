//! Trusted in-process composition port for Field visit intake.
//!
//! Implementations are compiled composition code, not HTTP input or plugins. The
//! implementation must retain the owner proof obtained by `authorize` and
//! revalidate that proof, Account and purchase authority on this connection.
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::PgConnection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldIntakeScope {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}

pub trait FieldIntakeAuthority {
    fn owner_kind(&self) -> IntakeOwner {
        IntakeOwner::FieldVisit
    }
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        target_id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError>;
    fn revalidate<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        target_id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

pub use crate::adapters::field_visit_intake::{CurrentFieldIntake, prepare_field_intake};

pub use crate::adapters::field_visit_intake::{field_set_query_has_rows, list_field_sets};
pub use crate::domain::field_visit_intake::{
    FieldSetListRequest, FieldSetRequest, IntakeOwner, classification, component_ref,
    field_set_command_id, field_set_id, owner_classification, validate_component_ref,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldIntakeAvailability {
    pub uploads_frozen: bool,
    pub scanner_ready: bool,
    pub preview_ready: bool,
}
pub use crate::adapters::field_visit_intake::create_field_set;

pub use crate::adapters::field_visit_intake::{
    FieldSetCreationPlan, FieldSetDisposition, WrittenFieldSet, prepare_field_set_creation,
};

pub use crate::adapters::field_visit_intake::{
    RecordedFieldSet, RecordedFieldSetKind, recorded_field_set,
};
