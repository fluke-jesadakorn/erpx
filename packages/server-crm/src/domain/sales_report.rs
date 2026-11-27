use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesReportQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub branch_id: Option<String>,
    pub property_id: Option<String>,
    pub campaign_id: Option<String>,
    pub owner_id: Option<String>,
}

impl SalesReportQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        for date in [&self.from, &self.to].into_iter().flatten() {
            let bytes = date.as_bytes();
            if bytes.len() != 10
                || bytes.iter().enumerate().any(|(i, byte)| {
                    if matches!(i, 4 | 7) {
                        *byte != b'-'
                    } else {
                        !byte.is_ascii_digit()
                    }
                })
            {
                return Err(request_invalid());
            }
        }
        for id in [
            &self.branch_id,
            &self.property_id,
            &self.campaign_id,
            &self.owner_id,
        ]
        .into_iter()
        .flatten()
        {
            if id.len() > 200 || id.chars().any(char::is_control) {
                return Err(request_invalid());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesReportAccess {
    pub crm: bool,
    pub marketing: bool,
    pub hotel: bool,
    pub sales_revenue: bool,
    pub hotel_revenue: bool,
    pub costs: bool,
    pub revision: String,
}

#[derive(Clone, Debug)]
pub struct SalesReportScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub sales_orders: bool,
    pub access: SalesReportAccess,
}

#[derive(Clone, Debug)]
pub struct SalesReportRead {
    pub report: Value,
    pub options: Value,
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_SALES_REPORT_REQUEST_INVALID",
        "Review the report filters and use valid ISO dates.",
    )
}

pub(crate) fn scope_denied(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Filters {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
}

pub(crate) struct Window {
    pub start: i64,
    pub end: i64,
    pub today: String,
}

#[derive(Deserialize)]
pub(crate) struct Lead {
    id: String,
    account_id: Option<String>,
    created_at: i64,
    opportunity_id: Option<String>,
    stage: Option<String>,
    expected_minor: Option<i64>,
    sales_id: Option<String>,
    reservation_id: Option<String>,
    win_source: Option<String>,
    won_at: Option<i64>,
    checked_in_at: Option<i64>,
    source_visible: bool,
    sales_booked_minor: Option<i64>,
    sales_state: Option<String>,
    reservation_state: Option<String>,
    booked_currency: Option<String>,
    currency: Option<String>,
    recognized_minor: Option<i64>,
}

#[derive(Deserialize)]
struct Event {
    lead_id: String,
    opportunity_id: Option<String>,
    created_at: i64,
    action: String,
    loss_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SourceRows {
    leads: Vec<Lead>,
    events: Vec<Event>,
    overdue: Vec<String>,
}

const TIMEZONE: &str = "Asia/Bangkok";

fn definition(f: &Filters, id: &str, unit: &str, formula: &str, source: &str) -> Value {
    json!({"id":id,"unit":unit,"formula":formula,"source":source,"from":f.from,"to":f.to,"timezone":TIMEZONE})
}

fn metric(
    f: &Filters,
    id: &str,
    label: &str,
    value: Value,
    description: (&str, &str, &str),
    ids: Vec<&str>,
) -> Value {
    let (unit, formula, source) = description;
    let mut result = definition(f, id, unit, formula, source);
    result["label"] = json!(label);
    result["value"] = value;
    result["drilldown"] = json!(ids);
    result
}

fn lead_ids<'a>(leads: &[&'a Lead]) -> Vec<&'a str> {
    leads.iter().map(|lead| lead.id.as_str()).collect()
}

fn cluster<'a>(groups: &mut Vec<(&'a str, Vec<&'a str>)>, key: &'a str, id: &'a str) {
    if let Some((_, ids)) = groups.iter_mut().find(|(kind, _)| *kind == key) {
        ids.push(id);
    } else {
        groups.push((key, vec![id]));
    }
}

fn breakdown(groups: Vec<(&str, Vec<&str>)>, field: &str) -> Vec<Value> {
    groups.into_iter().map(|(key, ids)| {
        let count = ids.len();
        let mut seen = HashSet::new();
        let mut value = json!({"count":count,"leadIds":ids.into_iter().filter(|id| seen.insert(*id)).collect::<Vec<_>>()});
        value[field] = json!(key);
        value
    }).collect()
}

fn report_amount(amount: i128) -> Result<Value, BusinessError> {
    // Report JSON is consumed as exact JavaScript integer minor amounts.
    const MAX_SAFE_AMOUNT: i128 = 9_007_199_254_740_991;
    if !(-MAX_SAFE_AMOUNT..=MAX_SAFE_AMOUNT).contains(&amount) {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "CRM_SALES_REPORT_AMOUNT_RANGE",
            "The report total exceeds the supported minor amount range.",
        ));
    }
    Ok(json!(amount as i64))
}

pub(crate) fn report(
    filters: &Filters,
    window: &Window,
    data: SourceRows,
    access: &SalesReportAccess,
) -> Result<Value, BusinessError> {
    let in_window = |at| (window.start..=window.end).contains(&at);
    let mut by_lead = HashMap::<&str, HashSet<&str>>::new();
    let mut transitions = Vec::new();
    let mut loss_reasons = Vec::new();
    for event in &data.events {
        let id = event.lead_id.as_str();
        let action = event.action.as_str();
        by_lead.entry(id).or_default().insert(action);
        if event.created_at < window.start
            || (event.opportunity_id.is_none() && !matches!(action, "disqualify" | "reopen"))
        {
            continue;
        }
        cluster(&mut transitions, action, id);
        if action == "lose" {
            let reason = event
                .loss_reason
                .as_deref()
                .filter(|reason| !reason.is_empty())
                .unwrap_or("unspecified");
            cluster(&mut loss_reasons, reason, id);
        }
    }
    let has = |lead: &Lead, action| {
        by_lead
            .get(lead.id.as_str())
            .is_some_and(|actions| actions.contains(action))
    };
    let cohort: Vec<_> = data
        .leads
        .iter()
        .filter(|lead| in_window(lead.created_at))
        .collect();
    let qualified: Vec<_> = cohort
        .iter()
        .copied()
        .filter(|lead| has(lead, "qualify"))
        .collect();
    let won: Vec<_> = cohort
        .iter()
        .copied()
        .filter(|lead| has(lead, "win") || has(lead, "win-pms"))
        .collect();
    let lost: Vec<_> = cohort
        .iter()
        .copied()
        .filter(|lead| has(lead, "lose"))
        .collect();
    let open: Vec<_> = cohort
        .iter()
        .copied()
        .filter(|lead| {
            lead.opportunity_id.is_some() && !matches!(lead.stage.as_deref(), Some("won" | "lost"))
        })
        .collect();
    let mut repeat = HashMap::<&str, (usize, bool)>::new();
    for lead in &data.leads {
        let sales = lead.win_source.as_deref() == Some("sales");
        if let Some(account_id) = lead.account_id.as_deref()
            && lead.stage.as_deref() == Some("won")
            && (sales || lead.checked_in_at.is_some())
        {
            let (total, recent) = repeat.entry(account_id).or_default();
            *total += 1;
            *recent |= lead_checked_win(lead, sales).is_some_and(in_window);
        }
    }
    let restricted_hotel = data
        .leads
        .iter()
        .any(|lead| lead.win_source.as_deref() == Some("pms") && !lead.source_visible);
    let repeated = (!restricted_hotel).then(|| {
        repeat
            .values()
            .filter(|(total, recent)| *total >= 2 && *recent)
            .count()
    });
    let forecast: i128 = open
        .iter()
        .filter(|lead| lead.currency.as_deref() == Some("THB"))
        .map(|lead| i128::from(lead.expected_minor.unwrap_or_default()))
        .sum();
    let booked: i128 = won
        .iter()
        .map(|lead| {
            if lead.win_source.as_deref() == Some("sales") {
                (lead.booked_currency.as_deref() == Some("THB"))
                    .then_some(lead.sales_booked_minor)
                    .flatten()
            } else {
                (lead.currency.as_deref() == Some("THB"))
                    .then_some(lead.expected_minor)
                    .flatten()
            }
            .map_or(0, i128::from)
        })
        .sum();
    let recognized: i128 = data
        .leads
        .iter()
        .filter_map(|lead| lead.recognized_minor)
        .map(i128::from)
        .sum();
    let count = |id, label, leads: &[&Lead], formula, source| {
        metric(
            filters,
            id,
            label,
            json!(leads.len()),
            ("count", formula, source),
            lead_ids(leads),
        )
    };
    let outcomes: Vec<_> = won.iter().map(|lead| {
        let source_id = lead.sales_id.as_deref().or(lead.reservation_id.as_deref());
        let state = lead.sales_state.as_deref().or(lead.reservation_state.as_deref());
        json!({
            "leadId": lead.id,
            "source": lead.win_source.as_deref().unwrap_or(""),
            "sourceId": if lead.source_visible { source_id.unwrap_or("") } else { "" },
            "currentSourceState": if lead.source_visible { state.unwrap_or("") } else { "Restricted" },
            "sourceAvailable": lead.source_visible
        })
    }).collect();
    let forecast_amount = report_amount(forecast)?;
    let booked_amount = report_amount(booked)?;
    let recognized_amount = report_amount(recognized)?;
    Ok(json!({
        "filters": filters,
        "access": access,
        "metrics": [
            count("received", "Leads received", &cohort, "Distinct leads created in the selected dates", "crm_leads"),
            count("qualified", "Qualified", &qualified, "Cohort leads with a qualification event by the end date", "crm_case_events"),
            count("won", "Won", &won, "Cohort leads with a confirmed Sales or PMS win event by the end date", "crm_case_events"),
            count("lost", "Lost", &lost, "Cohort leads with a loss event by the end date", "crm_case_events"),
            metric(filters, "conversion", "Lead to won conversion", json!((!cohort.is_empty()).then(|| won.len() as f64 / cohort.len() as f64)),
                ("ratio", "Won cohort leads / received cohort leads", "crm_case_events + crm_leads"), vec![]),
            metric(filters, "overdue", "Overdue follow-ups", json!(data.overdue.len()),
                ("count", "Open CRM activities due before today or the report end date", "crm_lead_activities"), data.overdue.iter().map(String::as_str).collect()),
            metric(filters, "repeat", "Repeat customers", json!(repeated),
                ("count", "Accounts with at least two confirmed Sales wins or checked-in PMS reservations across history and at least one outcome in the selected dates", "crm_accounts + crm_opportunities + pms_records"), vec![]),
            metric(filters, "forecast", "Open expected value", forecast_amount,
                ("minor THB", "Sum expected net value of open THB opportunities in the lead cohort", "crm_opportunities"), lead_ids(&open)),
            metric(filters, "booked", "Confirmed booked value", booked_amount,
                ("minor THB", "Sum confirmed THB Sales order net or PMS reservation net at win; later cancellation stays separate", "commerce_orders + pms_records + crm_opportunities"), lead_ids(&won)),
            metric(filters, "recognized", "Posted net revenue", recognized_amount,
                ("minor THB", "THB revenue account credits less debits in posted Finance journals, including posted Sales returns, during the selected dates", "finance_journals + finance_journal_lines"), data.leads.iter().filter(|lead| lead.recognized_minor.is_some()).map(|lead| lead.id.as_str()).collect())
        ],
        "definitions": [
            definition(filters, "transitions", "count", "One opportunity event per stage change, plus lead disqualification or reopening, created during the selected dates", "crm_case_events"),
            definition(filters, "lossReasons", "count", "Loss events created during the selected dates, grouped by recorded reason", "crm_case_events"),
            definition(filters, "outcomes", "record", "Cohort wins with the latest state of the linked Sales order or PMS reservation", "crm_opportunities + commerce_orders + pms_records")
        ],
        "transitions": breakdown(transitions, "action"),
        "lossReasons": breakdown(loss_reasons, "reason"),
        "outcomes": outcomes
    }))
}

fn lead_checked_win(lead: &Lead, sales: bool) -> Option<i64> {
    if sales {
        lead.won_at
    } else {
        lead.checked_in_at
    }
}
