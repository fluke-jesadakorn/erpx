//! Pure rules for the native Agent assistant-view read. No SQL, no HTTP and no
//! authority lives here: this module only validates the bounded request and
//! reproduces the page/guide and conversation-key derivation of the retained
//! TypeScript application.
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// The two conversation modes accepted by `GET /assistant`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistantViewMode {
    Page,
    Workspace,
}

impl AssistantViewMode {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "page" => Ok(Self::Page),
            "workspace" => Ok(Self::Workspace),
            _ => Err(invalid(
                "AI_CONTEXT_UNSUPPORTED",
                "Use one bounded assistant view mode.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Workspace => "workspace",
        }
    }
}

/// Bounded request for the native assistant view. Unknown or malformed paths
/// fail closed before any port is touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistantViewRequest {
    pub path: String,
    pub mode: AssistantViewMode,
}

/// Canonicalise an assistant page path exactly as `assistantPage` does: strip
/// navigation-only query keys, sort the remaining keys and values, and bound
/// each value. Returns the canonical path and its guide root.
pub fn canonical_page(path: &str) -> Result<(String, String), BusinessError> {
    let unsupported = || {
        invalid(
            "AI_CONTEXT_UNSUPPORTED",
            "This page is not available to the assistant yet.",
        )
    };
    if !path.starts_with('/') || path.starts_with("//") || path.contains('\\') {
        return Err(unsupported());
    }
    let (raw_path, raw_query) = match path.split_once('?') {
        Some((head, query)) => (head, Some(query)),
        None => (path, None),
    };
    if raw_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .any(|segment| segment == "." || segment == ".." || segment.contains('%'))
    {
        return Err(unsupported());
    }
    let segments: Vec<&str> = raw_path.split('/').filter(|s| !s.is_empty()).collect();
    let root = segments.first().copied().ok_or_else(unsupported)?;
    if !root.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
        || segments.iter().any(|s| {
            !s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
        })
        || !supported_root(root)
    {
        return Err(unsupported());
    }
    let mut query: Vec<(String, String)> = Vec::new();
    if let Some(raw_query) = raw_query {
        for pair in raw_query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
            if matches!(
                key,
                "cursor" | "limit" | "sort" | "return" | "frontReturn" | "offset" | "movementCursor"
            ) {
                continue;
            }
            let value: String = raw_value.chars().take(300).collect();
            query.push((key.to_owned(), value));
        }
    }
    query.sort();
    query.dedup();
    let canonical = format!(
        "/{}{}",
        segments.join("/"),
        if query.is_empty() {
            String::new()
        } else {
            format!(
                "?{}",
                query
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("&")
            )
        }
    );
    Ok((canonical, root.to_owned()))
}

/// Roots with a manual guide. Unknown roots are rejected, matching the
/// TypeScript `assistantPage` guard rather than inventing a new context.
pub fn supported_root(root: &str) -> bool {
    const ROOTS: &[&str] = &[
        "home", "work", "requests", "expenses", "commerce", "finance", "people", "calendar",
        "staff-tasks", "projects", "handbook", "learn", "pos", "pms", "inbox", "leave",
        "overtime", "settings", "customer-followups", "customer-duplicates", "customer-access",
        "quality", "stock-transit", "stock-counts", "stock-transfers", "procurement-rfqs",
        "supplier-bills", "supplier-promises", "routine-templates", "routine-schedules",
        "executive", "posts", "agents", "crm", "customers", "support", "technology", "legal",
        "assets", "maintenance", "field-service", "planning", "manufacturing", "talent",
        "benefits", "marketing", "ecommerce", "operations", "records", "files", "inventory",
        "procurement", "sales", "company", "dashboard", "reports",
    ];
    ROOTS.contains(&root)
}

/// The retained manual guide for a root. The explicit guides are reproduced
/// verbatim; other discoverable roots use the standard owner-guide shape.
pub fn page_guide(root: &str, title_override: Option<&str>) -> Value {
    let guide = match root {
        "home" => json!({"title":"Today","purpose":"Understand team activity, share updates and hand over linked work.","steps":["Read Latest updates or Following for team conversations.","Use Start to begin work or Calendar to plan the day.","Open My work for your actions, approvals and tracked requests."],"prompts":["Summarize my team's recent updates.","Explain how to start work."]}),
        "work" => json!({"title":"My work","purpose":"Act, decide and track work through its responsible owner.","steps":["Use My actions for steps you can perform and Approvals for permitted decisions.","Follow My requests and Waiting under Tracking.","Open linked work for evidence, the current owner and next step."],"prompts":["Summarize this queue and its blockers.","What information should I review before deciding?"]}),
        "requests" => json!({"title":"Request","purpose":"Make the need clear and follow approval and fulfilment separately.","steps":["Save a clear description of the need and location.","Use AI preparation to review proposed wording and category.","Review the responsible queue and next action before submitting."],"prompts":["Summarize this request and what is missing.","Explain the next step."]}),
        "expenses" => json!({"title":"Expense","purpose":"Turn supporting documents into a reviewed expense draft.","steps":["Attach the receipt or invoice to this draft.","Use document reading, then compare proposed fields with the original evidence.","Check amounts, currency, tax and duplicates before submitting for approval."],"prompts":["Check whether this expense is complete.","Explain how to review receipt extraction."]}),
        "commerce" => json!({"title":"Sales & purchasing","purpose":"Follow an order from preparation through fulfilment and accounting.","steps":["Choose the correct order type and counterparty.","Check products, quantities, prices and delivery details.","Review approvals, then record fulfilment before invoicing."],"prompts":["Explain the workflow on this page.","What should I check before confirming?"]}),
        "finance" => json!({"title":"Finance","purpose":"Review accounting evidence before posting or settling transactions.","steps":["Choose the accounting period and document type.","Reconcile amounts, currency and source evidence.","Review the approval and posting status; payment is a separate action."],"prompts":["Explain what I should check on this page.","Summarize the status from the supplied information."]}),
        "people" => json!({"title":"People","purpose":"Keep a person's employment record and account access consistent.","steps":["Locate the person and the correct company engagement.","Review effective dates, position and reporting line.","Review access changes through the applicable approval process."],"prompts":["Explain the difference between an account and an employee.","What should I check when an employee changes teams?"]}),
        "calendar" => json!({"title":"Calendar","purpose":"Understand work commitments in the selected date range and timezone.","steps":["Choose your date range, team scope and timezone.","Select the event layers needed for this task.","Open an event to use its owning workflow; a calendar entry is not an approval."],"prompts":["Explain how to choose calendar layers.","Why are some event details unavailable?"]}),
        "staff-tasks" => json!({"title":"Staff task","purpose":"Complete assigned work with clear evidence and handover.","steps":["Read the task, owner, due date and blocked reason.","Record progress and attach evidence.","Submit completion for review when required."],"prompts":["Summarize this task and its next steps.","What evidence should I prepare?"]}),
        "projects" => json!({"title":"Projects","purpose":"Coordinate related work, ownership and progress.","steps":["Open the project and confirm its scope.","Review tasks, owners and dependencies.","Update the owning task with progress and blockers."],"prompts":["Explain how to track project progress.","How should I handle blocked work?"]}),
        "handbook" => json!({"title":"Company handbook","purpose":"Use published company guidance and keep drafts under review.","steps":["Search for the relevant policy or procedure.","Check the published revision and its applicability.","Editors prepare a new revision before publishing changes."],"prompts":["Explain the procedure in this handbook.","Summarize the actions required by this article."]}),
        "learn" => json!({"title":"Guides","purpose":"Learn the workflow you are working in and consult approved guidance.","steps":["Choose a workflow you want to learn.","Open that page and ask the assistant to explain its steps.","Use published handbook articles for company-specific policy."],"prompts":["Teach me how to get started with ERPX.","Which preparation tasks can AI help with?"]}),
        "pos" => json!({"title":"Point of sale","purpose":"Prepare a sale and complete payment and fulfilment using the POS workflow.","steps":["Confirm the location, session and selected products.","Review quantity, stock and order total.","Complete payment and fulfilment using the available POS actions."],"prompts":["Explain sales and fulfillment.","What should I check when a payment fails?"]}),
        "pms" => json!({"title":"Hotel operations","purpose":"Keep reservations, stays, rooms and guest charges aligned.","steps":["Select the property and the correct reservation or stay.","Check dates, room availability and guest account.","Use the owning PMS action for check-in, charges and check-out."],"prompts":["Explain the check-in workflow.","What should I check in the guest account?"]}),
        "inbox" => json!({"title":"Inbox","purpose":"Read work conversations and follow up in the related workflow.","steps":["Choose the relevant conversation.","Review its participants and linked work.","Open linked work to take the business action."],"prompts":["Explain the difference between a chat and an approval."]}),
        "leave" => json!({"title":"Leave","purpose":"Prepare a leave request against current eligibility and entitlement.","steps":["Check the leave type, dates and available entitlement.","Review the requested duration and required evidence.","Submit and follow the approval status."],"prompts":["What should I prepare for a leave request?"]}),
        "overtime" => json!({"title":"Overtime","purpose":"Prepare overtime hours and evidence for review.","steps":["Check eligibility and the actual work period.","Provide the reason and supporting evidence.","Review hours before submission; payroll uses approved records."],"prompts":["Explain how to request overtime."]}),
        "settings" => json!({"title":"Settings","purpose":"Review and test a configuration before activating it.","steps":["Select the configuration area.","Review the current active settings and any draft changes.","Test and activate only the reviewed revision."],"prompts":["Explain the AI settings.","Why should I test before activation?"]}),
        "posts" => json!({"title":"Conversation","purpose":"Read work conversations and follow up in the related workflow.","steps":["Choose the relevant conversation.","Review its participants and linked work.","Open linked work to take the business action."],"prompts":["Explain the difference between a chat and an approval."]}),
        _ => json!({"title":"Assistant","purpose":"Open the source record and check its current status and responsible person.","steps":["Review evidence and the permitted next step in the owning workspace.","Confirm your action there, then follow progress in My work."],"prompts":["Explain this workflow.","What should I review before continuing?"]}),
    };
    match title_override {
        Some(title) => {
            let mut guide = guide;
            if let Some(map) = guide.as_object_mut() {
                map.insert("title".into(), Value::String(title.to_owned()));
            }
            guide
        }
        None => guide,
    }
}

/// Conversation key. Workspace mode requires the cross-context permission and
/// deliberately joins all pages under one `"workspace"` conversation.
pub fn conversation_key(
    canonical_path: &str,
    mode: AssistantViewMode,
    cross_context: bool,
) -> Result<String, BusinessError> {
    match mode {
        AssistantViewMode::Workspace if !cross_context => Err(invalid(
            "AI_CONTEXT_FORBIDDEN",
            "Your current role allows one page at a time.",
        )),
        AssistantViewMode::Workspace => Ok("workspace".into()),
        AssistantViewMode::Page => Ok(canonical_path.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_page_sorts_and_drops_navigation_keys() {
        let (path, root) = canonical_page("/work?view=actions&cursor=5&queue=todo").unwrap();
        assert_eq!(path, "/work?queue=todo&view=actions");
        assert_eq!(root, "work");
    }

    #[test]
    fn canonical_page_fails_closed() {
        assert!(canonical_page("work").is_err());
        assert!(canonical_page("/unknown").is_err());
        assert!(canonical_page("/work/..").is_err());
        assert!(canonical_page("/work?q=%20").is_ok());
    }

    #[test]
    fn modes_and_conversation_keys_are_bounded() {
        assert!(AssistantViewMode::parse("other").is_err());
        assert_eq!(
            conversation_key("/work", AssistantViewMode::Page, false).unwrap(),
            "/work"
        );
        assert!(conversation_key("/work", AssistantViewMode::Workspace, false).is_err());
        assert_eq!(
            conversation_key("/work", AssistantViewMode::Workspace, true).unwrap(),
            "workspace"
        );
    }
}
