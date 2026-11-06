//! Native CRM lead / opportunity command validation.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

const TITLE_LIMIT: usize = 160;
const NAME_LIMIT: usize = 160;
const TEXT_LIMIT: usize = 2_000;
const ID_LIMIT: usize = 200;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn js_trim(value: &str) -> &str {
    crate::domain::accounts::js_trim(value)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= ID_LIMIT && !value.chars().any(char::is_control)
}

fn valid_text(value: &str, min: usize, max: usize) -> bool {
    let count = js_trim(value).encode_utf16().count();
    (min..=max).contains(&count)
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
    if year == 0 || !(1..=12).contains(&month) {
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

fn valid_expected_value(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 15 {
        return false;
    }
    let mut seen_dot = false;
    let mut int_digits = 0usize;
    let mut frac_digits = 0usize;
    for &b in bytes {
        if b == b'.' {
            if seen_dot {
                return false;
            }
            seen_dot = true;
        } else if b.is_ascii_digit() {
            if seen_dot {
                frac_digits += 1;
                if frac_digits > 2 {
                    return false;
                }
            } else {
                int_digits += 1;
                if int_digits > 12 {
                    return false;
                }
            }
        } else {
            return false;
        }
    }
    int_digits >= 1 && (!seen_dot || frac_digits >= 1)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeadCreateInput {
    #[serde(rename = "branchId")]
    pub branch_id: String,
    pub title: String,
    #[serde(rename = "contactName")]
    pub contact_name: String,
    #[serde(rename = "contactDetail")]
    pub contact_detail: String,
    pub source: String,
    #[serde(rename = "sourceEvidence")]
    pub source_evidence: Option<String>,
    #[serde(rename = "sourceKind")]
    pub source_kind: Option<String>,
    #[serde(rename = "sourceId")]
    pub source_id: Option<String>,
    #[serde(rename = "campaignId")]
    pub campaign_id: Option<String>,
    #[serde(rename = "accountId")]
    pub account_id: Option<String>,
    #[serde(rename = "contactId")]
    pub contact_id: Option<String>,
    pub brief: String,
    #[serde(rename = "dueDate")]
    pub due_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LeadCreatePlan {
    pub branch_id: String,
    pub title: String,
    pub contact_name: String,
    pub contact_detail: String,
    pub source: String,
    pub source_evidence: Option<String>,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
    pub campaign_id: Option<String>,
    pub account_id: Option<String>,
    pub contact_id: Option<String>,
    pub brief: String,
    pub due_date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeadChangeInput {
    pub action: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    #[serde(rename = "ownerId")]
    pub owner_id: Option<String>,
    #[serde(rename = "dueDate")]
    pub due_date: Option<String>,
    pub reason: String,
    #[serde(rename = "customerId")]
    pub customer_id: Option<String>,
    #[serde(rename = "expectedValue")]
    pub expected_value: Option<String>,
    #[serde(rename = "closeDate")]
    pub close_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LeadChangePlan {
    pub action: String,
    pub expected_version: i64,
    pub owner_id: Option<String>,
    pub due_date: Option<String>,
    pub reason: String,
    pub customer_id: Option<String>,
    pub expected_value: Option<String>,
    pub close_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpportunityChangeInput {
    pub action: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    #[serde(rename = "leadVersion")]
    pub lead_version: i64,
    #[serde(rename = "quoteId")]
    pub quote_id: Option<String>,
    #[serde(rename = "quoteVersion")]
    pub quote_version: Option<i64>,
    #[serde(rename = "lossReason")]
    pub loss_reason: Option<String>,
    #[serde(rename = "salesId")]
    pub sales_id: Option<String>,
    #[serde(rename = "salesVersion")]
    pub sales_version: Option<i64>,
    #[serde(rename = "reservationId")]
    pub reservation_id: Option<String>,
    #[serde(rename = "propertyId")]
    pub property_id: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OpportunityChangePlan {
    pub action: String,
    pub expected_version: i64,
    pub lead_version: i64,
    pub quote_id: Option<String>,
    pub quote_version: Option<i64>,
    pub loss_reason: Option<String>,
    pub sales_id: Option<String>,
    pub sales_version: Option<i64>,
    pub reservation_id: Option<String>,
    pub property_id: Option<String>,
    pub reason: String,
    pub stage: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeadActor {
    pub user_id: String,
    pub company_id: String,
    pub can_read: bool,
    pub can_sales: bool,
    pub can_manage: bool,
    pub can_pms: bool,
    pub can_support: bool,
    pub same_person_ids: Vec<String>,
}

impl LeadActor {
    pub fn is_same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

pub fn validate_lead_create(
    actor: &LeadActor,
    input: &LeadCreateInput,
) -> Result<LeadCreatePlan, BusinessError> {
    if !actor.can_sales {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_LEAD_SALES_REQUIRED",
            "Your role does not grant lead create access.",
        ));
    }
    if !actor.can_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_PERMISSION_REQUIRED",
            "Your selected role does not grant CRM read access.",
        ));
    }
    if !valid_id(&input.branch_id) {
        return Err(invalid("CRM_LEAD_INPUT_INVALID", "Use a valid branch."));
    }
    if !valid_text(&input.title, 3, TITLE_LIMIT) {
        return Err(invalid(
            "CRM_LEAD_INPUT_INVALID",
            "Use a bounded lead title.",
        ));
    }
    if !valid_text(&input.contact_name, 2, NAME_LIMIT) {
        return Err(invalid(
            "CRM_LEAD_INPUT_INVALID",
            "Use a bounded contact name.",
        ));
    }
    if !valid_text(&input.contact_detail, 3, TEXT_LIMIT) {
        return Err(invalid(
            "CRM_LEAD_INPUT_INVALID",
            "Use a bounded contact detail.",
        ));
    }
    if !valid_text(&input.source, 3, TEXT_LIMIT) {
        return Err(invalid("CRM_LEAD_INPUT_INVALID", "Use a bounded source."));
    }
    if !valid_text(&input.brief, 3, TEXT_LIMIT) {
        return Err(invalid("CRM_LEAD_INPUT_INVALID", "Use a bounded brief."));
    }
    if !valid_date(&input.due_date) {
        return Err(invalid("CRM_LEAD_INPUT_INVALID", "Use a valid due date."));
    }
    if let Some(value) = &input.source_evidence {
        if !valid_text(value, 3, TEXT_LIMIT) {
            return Err(invalid(
                "CRM_LEAD_INPUT_INVALID",
                "Use bounded source evidence.",
            ));
        }
    }
    if let Some(value) = &input.source_kind {
        if !matches!(value.as_str(), "staff" | "form" | "pms") {
            return Err(invalid(
                "CRM_LEAD_SOURCE_KIND_INVALID",
                "Choose a staff, form or PMS source.",
            ));
        }
    }
    for value in [
        &input.source_id,
        &input.campaign_id,
        &input.account_id,
        &input.contact_id,
    ] {
        if let Some(value) = value {
            if !valid_id(value) {
                return Err(invalid(
                    "CRM_LEAD_INPUT_INVALID",
                    "Use valid linked identifiers.",
                ));
            }
        }
    }
    if input.account_id.is_none() && input.contact_id.is_some() {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "CRM_ACCOUNT_REQUIRED",
            "Select the account for this contact.",
        ));
    }
    Ok(LeadCreatePlan {
        branch_id: input.branch_id.clone(),
        title: js_trim(&input.title).to_owned(),
        contact_name: js_trim(&input.contact_name).to_owned(),
        contact_detail: js_trim(&input.contact_detail).to_owned(),
        source: js_trim(&input.source).to_owned(),
        source_evidence: input
            .source_evidence
            .as_ref()
            .map(|v| js_trim(v).to_owned()),
        source_kind: input.source_kind.clone(),
        source_id: input.source_id.clone(),
        campaign_id: input.campaign_id.clone(),
        account_id: input.account_id.clone(),
        contact_id: input.contact_id.clone(),
        brief: js_trim(&input.brief).to_owned(),
        due_date: input.due_date.clone(),
    })
}

pub fn validate_lead_change(
    actor: &LeadActor,
    input: &LeadChangeInput,
) -> Result<LeadChangePlan, BusinessError> {
    if !actor.can_sales || !actor.can_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_LEAD_SALES_REQUIRED",
            "Your role does not grant lead change access.",
        ));
    }
    if !matches!(
        input.action.as_str(),
        "reassign" | "disqualify" | "reopen" | "qualify"
    ) {
        return Err(invalid(
            "CRM_LEAD_ACTION_INVALID",
            "Choose a supported lead action.",
        ));
    }
    if input.expected_version < 1 {
        return Err(invalid(
            "CRM_LEAD_INPUT_INVALID",
            "Use a positive expected version.",
        ));
    }
    if !valid_text(&input.reason, 3, TEXT_LIMIT) {
        return Err(invalid("CRM_LEAD_INPUT_INVALID", "Use a bounded reason."));
    }
    let reason = js_trim(&input.reason).to_owned();
    match input.action.as_str() {
        "reassign" => {
            let owner_id = input
                .owner_id
                .clone()
                .ok_or_else(|| invalid("CRM_LEAD_INPUT_INVALID", "Use an owner."))?;
            let due_date = input
                .due_date
                .clone()
                .ok_or_else(|| invalid("CRM_LEAD_INPUT_INVALID", "Use a due date."))?;
            if !valid_id(&owner_id) || !valid_date(&due_date) {
                return Err(invalid(
                    "CRM_LEAD_INPUT_INVALID",
                    "Use valid owner and due date.",
                ));
            }
            if !actor.can_manage {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "CRM_MANAGER_REQUIRED",
                    "A sales manager must change ownership or reopen a closed lead.",
                ));
            }
            Ok(LeadChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                owner_id: Some(owner_id),
                due_date: Some(due_date),
                reason,
                customer_id: None,
                expected_value: None,
                close_date: None,
            })
        }
        "qualify" => {
            let customer_id = input
                .customer_id
                .clone()
                .ok_or_else(|| invalid("CRM_LEAD_INPUT_INVALID", "Use a customer."))?;
            let expected_value = input
                .expected_value
                .clone()
                .ok_or_else(|| invalid("CRM_LEAD_INPUT_INVALID", "Use an expected value."))?;
            let close_date = input
                .close_date
                .clone()
                .ok_or_else(|| invalid("CRM_LEAD_INPUT_INVALID", "Use a close date."))?;
            if !valid_id(&customer_id)
                || !valid_expected_value(&expected_value)
                || !valid_date(&close_date)
            {
                return Err(invalid(
                    "CRM_LEAD_INPUT_INVALID",
                    "Use valid qualification fields.",
                ));
            }
            Ok(LeadChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                owner_id: None,
                due_date: None,
                reason,
                customer_id: Some(customer_id),
                expected_value: Some(expected_value),
                close_date: Some(close_date),
            })
        }
        "reopen" => {
            if !actor.can_manage {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "CRM_MANAGER_REQUIRED",
                    "A sales manager must change ownership or reopen a closed lead.",
                ));
            }
            Ok(LeadChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                owner_id: None,
                due_date: None,
                reason,
                customer_id: None,
                expected_value: None,
                close_date: None,
            })
        }
        "disqualify" => Ok(LeadChangePlan {
            action: input.action.clone(),
            expected_version: input.expected_version,
            owner_id: None,
            due_date: None,
            reason,
            customer_id: None,
            expected_value: None,
            close_date: None,
        }),
        _ => Err(invalid(
            "CRM_LEAD_ACTION_INVALID",
            "Choose a supported lead action.",
        )),
    }
}

pub fn validate_opportunity_change(
    actor: &LeadActor,
    input: &OpportunityChangeInput,
) -> Result<OpportunityChangePlan, BusinessError> {
    if !actor.can_sales || !actor.can_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_OPPORTUNITY_SALES_REQUIRED",
            "Your role does not grant opportunity change access.",
        ));
    }
    if !matches!(
        input.action.as_str(),
        "link-quote" | "negotiate" | "reopen" | "lose" | "win" | "win-pms"
    ) {
        return Err(invalid(
            "CRM_OPPORTUNITY_ACTION_INVALID",
            "Choose a supported opportunity action.",
        ));
    }
    if input.expected_version < 1 || input.lead_version < 1 {
        return Err(invalid(
            "CRM_OPPORTUNITY_INPUT_INVALID",
            "Use positive expected versions.",
        ));
    }
    if !valid_text(&input.reason, 3, TEXT_LIMIT) {
        return Err(invalid(
            "CRM_OPPORTUNITY_INPUT_INVALID",
            "Use a bounded reason.",
        ));
    }
    let reason = js_trim(&input.reason).to_owned();
    let stage = match input.action.as_str() {
        "lose" => "lost",
        "reopen" => "discovery",
        "win" | "win-pms" => "won",
        "link-quote" => "proposal",
        "negotiate" => "negotiation",
        _ => "discovery",
    }
    .to_owned();
    match input.action.as_str() {
        "link-quote" => {
            let quote_id = input
                .quote_id
                .clone()
                .ok_or_else(|| invalid("CRM_OPPORTUNITY_INPUT_INVALID", "Use a quote."))?;
            let quote_version = input
                .quote_version
                .ok_or_else(|| invalid("CRM_OPPORTUNITY_INPUT_INVALID", "Use a quote version."))?;
            if !valid_id(&quote_id) || quote_version < 1 {
                return Err(invalid(
                    "CRM_OPPORTUNITY_INPUT_INVALID",
                    "Use valid quote identifiers.",
                ));
            }
            Ok(OpportunityChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                lead_version: input.lead_version,
                quote_id: Some(quote_id),
                quote_version: Some(quote_version),
                loss_reason: None,
                sales_id: None,
                sales_version: None,
                reservation_id: None,
                property_id: None,
                reason,
                stage,
            })
        }
        "win" => {
            let sales_id = input
                .sales_id
                .clone()
                .ok_or_else(|| invalid("CRM_OPPORTUNITY_INPUT_INVALID", "Use a sales order."))?;
            let sales_version = input
                .sales_version
                .ok_or_else(|| invalid("CRM_OPPORTUNITY_INPUT_INVALID", "Use a sales version."))?;
            if !valid_id(&sales_id) || sales_version < 1 {
                return Err(invalid(
                    "CRM_OPPORTUNITY_INPUT_INVALID",
                    "Use valid sales identifiers.",
                ));
            }
            Ok(OpportunityChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                lead_version: input.lead_version,
                quote_id: None,
                quote_version: None,
                loss_reason: None,
                sales_id: Some(sales_id),
                sales_version: Some(sales_version),
                reservation_id: None,
                property_id: None,
                reason,
                stage,
            })
        }
        "win-pms" => {
            let reservation_id = input
                .reservation_id
                .clone()
                .ok_or_else(|| invalid("CRM_OPPORTUNITY_INPUT_INVALID", "Use a reservation."))?;
            let property_id = input
                .property_id
                .clone()
                .ok_or_else(|| invalid("CRM_OPPORTUNITY_INPUT_INVALID", "Use a property."))?;
            if !valid_id(&reservation_id) || !valid_id(&property_id) {
                return Err(invalid(
                    "CRM_OPPORTUNITY_INPUT_INVALID",
                    "Use valid reservation identifiers.",
                ));
            }
            if !actor.can_pms {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "CRM_OPPORTUNITY_PMS_REQUIRED",
                    "Your role does not grant PMS opportunity access.",
                ));
            }
            Ok(OpportunityChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                lead_version: input.lead_version,
                quote_id: None,
                quote_version: None,
                loss_reason: None,
                sales_id: None,
                sales_version: None,
                reservation_id: Some(reservation_id),
                property_id: Some(property_id),
                reason,
                stage,
            })
        }
        "lose" => {
            let loss_reason = input.loss_reason.clone().unwrap_or_else(|| "other".into());
            if !matches!(
                loss_reason.as_str(),
                "price" | "availability" | "competitor" | "timing" | "no_response" | "other"
            ) {
                return Err(invalid(
                    "CRM_OPPORTUNITY_LOSS_REASON_INVALID",
                    "Choose a supported loss reason.",
                ));
            }
            Ok(OpportunityChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                lead_version: input.lead_version,
                quote_id: None,
                quote_version: None,
                loss_reason: Some(loss_reason),
                sales_id: None,
                sales_version: None,
                reservation_id: None,
                property_id: None,
                reason,
                stage,
            })
        }
        "reopen" => {
            if !actor.can_manage {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "CRM_MANAGER_REQUIRED",
                    "A sales manager must reopen a lost opportunity.",
                ));
            }
            Ok(OpportunityChangePlan {
                action: input.action.clone(),
                expected_version: input.expected_version,
                lead_version: input.lead_version,
                quote_id: None,
                quote_version: None,
                loss_reason: None,
                sales_id: None,
                sales_version: None,
                reservation_id: None,
                property_id: None,
                reason,
                stage,
            })
        }
        "negotiate" => Ok(OpportunityChangePlan {
            action: input.action.clone(),
            expected_version: input.expected_version,
            lead_version: input.lead_version,
            quote_id: None,
            quote_version: None,
            loss_reason: None,
            sales_id: None,
            sales_version: None,
            reservation_id: None,
            property_id: None,
            reason,
            stage,
        }),
        _ => Err(invalid(
            "CRM_OPPORTUNITY_ACTION_INVALID",
            "Choose a supported opportunity action.",
        )),
    }
}

pub fn require_lead_state_for_change(
    action: &str,
    status: &str,
    stage: Option<&str>,
    is_owner: bool,
    can_manage: bool,
) -> Result<(), BusinessError> {
    match action {
        "reassign" | "reopen" => {
            if !can_manage {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "CRM_MANAGER_REQUIRED",
                    "A sales manager must change ownership or reopen a closed lead.",
                ));
            }
        }
        _ => {
            if !is_owner {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "CRM_OWNER_REQUIRED",
                    "The assigned salesperson must qualify this lead.",
                ));
            }
        }
    }
    let closed_for_reassign = stage == Some("won") || status == "disqualified";
    let allowed = match action {
        "reopen" => status == "disqualified",
        "reassign" => !closed_for_reassign,
        _ => status == "open",
    };
    if !allowed {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CRM_STATE_CHANGED",
            "Review the current lead or opportunity state.",
        ));
    }
    Ok(())
}

pub fn require_opportunity_state(
    action: &str,
    stage: &str,
    is_owner: bool,
    can_manage: bool,
) -> Result<(), BusinessError> {
    if action == "reopen" {
        if !can_manage {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CRM_MANAGER_REQUIRED",
                "A sales manager must reopen a lost opportunity.",
            ));
        }
        if stage != "lost" {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "CRM_STATE_CHANGED",
                "Only a lost opportunity can be reopened.",
            ));
        }
        return Ok(());
    }
    if !is_owner {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_OWNER_REQUIRED",
            "The assigned salesperson must act on the opportunity.",
        ));
    }
    if !matches!(stage, "discovery" | "proposal" | "negotiation") {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CRM_STATE_CHANGED",
            "This opportunity is closed.",
        ));
    }
    Ok(())
}
