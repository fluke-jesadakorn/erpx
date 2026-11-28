//! Portable preventive-action command input. Only the owner adapter constructs
//! it after canonical validation; the canonical receipt bytes are preserved
//! for fingerprinting. Current plan state and authority are separate.
use crate::application::work_commands::MaintenanceCommandInputJson;
use crate::domain::preventive_input::MaintenancePreventiveActionInput;

#[derive(Clone, Debug)]
pub struct NormalizedMaintenancePreventiveAction {
    input: MaintenancePreventiveActionInput,
    json: MaintenanceCommandInputJson,
}

impl NormalizedMaintenancePreventiveAction {
    pub(crate) fn new(input: MaintenancePreventiveActionInput, json: String) -> Self {
        Self {
            input,
            json: MaintenanceCommandInputJson::from_owned(json),
        }
    }
    pub fn input(&self) -> &MaintenancePreventiveActionInput {
        &self.input
    }
    pub fn receipt_input(&self) -> &MaintenanceCommandInputJson {
        &self.json
    }
}
