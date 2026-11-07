//! Native CRM lead/opportunity pipeline read facts.
//!
//! These are owner read projections only. They never create, qualify, reassign
//! or close a lead or opportunity. Composition retains the normal Account /
//! Holding session, current authority, package admission and serializable
//! owner transaction; this domain enforces the Company and selected
//! branch/department scope, `commerce.read`, the CRM module, work-context
//! expiry, bounded search/stage/cursor input and exact row/forecast shapes
//! before presentation.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PAGE_SIZE: usize = 30;
pub const SEARCH_LIMIT: usize = 200;
const CURSOR_LIMIT: usize = 4_096;
const NAME_LIMIT: usize = 300;
const TITLE_LIMIT: usize = 160;

#[derive(Clone, Debug)]
pub struct CrmPipelineActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl CrmPipelineActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.is_empty() || self.modules.iter().any(|value| value == module)
    }

    pub fn can_create(&self) -> bool {
        self.has("commerce.sales")
    }

    pub fn require_pipeline(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CRM_PERMISSION_REQUIRED",
                "Your selected role does not grant CRM read access.",
            ));
        }
        if !self.has_module("crm") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CRM_MODULE_FORBIDDEN",
                "Your current role cannot access the CRM pipeline.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_PIPELINE_SOURCE_INVALID",
        "The current CRM pipeline source is invalid.",
    )
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_PIPELINE_REQUEST_INVALID",
        "Use one bounded CRM pipeline request.",
    )
}

pub(crate) fn cursor_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_PIPELINE_CURSOR_INVALID",
        "Restart this CRM pipeline search to load results.",
    )
}

fn too_large() -> BusinessError {
    BusinessError::invalid(
        "CRM_PIPELINE_TOO_LARGE",
        "Narrow this CRM pipeline search before reading it.",
    )
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    if !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn valid_status(value: &str) -> bool {
    matches!(value, "open" | "qualified" | "disqualified")
}

fn valid_stage(value: &str) -> bool {
    matches!(
        value,
        "discovery" | "proposal" | "negotiation" | "won" | "lost"
    )
}

fn valid_stage_filter(value: &str) -> bool {
    matches!(
        value,
        "open"
            | "qualified"
            | "disqualified"
            | "discovery"
            | "proposal"
            | "negotiation"
            | "won"
            | "lost"
    )
}

pub(crate) fn validate_directory_name(value: &str) -> Result<(), BusinessError> {
    if !valid_text(value, NAME_LIMIT) {
        return Err(source_invalid());
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CrmPipelineQuery {
    pub search: String,
    pub stage: Option<String>,
    pub cursor: Option<String>,
}

impl CrmPipelineQuery {
    pub fn normalized_search(&self) -> String {
        self.search.trim().to_owned()
    }

    pub fn normalized_stage(&self) -> Result<Option<String>, BusinessError> {
        let Some(stage) = self.stage.as_deref() else {
            return Ok(None);
        };
        if stage.is_empty() {
            return Ok(None);
        }
        if !valid_stage_filter(stage) {
            return Err(BusinessError::invalid(
                "CRM_PIPELINE_STAGE_INVALID",
                "Choose an available CRM pipeline stage.",
            ));
        }
        Ok(Some(stage.to_owned()))
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.search.len() > SEARCH_LIMIT || self.search.chars().any(char::is_control) {
            return Err(request_invalid());
        }
        self.normalized_stage()?;
        if self
            .cursor
            .as_deref()
            .is_some_and(|value| value.is_empty() || value.len() > CURSOR_LIMIT)
        {
            return Err(cursor_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CrmPipelineRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub contact_name: String,
    pub status: String,
    pub stage: Option<String>,
    pub opportunity_id: Option<String>,
    pub expected_minor: Option<i64>,
    pub currency: Option<String>,
    pub close_date: Option<String>,
    pub due_date: String,
    pub owner_id: String,
    pub owner_name: Option<String>,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub version: i64,
    pub opportunity_version: Option<i64>,
    pub updated_at: i64,
}

impl CrmPipelineRow {
    pub fn validate(
        &self,
        actor: &CrmPipelineActor,
        search: &str,
        stage: Option<&str>,
    ) -> Result<(), BusinessError> {
        let opportunity = self.opportunity_id.is_some();
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.owner_id)
            || !valid_text(&self.title, TITLE_LIMIT)
            || !valid_text(&self.contact_name, NAME_LIMIT)
            || !valid_status(&self.status)
            || !valid_date(&self.due_date)
            || self.version < 1
            || self.updated_at < 0
            || self
                .branch_id
                .as_deref()
                .is_none_or(|value| !valid_identifier(value))
            || self
                .department_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self
                .owner_name
                .as_deref()
                .is_some_and(|value| !valid_text(value, NAME_LIMIT))
            || self
                .customer_name
                .as_deref()
                .is_some_and(|value| !valid_text(value, NAME_LIMIT))
        {
            return Err(source_invalid());
        }
        if opportunity {
            if !self.opportunity_id.as_deref().is_some_and(valid_identifier)
                || !self.stage.as_deref().is_some_and(valid_stage)
                || !self.customer_id.as_deref().is_some_and(valid_identifier)
                || self
                    .currency
                    .as_deref()
                    .is_none_or(|value| !valid_currency(value))
                || self
                    .close_date
                    .as_deref()
                    .is_none_or(|value| !valid_date(value))
                || self.expected_minor.is_none_or(|value| value < 0)
                || self.opportunity_version.is_none_or(|value| value < 1)
                || self.status != "qualified"
            {
                return Err(source_invalid());
            }
        } else if self.stage.is_some()
            || self.expected_minor.is_some()
            || self.currency.is_some()
            || self.close_date.is_some()
            || self.customer_id.is_some()
            || self.customer_name.is_some()
            || self.opportunity_version.is_some()
        {
            return Err(source_invalid());
        }
        if self.status == "qualified" && !opportunity {
            return Err(source_invalid());
        }
        if actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| self.branch_id.as_deref() != Some(branch.as_str()))
        {
            return Err(source_invalid());
        }
        if actor
            .department_id
            .as_ref()
            .is_some_and(|department| self.department_id.as_deref() != Some(department.as_str()))
        {
            return Err(source_invalid());
        }
        if !search.is_empty()
            && !self.title.to_lowercase().contains(&search.to_lowercase())
            && !self
                .contact_name
                .to_lowercase()
                .contains(&search.to_lowercase())
        {
            return Err(source_invalid());
        }
        if stage.is_some_and(|value| self.stage.as_deref().unwrap_or(self.status.as_str()) != value)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrmPipelineCursor {
    pub updated_at: i64,
    pub id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    scope: String,
    updated_at: i64,
    id: String,
}

pub(crate) fn cursor_scope(
    actor: &CrmPipelineActor,
    search: &str,
    stage: Option<&str>,
) -> Result<String, BusinessError> {
    serde_json::to_string(&(
        actor.company_id.as_str(),
        actor.branch_id.as_ref().map(RecordId::as_str),
        actor.department_id.as_ref().map(RecordId::as_str),
        search,
        stage,
    ))
    .map_err(|_| cursor_invalid())
}

pub(crate) fn encode_cursor(
    scope: &str,
    cursor: &CrmPipelineCursor,
) -> Result<String, BusinessError> {
    let wire = CursorWire {
        scope: scope.to_owned(),
        updated_at: cursor.updated_at,
        id: cursor.id.clone(),
    };
    let value = serde_json::to_string(&wire).map_err(|_| cursor_invalid())?;
    if value.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    Ok(value)
}

pub(crate) fn decode_cursor(value: &str, scope: &str) -> Result<CrmPipelineCursor, BusinessError> {
    if value.is_empty() || value.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    let wire: CursorWire = serde_json::from_str(value).map_err(|_| cursor_invalid())?;
    if wire.scope != scope || wire.updated_at < 0 || !valid_identifier(&wire.id) {
        return Err(cursor_invalid());
    }
    Ok(CrmPipelineCursor {
        updated_at: wire.updated_at,
        id: wire.id,
    })
}

#[derive(Clone, Debug)]
pub struct CrmPipelinePageData {
    pub items: Vec<CrmPipelineRow>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

pub(crate) fn validate_page(
    actor: &CrmPipelineActor,
    search: &str,
    stage: Option<&str>,
    cursor: Option<&CrmPipelineCursor>,
    source: Vec<CrmPipelineRow>,
    scope: &str,
) -> Result<CrmPipelinePageData, BusinessError> {
    if source.len() > PAGE_SIZE + 1 {
        return Err(too_large());
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<(i64, String)> = None;
    for row in &source {
        row.validate(actor, search, stage)?;
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
        let current = (row.updated_at, row.id.clone());
        if previous
            .as_ref()
            .is_some_and(|previous| current >= *previous)
        {
            return Err(source_invalid());
        }
        if cursor.is_some_and(|cursor| (cursor.updated_at, cursor.id.clone()) <= current) {
            return Err(source_invalid());
        }
        previous = Some(current);
    }
    let has_more = source.len() > PAGE_SIZE;
    let mut items = source;
    if has_more {
        items.truncate(PAGE_SIZE);
    }
    let next_cursor = if has_more {
        items
            .last()
            .map(|row| {
                encode_cursor(
                    scope,
                    &CrmPipelineCursor {
                        updated_at: row.updated_at,
                        id: row.id.clone(),
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(CrmPipelinePageData {
        items,
        next_cursor,
        has_more,
    })
}

#[derive(Clone, Debug)]
pub struct CrmForecastSourceRow {
    pub currency: String,
    pub stage: String,
    pub unweighted_minor: i64,
    pub weighted_minor: i64,
    pub won_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CrmForecastTotal {
    #[serde(rename = "unweightedMinor")]
    pub unweighted_minor: String,
    #[serde(rename = "weightedMinor")]
    pub weighted_minor: String,
    #[serde(rename = "wonMinor")]
    pub won_minor: String,
}

pub fn validate_forecast(
    source: Vec<CrmForecastSourceRow>,
) -> Result<std::collections::BTreeMap<String, CrmForecastTotal>, BusinessError> {
    let mut totals: std::collections::BTreeMap<String, (i64, i64, i64)> =
        std::collections::BTreeMap::new();
    for row in source {
        if !valid_currency(&row.currency)
            || !valid_stage(&row.stage)
            || row.unweighted_minor < 0
            || row.weighted_minor < 0
            || row.won_minor < 0
        {
            return Err(source_invalid());
        }
        let coherent = match row.stage.as_str() {
            "won" => row.unweighted_minor == 0 && row.weighted_minor == 0,
            "lost" => row.unweighted_minor == 0 && row.weighted_minor == 0 && row.won_minor == 0,
            _ => row.won_minor == 0,
        };
        if !coherent {
            return Err(source_invalid());
        }
        let total = totals.entry(row.currency).or_insert((0, 0, 0));
        total.0 = total
            .0
            .checked_add(row.unweighted_minor)
            .ok_or_else(source_invalid)?;
        total.1 = total
            .1
            .checked_add(row.weighted_minor)
            .ok_or_else(source_invalid)?;
        total.2 = total
            .2
            .checked_add(row.won_minor)
            .ok_or_else(source_invalid)?;
    }
    Ok(totals
        .into_iter()
        .map(|(currency, (unweighted, weighted, won))| {
            (
                currency,
                CrmForecastTotal {
                    unweighted_minor: unweighted.to_string(),
                    weighted_minor: weighted.to_string(),
                    won_minor: won.to_string(),
                },
            )
        })
        .collect())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CrmPurchaseControls {
    #[serde(rename = "newWork")]
    pub new_work: bool,
    #[serde(rename = "settleExisting")]
    pub settle_existing: bool,
    pub history: bool,
    pub revision: String,
}

#[derive(Clone, Debug)]
pub struct CrmPipelineEntitlement {
    pub enforced: bool,
    pub history_allowed: bool,
    pub new_work: bool,
    pub settle_existing: bool,
    pub revision: String,
}

impl CrmPipelineEntitlement {
    pub fn require_history(&self) -> Result<(), BusinessError> {
        if self.enforced && !self.history_allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "Sales CRM or retained history is required.",
            ));
        }
        Ok(())
    }

    pub fn purchase(&self) -> Option<CrmPurchaseControls> {
        self.enforced.then(|| CrmPurchaseControls {
            new_work: self.new_work,
            settle_existing: self.settle_existing,
            history: self.history_allowed,
            revision: self.revision.clone(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CrmPipelinePage {
    pub items: Vec<CrmPipelineRow>,
    pub forecast: std::collections::BTreeMap<String, CrmForecastTotal>,
    #[serde(rename = "canCreate")]
    pub can_create: bool,
    pub purchase: Option<CrmPurchaseControls>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}
