//! Native Organization setup contract: staff-level install, saved templates
//! and reviewed template application. Consumers use these named exports; no
//! SQL relation or private adapter type escapes the crate.
pub use crate::adapters::postgres_organization_setup::PgOrganizationSetup;
pub use crate::application::organization_setup::{
    AppliedPosition, ApplyTemplateCommand, ClockSource, InstallLevelsCommand,
    OrganizationSetupRepository, SaveTemplateCommand, TemplateRow, apply_template, install_levels,
    save_template,
};
pub use crate::domain::organization_setup::{
    ApplyTemplateInput, MAX_TEMPLATE_POSITIONS, STAFF_LEVEL_TITLES, TemplateInput,
    TemplatePosition, validate_scope, validate_template_positions,
};
