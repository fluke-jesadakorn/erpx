use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::{StableRecordRef, catalog::ContextId};
use erpx_kernel::{AuthorityScope, DataClassification};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_SEARCH_QUERY_BYTES: usize = 512;
pub const MAX_SEARCH_PAGE_SIZE: u16 = 100;
pub const MAX_IMPORT_ROWS: usize = 100_000;
pub const MAX_EXTENSION_FIELDS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InformationServiceError {
    #[error("shared-service input is invalid")]
    Invalid,
    #[error("shared-service authorization is denied")]
    Denied,
    #[error("source authority or freshness is unavailable")]
    SourceUnavailable,
    #[error("shared-service lifecycle conflict")]
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceLifecycle {
    Draft,
    Approved,
    Active,
    Suspended,
    Retired,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceDescriptor {
    pub record: StableRecordRef,
    pub owner: ContextId,
    pub scope: AuthorityScope,
    pub stable_code: String,
    pub labels: BTreeMap<String, String>,
    pub classification: DataClassification,
    pub lifecycle: ReferenceLifecycle,
    pub version: u64,
    pub effective_from: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub source_position: u64,
}

impl ReferenceDescriptor {
    pub fn validate(&self) -> Result<(), InformationServiceError> {
        if self.record.validate().is_err()
            || self.record.owner_context() != self.owner
            || self.scope.validate().is_err()
            || !valid_text(&self.stable_code, 128)
            || self.labels.is_empty()
            || !self.labels.contains_key("en")
            || self
                .labels
                .iter()
                .any(|(locale, label)| !valid_text(locale, 16) || !valid_text(label, 255))
            || self.version == 0
            || self.source_position == 0
            || self
                .effective_until
                .is_some_and(|until| until <= self.effective_from)
        {
            return Err(InformationServiceError::Invalid);
        }
        Ok(())
    }

    pub fn label(&self, locale: &str) -> Option<(&str, bool)> {
        self.labels
            .get(locale)
            .map(|value| (value.as_str(), false))
            .or_else(|| self.labels.get("en").map(|value| (value.as_str(), true)))
    }
}

pub fn validate_reference_effectivity(
    descriptors: &[ReferenceDescriptor],
) -> Result<(), InformationServiceError> {
    for descriptor in descriptors {
        descriptor.validate()?;
    }
    for (index, left) in descriptors.iter().enumerate() {
        for right in &descriptors[index + 1..] {
            if left.record == right.record
                && left.scope == right.scope
                && ranges_overlap(
                    left.effective_from,
                    left.effective_until,
                    right.effective_from,
                    right.effective_until,
                )
            {
                return Err(InformationServiceError::Conflict);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Xlsx,
    Pdf,
    InternalApi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportDefinition {
    pub definition_id: Uuid,
    pub version: u64,
    pub owner_context: ContextId,
    pub measures: Vec<String>,
    pub dimensions: Vec<String>,
    pub allowed_formats: HashSet<ExportFormat>,
    pub classification: DataClassification,
    pub requires_approval: bool,
    pub source_contract_versions: BTreeMap<String, u64>,
}

impl ReportDefinition {
    pub fn validate(&self) -> Result<(), InformationServiceError> {
        if self.definition_id.is_nil()
            || self.version == 0
            || self.measures.is_empty()
            || self.allowed_formats.is_empty()
            || self
                .measures
                .iter()
                .chain(&self.dimensions)
                .any(|value| !valid_text(value, 128))
            || self
                .source_contract_versions
                .iter()
                .any(|(source, version)| !valid_text(source, 128) || *version == 0)
        {
            return Err(InformationServiceError::Invalid);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportManifest {
    pub export_id: Uuid,
    pub definition_id: Uuid,
    pub definition_version: u64,
    pub parameters_sha256: String,
    pub scope: AuthorityScope,
    pub source_positions: BTreeMap<String, u64>,
    pub freshness_cutoff: DateTime<Utc>,
    pub generated_at: DateTime<Utc>,
    pub generated_by: String,
    pub format: ExportFormat,
    pub content_sha256: String,
    pub classification: DataClassification,
    pub approval_sha256: Option<String>,
}

impl ExportManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        export_id: Uuid,
        definition: &ReportDefinition,
        parameters_sha256: String,
        scope: AuthorityScope,
        source_positions: BTreeMap<String, u64>,
        freshness_cutoff: DateTime<Utc>,
        generated_at: DateTime<Utc>,
        generated_by: String,
        format: ExportFormat,
        content_sha256: String,
        approval_sha256: Option<String>,
    ) -> Result<Self, InformationServiceError> {
        definition.validate()?;
        if export_id.is_nil()
            || scope.validate().is_err()
            || !definition.allowed_formats.contains(&format)
            || !valid_digest(&parameters_sha256)
            || !valid_digest(&content_sha256)
            || !valid_text(&generated_by, 255)
            || source_positions.is_empty()
            || source_positions
                .iter()
                .any(|(source, position)| !valid_text(source, 128) || *position == 0)
            || generated_at < freshness_cutoff
            || (definition.requires_approval
                && approval_sha256
                    .as_deref()
                    .is_none_or(|value| !valid_digest(value)))
        {
            return Err(InformationServiceError::Denied);
        }
        Ok(Self {
            export_id,
            definition_id: definition.definition_id,
            definition_version: definition.version,
            parameters_sha256,
            scope,
            source_positions,
            freshness_cutoff,
            generated_at,
            generated_by,
            format,
            content_sha256,
            classification: definition.classification,
            approval_sha256,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportDownloadGrant {
    pub grant_id: Uuid,
    pub export_id: Uuid,
    pub recipient_ref: String,
    pub manifest_sha256: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl ExportDownloadGrant {
    pub fn authorize(
        &self,
        recipient_ref: &str,
        manifest_sha256: &str,
        at: DateTime<Utc>,
    ) -> Result<(), InformationServiceError> {
        if self.grant_id.is_nil()
            || self.export_id.is_nil()
            || recipient_ref != self.recipient_ref
            || manifest_sha256 != self.manifest_sha256
            || !valid_digest(manifest_sha256)
            || self.revoked_at.is_some()
            || at >= self.expires_at
        {
            return Err(InformationServiceError::Denied);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportAtomicity {
    AllOrNothing,
    PerRow,
    PerGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportTemplate {
    pub template_id: Uuid,
    pub version: u64,
    pub owner_context: ContextId,
    pub target_command: String,
    pub schema_sha256: String,
    pub atomicity: ImportAtomicity,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanVerdict {
    Safe,
    Quarantined,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportFinding {
    pub row_number: u32,
    pub finding_code: String,
    pub eligible_command: Option<String>,
    pub authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRowReceipt {
    pub row_number: u32,
    pub command_id: Uuid,
    pub source_receipt_ref: Option<String>,
    pub rejection_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Uploaded,
    Validated,
    Approved,
    Executing,
    Reconciled,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportBatch {
    pub batch_id: Uuid,
    pub version: u64,
    pub scope: AuthorityScope,
    pub template_id: Uuid,
    pub template_version: u64,
    pub template_sha256: String,
    pub file_sha256: String,
    pub classification: DataClassification,
    pub uploader_ref: String,
    pub row_count: u32,
    pub status: ImportStatus,
    pub findings: Vec<ImportFinding>,
    pub dry_run_sha256: Option<String>,
    pub approval_sha256: Option<String>,
    pub receipts: Vec<ImportRowReceipt>,
}

impl ImportBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn upload(
        batch_id: Uuid,
        scope: AuthorityScope,
        template: &ImportTemplate,
        file_sha256: String,
        classification: DataClassification,
        uploader_ref: String,
        row_count: u32,
        scan: ScanVerdict,
    ) -> Result<Self, InformationServiceError> {
        if batch_id.is_nil()
            || scope.validate().is_err()
            || template.template_id.is_nil()
            || template.version == 0
            || !template.active
            || !valid_text(&template.target_command, 255)
            || !valid_digest(&template.schema_sha256)
            || !valid_digest(&file_sha256)
            || !valid_text(&uploader_ref, 255)
            || row_count == 0
            || usize::try_from(row_count).map_or(true, |value| value > MAX_IMPORT_ROWS)
        {
            return Err(InformationServiceError::Invalid);
        }
        if scan == ScanVerdict::Unavailable {
            return Err(InformationServiceError::SourceUnavailable);
        }
        Ok(Self {
            batch_id,
            version: 1,
            scope,
            template_id: template.template_id,
            template_version: template.version,
            template_sha256: template.schema_sha256.clone(),
            file_sha256,
            classification,
            uploader_ref,
            row_count,
            status: if scan == ScanVerdict::Safe {
                ImportStatus::Uploaded
            } else {
                ImportStatus::Quarantined
            },
            findings: Vec::new(),
            dry_run_sha256: None,
            approval_sha256: None,
            receipts: Vec::new(),
        })
    }

    pub fn record_dry_run(
        &mut self,
        findings: Vec<ImportFinding>,
        authority_snapshot_sha256: &str,
    ) -> Result<(), InformationServiceError> {
        if self.status != ImportStatus::Uploaded
            || !valid_digest(authority_snapshot_sha256)
            || findings.len() != usize::try_from(self.row_count).unwrap_or(usize::MAX)
            || findings.iter().enumerate().any(|(index, finding)| {
                finding.row_number != u32::try_from(index + 1).unwrap_or(u32::MAX)
                    || !valid_text(&finding.finding_code, 128)
                    || (finding.authorized && finding.eligible_command.is_none())
            })
        {
            return Err(InformationServiceError::Invalid);
        }
        self.findings = findings;
        self.dry_run_sha256 = Some(digest_json(&(
            &self.file_sha256,
            &self.template_sha256,
            authority_snapshot_sha256,
            &self.findings,
        )));
        self.status = ImportStatus::Validated;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(InformationServiceError::Invalid)?;
        Ok(())
    }

    pub fn approve(
        &mut self,
        approver_ref: &str,
        expected_file_sha256: &str,
        expected_dry_run_sha256: &str,
        command_policy_sha256: &str,
    ) -> Result<(), InformationServiceError> {
        if self.status != ImportStatus::Validated
            || approver_ref == self.uploader_ref
            || !valid_text(approver_ref, 255)
            || expected_file_sha256 != self.file_sha256
            || self.dry_run_sha256.as_deref() != Some(expected_dry_run_sha256)
            || !valid_digest(command_policy_sha256)
        {
            return Err(InformationServiceError::Denied);
        }
        self.approval_sha256 = Some(digest_json(&(
            expected_file_sha256,
            expected_dry_run_sha256,
            command_policy_sha256,
            approver_ref,
        )));
        self.status = ImportStatus::Approved;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(InformationServiceError::Invalid)?;
        Ok(())
    }

    pub fn record_row_receipt(
        &mut self,
        receipt: ImportRowReceipt,
    ) -> Result<(), InformationServiceError> {
        if !matches!(
            self.status,
            ImportStatus::Approved | ImportStatus::Executing
        ) || receipt.row_number == 0
            || receipt.row_number > self.row_count
            || receipt.command_id.is_nil()
            || receipt.source_receipt_ref.is_some() == receipt.rejection_code.is_some()
            || self
                .receipts
                .iter()
                .any(|existing| existing.row_number == receipt.row_number)
        {
            return Err(InformationServiceError::Conflict);
        }
        self.receipts.push(receipt);
        self.status =
            if self.receipts.len() == usize::try_from(self.row_count).unwrap_or(usize::MAX) {
                ImportStatus::Reconciled
            } else {
                ImportStatus::Executing
            };
        self.version = self
            .version
            .checked_add(1)
            .ok_or(InformationServiceError::Invalid)?;
        Ok(())
    }
}

#[async_trait]
pub trait ImportCommandPort: Send + Sync {
    async fn execute_row(
        &self,
        batch: &ImportBatch,
        row_number: u32,
        target_command: &str,
        idempotency_key: &str,
    ) -> Result<ImportRowReceipt, InformationServiceError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub principal_ref: String,
    pub scope: AuthorityScope,
    pub purpose: String,
    pub query: String,
    pub query_sha256: String,
    pub page_size: u16,
    pub cursor: Option<String>,
}

impl SearchRequest {
    pub fn validate(&self) -> Result<(), InformationServiceError> {
        if !valid_text(&self.principal_ref, 255)
            || self.scope.validate().is_err()
            || !valid_text(&self.purpose, 255)
            || self.query.trim().is_empty()
            || self.query.len() > MAX_SEARCH_QUERY_BYTES
            || self.query.contains('*')
            || !valid_digest(&self.query_sha256)
            || self.page_size == 0
            || self.page_size > MAX_SEARCH_PAGE_SIZE
            || self
                .cursor
                .as_deref()
                .is_some_and(|value| !valid_text(value, 512))
        {
            return Err(InformationServiceError::Invalid);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedSearchResult {
    pub record: StableRecordRef,
    pub authorized_label: String,
    pub lifecycle_state: String,
    pub canonical_resolver: String,
    pub source_position: u64,
    pub freshness_known: bool,
}

#[async_trait]
pub trait FederatedSearchPort: Send + Sync {
    /// Implementations authorize before indexing, matching, snippets, facets,
    /// and ranking. Returning unfiltered source rows violates this contract.
    async fn search(
        &self,
        request: &SearchRequest,
    ) -> Result<Vec<AuthorizedSearchResult>, InformationServiceError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    CustomField,
    Form,
    WorkflowTemplate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionFieldType {
    Text,
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Reference,
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionField {
    pub key: String,
    pub field_type: ExtensionFieldType,
    pub classification: DataClassification,
    pub required: bool,
    pub searchable: bool,
    pub reportable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreWorkState {
    Proposed,
    Open,
    InProgress,
    Waiting,
    Resolved,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlledExtension {
    pub extension_id: Uuid,
    pub version: u64,
    pub kind: ExtensionKind,
    pub owner_ref: String,
    pub scope: AuthorityScope,
    pub purpose: String,
    pub affected_context: ContextId,
    pub fields: Vec<ExtensionField>,
    pub workflow_state_map: HashMap<String, CoreWorkState>,
    pub approval_sha256: Option<String>,
    pub active: bool,
}

impl ControlledExtension {
    pub fn activate(&mut self, approval_sha256: String) -> Result<(), InformationServiceError> {
        if self.extension_id.is_nil()
            || self.version == 0
            || !valid_text(&self.owner_ref, 255)
            || self.scope.validate().is_err()
            || !valid_text(&self.purpose, 512)
            || self.fields.is_empty()
            || self.fields.len() > MAX_EXTENSION_FIELDS
            || !valid_digest(&approval_sha256)
        {
            return Err(InformationServiceError::Invalid);
        }
        let mut keys = HashSet::new();
        if self.fields.iter().any(|field| {
            !valid_identifier(&field.key)
                || !keys.insert(field.key.as_str())
                || ((field.searchable || field.reportable)
                    && field.classification == DataClassification::Restricted)
        }) || (self.kind == ExtensionKind::WorkflowTemplate
            && (self.workflow_state_map.is_empty()
                || self
                    .workflow_state_map
                    .keys()
                    .any(|state| !valid_identifier(state))))
        {
            return Err(InformationServiceError::Denied);
        }
        self.approval_sha256 = Some(approval_sha256);
        self.active = true;
        Ok(())
    }
}

fn ranges_overlap(
    left_start: DateTime<Utc>,
    left_end: Option<DateTime<Utc>>,
    right_start: DateTime<Utc>,
    right_end: Option<DateTime<Utc>>,
) -> bool {
    left_end.is_none_or(|end| right_start < end) && right_end.is_none_or(|end| left_start < end)
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
fn valid_identifier(value: &str) -> bool {
    valid_text(value, 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}
fn digest_json(value: &impl Serialize) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(
            serde_json::to_vec(value).expect("serializable evidence")
        ))
    )
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_786_000_000, 0).unwrap()
    }
    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn template() -> ImportTemplate {
        ImportTemplate {
            template_id: Uuid::from_u128(10),
            version: 2,
            owner_context: ContextId::Finance,
            target_command: "CreateJournalDraft".into(),
            schema_sha256: format!("sha256:{}", "a".repeat(64)),
            atomicity: ImportAtomicity::PerRow,
            active: true,
        }
    }

    #[test]
    fn report_manifest_pins_sources_and_high_risk_approval() {
        let definition = ReportDefinition {
            definition_id: Uuid::from_u128(20),
            version: 4,
            owner_context: ContextId::Finance,
            measures: vec!["net_amount".into()],
            dimensions: vec!["company".into()],
            allowed_formats: HashSet::from([ExportFormat::Csv]),
            classification: DataClassification::Confidential,
            requires_approval: true,
            source_contract_versions: BTreeMap::from([("finance-ledger".into(), 3)]),
        };
        assert_eq!(
            ExportManifest::create(
                Uuid::from_u128(21),
                &definition,
                format!("sha256:{}", "b".repeat(64)),
                scope(),
                BTreeMap::from([("finance-ledger".into(), 77)]),
                at(),
                at(),
                "reporter".into(),
                ExportFormat::Csv,
                format!("sha256:{}", "c".repeat(64)),
                None,
            ),
            Err(InformationServiceError::Denied)
        );
        assert!(
            ExportManifest::create(
                Uuid::from_u128(21),
                &definition,
                format!("sha256:{}", "b".repeat(64)),
                scope(),
                BTreeMap::from([("finance-ledger".into(), 77)]),
                at(),
                at(),
                "reporter".into(),
                ExportFormat::Csv,
                format!("sha256:{}", "c".repeat(64)),
                Some(format!("sha256:{}", "d".repeat(64))),
            )
            .is_ok()
        );
    }

    #[test]
    fn import_scan_dry_run_approval_and_row_reconciliation_are_digest_bound() {
        let mut batch = ImportBatch::upload(
            Uuid::from_u128(30),
            scope(),
            &template(),
            format!("sha256:{}", "b".repeat(64)),
            DataClassification::Confidential,
            "uploader".into(),
            1,
            ScanVerdict::Safe,
        )
        .unwrap();
        batch
            .record_dry_run(
                vec![ImportFinding {
                    row_number: 1,
                    finding_code: "valid".into(),
                    eligible_command: Some("CreateJournalDraft".into()),
                    authorized: true,
                }],
                &format!("sha256:{}", "c".repeat(64)),
            )
            .unwrap();
        let dry_run = batch.dry_run_sha256.clone().unwrap();
        assert_eq!(
            batch.approve(
                "uploader",
                &batch.file_sha256.clone(),
                &dry_run,
                &format!("sha256:{}", "d".repeat(64)),
            ),
            Err(InformationServiceError::Denied)
        );
        batch
            .approve(
                "reviewer",
                &batch.file_sha256.clone(),
                &dry_run,
                &format!("sha256:{}", "d".repeat(64)),
            )
            .unwrap();
        batch
            .record_row_receipt(ImportRowReceipt {
                row_number: 1,
                command_id: Uuid::from_u128(31),
                source_receipt_ref: Some("finance-receipt-31".into()),
                rejection_code: None,
            })
            .unwrap();
        assert_eq!(batch.status, ImportStatus::Reconciled);
    }

    #[test]
    fn bounded_search_and_controlled_extension_reject_unsafe_shapes() {
        let request = SearchRequest {
            principal_ref: "user".into(),
            scope: scope(),
            purpose: "find invoice".into(),
            query: "*".into(),
            query_sha256: format!("sha256:{}", "a".repeat(64)),
            page_size: 20,
            cursor: None,
        };
        assert_eq!(request.validate(), Err(InformationServiceError::Invalid));

        let mut extension = ControlledExtension {
            extension_id: Uuid::from_u128(40),
            version: 1,
            kind: ExtensionKind::CustomField,
            owner_ref: "finance-owner".into(),
            scope: scope(),
            purpose: "local invoice classification".into(),
            affected_context: ContextId::Finance,
            fields: vec![ExtensionField {
                key: "risk_note".into(),
                field_type: ExtensionFieldType::Text,
                classification: DataClassification::Restricted,
                required: false,
                searchable: true,
                reportable: false,
            }],
            workflow_state_map: HashMap::new(),
            approval_sha256: None,
            active: false,
        };
        assert_eq!(
            extension.activate(format!("sha256:{}", "e".repeat(64))),
            Err(InformationServiceError::Denied)
        );

        let grant = ExportDownloadGrant {
            grant_id: Uuid::from_u128(50),
            export_id: Uuid::from_u128(51),
            recipient_ref: "recipient".into(),
            manifest_sha256: format!("sha256:{}", "f".repeat(64)),
            expires_at: at() + Duration::minutes(5),
            revoked_at: None,
        };
        assert_eq!(
            grant.authorize("other", &grant.manifest_sha256, at()),
            Err(InformationServiceError::Denied)
        );
    }
}
