//! Organization setup use cases: install the six staff levels, save a
//! template and apply a reviewed template to positions.
//!
//! The repository supplies current sources and owns the transaction; this
//! layer keeps the retained command order, optimistic template version checks
//! and current-placement guards. It never opens a transaction.
pub use crate::application::organization_nodes::ClockSource;
use crate::{
    application::organization_nodes::{OrganizationCommandActor, OrganizationFuture},
    domain::organization_setup::{
        ApplyTemplateInput, TemplateInput, TemplatePosition, validate_scope,
        validate_template_positions,
    },
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TemplateRow {
    pub positions_json: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct AppliedPosition {
    pub id: String,
    pub title: String,
    pub level_id: String,
    pub reports_to_position_id: Option<String>,
    pub is_ceo: bool,
    pub roster_required: bool,
}

#[derive(Clone, Debug)]
pub struct InstallLevelsCommand {
    pub actor: OrganizationCommandActor,
    pub key: String,
    pub titles: Vec<&'static str>,
}

#[derive(Clone, Debug)]
pub struct SaveTemplateCommand {
    pub actor: OrganizationCommandActor,
    pub key: String,
    pub id: Option<String>,
    pub record_id: String,
    pub input: TemplateInput,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ApplyTemplateCommand {
    pub actor: OrganizationCommandActor,
    pub key: String,
    pub template_id: String,
    pub input: ApplyTemplateInput,
    pub positions: Vec<AppliedPosition>,
    pub position_ids: Vec<String>,
    pub version: i64,
}

pub trait OrganizationSetupRepository: Send {
    type Error: From<BusinessError> + Send;

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> OrganizationFuture<'a, Option<Value>, Self::Error>;

    fn template<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> OrganizationFuture<'a, Option<TemplateRow>, Self::Error>;

    fn branch_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: &'a str,
    ) -> OrganizationFuture<'a, bool, Self::Error>;

    fn department_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        department: &'a str,
        branch: Option<&'a str>,
    ) -> OrganizationFuture<'a, bool, Self::Error>;

    fn levels<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> OrganizationFuture<'a, Vec<(i64, String)>, Self::Error>;

    fn ceo_position<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> OrganizationFuture<'a, Option<String>, Self::Error>;

    fn persist_install_levels<'a>(
        &'a mut self,
        command: &'a InstallLevelsCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error>;

    fn persist_save_template<'a>(
        &'a mut self,
        command: &'a SaveTemplateCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error>;

    fn persist_apply_template<'a>(
        &'a mut self,
        command: &'a ApplyTemplateCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error>;
}

fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
}

fn template_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TEMPLATE_NOT_FOUND",
        "Organization template not found.",
    )
}

fn conflict(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, "CONFLICT", message)
}

fn manage_projection(
    actor: &OrganizationCommandActor,
) -> Result<crate::domain::organization_nodes::OrganizationActor, BusinessError> {
    let projection = actor.projection();
    if !projection.can_manage() {
        return Err(permission_required());
    }
    Ok(projection)
}

fn record_id(value: String) -> Result<String, BusinessError> {
    RecordId::new(value)
        .map(|id| id.as_str().to_owned())
        .map_err(|_| {
            BusinessError::invalid(
                "VALIDATION_FAILED",
                "Provide a template identifier of at most 200 bytes.",
            )
        })
}

fn template_receipt(input: &TemplateInput) -> Result<Value, BusinessError> {
    serde_json::to_value(input).map_err(|_| {
        BusinessError::invalid(
            "VALIDATION_FAILED",
            "Provide a complete organization template.",
        )
    })
}

fn apply_receipt(input: &ApplyTemplateInput) -> Result<Value, BusinessError> {
    serde_json::to_value(input).map_err(|_| {
        BusinessError::invalid(
            "VALIDATION_FAILED",
            "Provide a complete template application.",
        )
    })
}

/// Retained `installLevels`: one fixed six-level catalog, insert-only.
pub async fn install_levels<R: OrganizationSetupRepository>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    key: &str,
    clock: &dyn ClockSource,
) -> Result<Value, R::Error> {
    let projection = manage_projection(&actor)?;
    let operation = "organization.levels.install";
    let record_id = projection.company_id.as_str().to_owned();
    let receipt = Value::Object(serde_json::Map::new());
    if let Some(prior) = repository
        .replay(&actor, key, operation, &record_id, &receipt)
        .await?
    {
        return Ok(prior);
    }
    let command = InstallLevelsCommand {
        actor,
        key: key.to_owned(),
        titles: crate::domain::organization_setup::STAFF_LEVEL_TITLES.to_vec(),
    };
    repository
        .persist_install_levels(&command, clock.now().get())
        .await
}

/// Retained `saveTemplate`: validate the whole template, then CAS the saved
/// version. A new template is named from the idempotency key.
pub async fn save_template<R: OrganizationSetupRepository>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    key: &str,
    id: Option<&str>,
    input: TemplateInput,
    clock: &dyn ClockSource,
) -> Result<Value, R::Error> {
    let projection = manage_projection(&actor)?;
    let record_id = match id {
        Some(id) => record_id(id.to_owned())?,
        None => record_id(format!("org-template-{key}"))?,
    };
    let operation = "organization.template.save";
    let receipt = template_receipt(&input)?;
    if let Some(prior) = repository
        .replay(&actor, key, operation, &record_id, &receipt)
        .await?
    {
        return Ok(prior);
    }
    input.validate()?;
    if id.is_some() {
        let current = repository
            .template(&projection.company_id, &record_id)
            .await?
            .ok_or_else(template_not_found)?;
        if current.version != input.expected_version {
            return Err(conflict(
                "The organization template changed. Reload it and review the revision.",
            )
            .into());
        }
    }
    let version = input.expected_version + 1;
    let command = SaveTemplateCommand {
        actor,
        key: key.to_owned(),
        id: id.map(str::to_owned),
        record_id,
        input,
        version,
    };
    repository
        .persist_save_template(&command, clock.now().get())
        .await
}

/// Retained `applyTemplate`: current placement, saved template validation,
/// existing-CEO reuse and one atomic position creation command.
pub async fn apply_template<R: OrganizationSetupRepository>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    key: &str,
    template_id: &str,
    input: ApplyTemplateInput,
    clock: &dyn ClockSource,
) -> Result<Value, R::Error> {
    let projection = manage_projection(&actor)?;
    let template_id = record_id(template_id.to_owned())?;
    let operation = "organization.template.apply";
    let receipt = apply_receipt(&input)?;
    if let Some(prior) = repository
        .replay(&actor, key, operation, &template_id, &receipt)
        .await?
    {
        return Ok(prior);
    }
    validate_scope(
        projection.branch_id.as_ref().map(RecordId::as_str),
        projection.department_id.as_ref().map(RecordId::as_str),
        input.branch_id.as_deref(),
        input.department_id.as_deref(),
    )?;
    if let Some(branch) = &input.branch_id
        && !repository
            .branch_active(&projection.company_id, branch)
            .await?
    {
        return Err(BusinessError::invalid(
            "BRANCH_INVALID",
            "Choose an active branch in this company.",
        )
        .into());
    }
    if let Some(department) = &input.department_id
        && !repository
            .department_active(
                &projection.company_id,
                department,
                input.branch_id.as_deref(),
            )
            .await?
    {
        return Err(BusinessError::invalid(
            "DEPARTMENT_INVALID",
            "Choose a department in the selected branch.",
        )
        .into());
    }
    let template = repository
        .template(&projection.company_id, &template_id)
        .await?
        .ok_or_else(template_not_found)?;
    if template.version != input.expected_version {
        return Err(conflict(
            "The organization template changed. Reload it and review the revision.",
        )
        .into());
    }
    let raw: Value = serde_json::from_str(&template.positions_json).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "TEMPLATE_SOURCE_INVALID",
            "The organization template is unavailable.",
        )
    })?;
    let values = raw.as_array().ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "TEMPLATE_SOURCE_INVALID",
            "The organization template is unavailable.",
        )
    })?;
    let positions = values
        .iter()
        .map(TemplatePosition::parse)
        .collect::<Result<Vec<_>, _>>()?;
    validate_template_positions(&positions)?;
    let levels = repository.levels(&projection.company_id).await?;
    let existing_ceo = repository.ceo_position(&projection.company_id).await?;
    let mut ids = std::collections::BTreeMap::new();
    for (index, position) in positions.iter().enumerate() {
        let id = if position.is_ceo && existing_ceo.is_some() {
            existing_ceo.clone().unwrap_or_default()
        } else {
            format!("position-{key}-{index}")
        };
        ids.insert(position.key.clone(), id);
    }
    let mut applied = Vec::with_capacity(positions.len());
    for position in &positions {
        if position.is_ceo && existing_ceo.is_some() {
            continue;
        }
        let level_id = levels
            .iter()
            .find(|(number, _)| *number == position.level_number)
            .map(|(_, id)| id.clone())
            .ok_or_else(|| {
                BusinessError::invalid(
                    "LEVEL_REQUIRED",
                    "Install the six staff levels before applying this template.",
                )
            })?;
        let id = ids
            .get(&position.key)
            .cloned()
            .ok_or_else(|| conflict("The organization template changed. Reload it."))?;
        let parent = position
            .reports_to_key
            .as_ref()
            .and_then(|key| ids.get(key).cloned());
        applied.push(AppliedPosition {
            id: id.clone(),
            title: position.title.clone(),
            level_id,
            reports_to_position_id: parent,
            is_ceo: position.is_ceo,
            roster_required: position.roster_required,
        });
    }
    // The retained result includes the reused CEO id alongside new positions.
    let position_ids = positions
        .iter()
        .filter_map(|position| ids.get(&position.key).cloned())
        .collect::<Vec<_>>();
    let command = ApplyTemplateCommand {
        actor,
        key: key.to_owned(),
        template_id,
        input,
        positions: applied,
        position_ids,
        version: template.version,
    };
    repository
        .persist_apply_template(&command, clock.now().get())
        .await
}
