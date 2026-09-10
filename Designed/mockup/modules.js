/* Module UX specifications. Fixtures model the living docs; no production services. */
const specField = (key, label, type = "text", value = "", options = []) => ({
  key,
  label,
  type,
  value,
  options,
});
const step = (status, action, next, help, gate = "", handoff = "") => ({
  status,
  action,
  next,
  help,
  gate,
  handoff,
});
const MODULES = {};
function defineModule(
  id,
  layout,
  tabs,
  kind,
  title,
  create,
  fields,
  steps,
  focus,
) {
  MODULES[id] = {
    id,
    layout,
    tabs: tabs.split("|"),
    kind,
    title,
    create,
    fields,
    steps,
    focus,
  };
}
defineModule(
  "brand",
  "studio",
  "Drafts|Identity preview|Version history",
  "Brand draft",
  "Autumn store identity",
  "Create brand draft",
  [
    specField("category", "Category", "select", "Store identity", [
      "Store identity",
      "Invoice identity",
    ]),
    specField(
      "notes",
      "Creative direction",
      "textarea",
      "A welcoming autumn palette with accessible purple accents.",
    ),
    specField("locked", "Category lock", "checkbox", false),
  ],
  [
    step(
      "Draft",
      "Preview identity",
      "In review",
      "Review the mark, color and wording before publication.",
      "brand",
    ),
    step(
      "In review",
      "Publish version",
      "Published",
      "Publish one reviewed version atomically.",
      "brand",
    ),
  ],
  "Consistent identity, across every touchpoint",
);
defineModule(
  "commercial",
  "catalog",
  "Catalog|Subscription|Invoices|Entitlements",
  "Catalog revision",
  "Enterprise annual · revision 4",
  "Create catalog revision",
  [
    specField("plan", "Plan name", "text", "Enterprise annual"),
    specField("price", "Annual price per seat (USD)", "number", 240),
    specField(
      "notes",
      "Included capabilities",
      "textarea",
      "All business modules, customer workspace and scoped collaboration.",
    ),
  ],
  [
    step(
      "Draft",
      "Review catalog",
      "In review",
      "Review the plan and included capabilities.",
    ),
    step(
      "In review",
      "Publish catalog",
      "Published",
      "Publish the sample catalog. Customer billing remains provider controlled.",
    ),
  ],
  "A clear view of plans and access",
);
defineModule(
  "identity_access",
  "access",
  "People|Assignments|Access reviews",
  "Role assignment",
  "Warehouse access · renewal",
  "Review an assignment",
  [
    specField("principal", "Person", "text", "Dao Arun"),
    specField("accessRole", "Assigned role", "text", "Warehouse specialist"),
    specField("days", "Renew for days", "number", 30),
    specField(
      "notes",
      "Renewal reason",
      "textarea",
      "Continue receiving and stock duties at the central warehouse.",
    ),
  ],
  [
    step(
      "Expiring",
      "Review renewal",
      "In review",
      "The renewal stays inside the parent assignment.",
      "renewal",
    ),
    step(
      "In review",
      "Renew assignment",
      "Active",
      "Confirm the bounded expiry and keep its audit history.",
      "renewal",
    ),
  ],
  "The right access, for the right time",
);
defineModule(
  "organization",
  "organization",
  "Organization tree|Branches|Departments|Sharing",
  "Branch",
  "Riverside branch",
  "Add branch",
  [
    specField("name", "Branch name", "text", "Riverside branch"),
    specField("company", "Parent company", "text", "Acme Retail Co"),
    specField(
      "notes",
      "Departments and purpose",
      "textarea",
      "Sales, Operations and Customer service. A new retail location.",
    ),
  ],
  [
    step(
      "Draft",
      "Review placement",
      "In review",
      "Confirm the company and department relationships.",
    ),
    step(
      "In review",
      "Activate branch",
      "Active",
      "Make the sample branch available in the workspace selector.",
      "",
      "organization",
    ),
  ],
  "One organization, with clear working spaces",
);
defineModule(
  "platform_administration",
  "service",
  "Service health|Provisioning|Backups",
  "Provisioning job",
  "East depot · provisioning retry",
  "Inspect provisioning",
  [
    specField("customer", "Customer workspace", "text", "Northstar Supply"),
    specField(
      "notes",
      "Recovery plan",
      "textarea",
      "Retry the failed workspace health check using the last verified configuration.",
    ),
  ],
  [
    step(
      "Failed",
      "Review retry",
      "Ready to retry",
      "Inspect the failure and recovery plan before retrying.",
    ),
    step(
      "Ready to retry",
      "Retry provisioning",
      "Active",
      "Run a local progress simulation and inspect the outcome.",
    ),
  ],
  "Reliable service for every customer",
);
defineModule(
  "customer_360",
  "profile",
  "Profiles|Sales summary|Service timeline",
  "Customer profile",
  "Riverstone Market",
  "Open customer profile",
  [],
  [],
  "The customer story, connected",
);
defineModule(
  "customer_authority",
  "access",
  "Bindings|Duplicate review|Access history",
  "Authority review",
  "Riverstone · duplicate review",
  "Review duplicate profiles",
  [
    specField("primary", "Retained profile", "text", "Riverstone Market"),
    specField("duplicate", "Duplicate profile", "text", "Riverstone Mkt"),
    specField(
      "notes",
      "Merge reason",
      "textarea",
      "Verified the same registration and customer contact.",
    ),
  ],
  [
    step(
      "Review needed",
      "Compare profiles",
      "In review",
      "Compare authority bindings and retain the source history.",
    ),
    step(
      "In review",
      "Confirm merge",
      "Merged",
      "Merge only the sample authority profiles; preserve both audit histories.",
    ),
  ],
  "Customer authority you can explain",
);
defineModule(
  "support",
  "queue",
  "Tickets|SLA watch|Incidents",
  "Support ticket",
  "Cold-room delivery temperature",
  "Open ticket",
  [
    specField("priority", "Priority", "select", "High", [
      "Normal",
      "High",
      "Critical",
    ]),
    specField("assignee", "Assigned to", "text", "Mali Srisai"),
    specField(
      "notes",
      "Resolution summary",
      "textarea",
      "Inspect temperature log and arrange a replacement delivery.",
    ),
  ],
  [
    step(
      "Open",
      "Triage ticket",
      "In progress",
      "Confirm priority, ownership and the next customer update.",
    ),
    step(
      "In progress",
      "Resolve ticket",
      "Resolved",
      "Leave a useful resolution summary before resolving.",
      "notes",
    ),
  ],
  "Help customers without losing the context",
);
defineModule(
  "finance",
  "ledger",
  "Journals|Receivables|Payables|Banking|Close checklist",
  "Journal",
  "September stock receipt journal",
  "Create journal",
  [
    specField("debit", "Inventory debit (USD)", "number", 2400),
    specField("credit", "Accounts payable credit (USD)", "number", 2400),
    specField("currency", "Journal currency", "select", "USD", ["USD", "EUR"]),
    specField("fx", "Exchange rate to USD", "number", 1),
    specField(
      "notes",
      "Journal description",
      "textarea",
      "Goods received for the September store refresh.",
    ),
    specField(
      "reconciled",
      "Bank and subledger checks completed",
      "checkbox",
      true,
    ),
  ],
  [
    step(
      "Draft",
      "Review journal",
      "Ready to post",
      "Debits and credits must balance in an open company period.",
      "balance",
    ),
    step(
      "Ready to post",
      "Post journal",
      "Posted",
      "Post the balanced sample entry and preserve its evidence.",
      "balance",
    ),
    step(
      "Posted",
      "Prepare close",
      "Close review",
      "Confirm reconciliation before requesting a separate reviewer.",
      "reconciled",
    ),
    step(
      "Close review",
      "Close period",
      "Closed",
      "A different person must review the company period.",
      "independent",
    ),
  ],
  "Confidence in every balance",
);
defineModule(
  "pos",
  "register",
  "Register|Held sales|Receipts|Refunds",
  "Register sale",
  "Bangkok · counter sale",
  "Start sale",
  [
    specField("quantity", "Canvas tote quantity", "number", 2),
    specField("unitPrice", "Unit price (USD)", "number", 24),
    specField("tender", "Tender received (USD)", "number", 50),
    specField("refund", "Requested refund (USD)", "number", 20),
    specField(
      "notes",
      "Receipt note",
      "textarea",
      "Thank you for shopping with us.",
    ),
  ],
  [
    step(
      "Open",
      "Review tender",
      "Payment review",
      "Confirm the basket and sample tender.",
      "tender",
    ),
    step(
      "Payment review",
      "Complete sale",
      "Completed",
      "Create a sample receipt. No payment is charged.",
      "tender",
    ),
    step(
      "Completed",
      "Review refund",
      "Refund review",
      "Refunds cannot exceed the original settlement.",
      "refund",
    ),
    step(
      "Refund review",
      "Confirm refund",
      "Refunded",
      "Record the bounded refund and keep the original receipt.",
      "refund",
    ),
  ],
  "A smooth checkout, a clear receipt",
);
defineModule(
  "sales",
  "documents",
  "Quotes|Orders|Fulfillment",
  "Quote",
  "Riverstone · autumn store order",
  "Create quote",
  [
    specField("customer", "Customer", "text", "Riverstone Market"),
    specField("amount", "Quote total (USD)", "number", 2400),
    specField("terms", "Payment terms (days)", "number", 30),
    specField(
      "notes",
      "Quote notes",
      "textarea",
      "Supply 100 canvas totes for the September store refresh.",
    ),
  ],
  [
    step(
      "Draft",
      "Send quote",
      "Sent",
      "Review customer, amount and payment terms.",
    ),
    step(
      "Sent",
      "Record acceptance",
      "Accepted",
      "Record the customer acceptance in the sample history.",
    ),
    step(
      "Accepted",
      "Create sales order",
      "Confirmed",
      "Create a separately typed order linked to this quote.",
      "",
      "sales-order",
    ),
    step(
      "Confirmed",
      "Confirm fulfillment",
      "Fulfilled",
      "Confirm the sample delivery and expose the finance handoff.",
      "",
      "finance",
    ),
  ],
  "From a good conversation to a fulfilled order",
);
defineModule(
  "assets",
  "register",
  "Asset register|Condition|Maintenance history",
  "Asset",
  "Cold-room unit CR-02",
  "Register asset",
  [
    specField("serial", "Serial number", "text", "CR-02-884"),
    specField("location", "Location", "text", "Bangkok receiving area"),
    specField(
      "notes",
      "Condition report",
      "textarea",
      "Temperature sensor needs calibration.",
    ),
  ],
  [
    step(
      "Degraded",
      "Begin maintenance",
      "Under maintenance",
      "Keep the asset unavailable while the work order is active.",
    ),
    step(
      "Under maintenance",
      "Return to service",
      "In service",
      "Review the maintenance evidence and updated condition.",
      "evidence",
    ),
  ],
  "Know the condition of every asset",
);
defineModule(
  "inventory",
  "stock",
  "Availability|Movements|Reservations",
  "Stock receipt",
  "Canvas totes · receiving",
  "Receive stock",
  [
    specField("sku", "SKU", "text", "TOTE-CANVAS-01"),
    specField("quantity", "Received quantity", "number", 100),
    specField("expected", "Expected quantity", "number", 100),
    specField(
      "notes",
      "Receiving note",
      "textarea",
      "Counted cartons against the linked goods receipt.",
    ),
  ],
  [
    step(
      "Expected",
      "Review receipt",
      "Checked",
      "Check received quantities and condition.",
      "quantity",
    ),
    step(
      "Checked",
      "Receive stock",
      "Received",
      "Create a sample movement and update available stock.",
      "quantity",
    ),
  ],
  "The stock you need, where you need it",
);
defineModule(
  "maintenance",
  "schedule",
  "Work orders|Schedule|Downtime",
  "Work order",
  "CR-02 · sensor calibration",
  "Open work order",
  [
    specField("scheduled", "Scheduled date", "date", "2026-09-06"),
    specField("technician", "Technician", "text", "Dao Arun"),
    specField("downtime", "Downtime (minutes)", "number", 45),
    specField(
      "notes",
      "Work performed",
      "textarea",
      "Calibrate the temperature sensor and verify the reading.",
    ),
  ],
  [
    step("Open", "Schedule order", "Scheduled", "Set a date and technician."),
    step(
      "Scheduled",
      "Start work",
      "In progress",
      "Record that the asset is under maintenance.",
    ),
    step(
      "In progress",
      "Complete work",
      "Completed",
      "Completion requires sample evidence.",
      "evidence",
    ),
  ],
  "Keep work moving and equipment reliable",
);
defineModule(
  "operations",
  "queue",
  "Exceptions|Area queues|Handovers",
  "Operational exception",
  "Cold-room handover exception",
  "Open exception",
  [
    specField("severity", "Severity", "select", "High", [
      "Normal",
      "High",
      "Critical",
    ]),
    specField("assignee", "Owner", "text", "Dao Arun"),
    specField(
      "notes",
      "Resolution and handover",
      "textarea",
      "Replacement stock isolated; next shift to verify cold-room readings.",
    ),
  ],
  [
    step(
      "Open",
      "Acknowledge exception",
      "Acknowledged",
      "Confirm ownership and urgency.",
    ),
    step(
      "Acknowledged",
      "Escalate to lead",
      "Escalated",
      "Capture why the area lead needs to act.",
    ),
    step(
      "Escalated",
      "Resolve exception",
      "Resolved",
      "Leave a clear resolution for the next shift.",
      "notes",
    ),
    step(
      "Resolved",
      "Close exception",
      "Closed",
      "Confirm that the resolution has been checked.",
    ),
  ],
  "A clear next step for every exception",
);
defineModule(
  "procurement",
  "documents",
  "Requisitions|Purchase orders|Goods receipts",
  "Requisition",
  "Autumn refresh · 100 canvas totes",
  "Create requisition",
  [
    specField("supplier", "Supplier", "text", "Cedar Distribution"),
    specField("quantity", "Requested quantity", "number", 100),
    specField("amount", "Total (USD)", "number", 2400),
    specField(
      "notes",
      "Justification",
      "textarea",
      "Replenish stock for the autumn store project.",
    ),
  ],
  [
    step(
      "Draft",
      "Submit requisition",
      "Pending approval",
      "Explain the need and link supporting work.",
      "notes",
    ),
    step(
      "Pending approval",
      "Approve requisition",
      "Approved",
      "Review requested items, total and business justification.",
    ),
    step(
      "Approved",
      "Create purchase order",
      "Ordered",
      "Create the sample supplier order.",
    ),
    step(
      "Ordered",
      "Accept goods receipt",
      "Received",
      "Review receipt evidence and open the inventory handoff.",
      "evidence",
      "inventory",
    ),
  ],
  "Buy with context, receive with confidence",
);
defineModule(
  "quality",
  "inspection",
  "Inspections|Results|Release gates",
  "Quality inspection",
  "CR-02 · temperature inspection",
  "Create inspection",
  [
    specField("result", "Inspection result", "select", "Fail", [
      "Fail",
      "Pass",
    ]),
    specField("reading", "Measured temperature (°C)", "number", 9),
    specField(
      "notes",
      "Inspection notes",
      "textarea",
      "Temperature must be between 0 and 5 degrees before release.",
    ),
  ],
  [
    step(
      "Open",
      "Record failed inspection",
      "Failed",
      "Capture the measured result and block downstream work.",
      "failed-check",
    ),
    step(
      "Failed",
      "Record recheck",
      "Passed",
      "Enter a passing result and safe temperature.",
      "quality",
    ),
    step(
      "Passed",
      "Release gate",
      "Released",
      "Review the passed result before releasing dependent work.",
      "quality",
    ),
  ],
  "Visible checks before work moves forward",
);
defineModule(
  "legal",
  "documents",
  "Contracts|Document versions|Holds",
  "Contract",
  "Cedar · supply agreement",
  "Create contract",
  [
    specField("party", "Counterparty", "text", "Cedar Distribution"),
    specField("versionLabel", "Document version", "text", "v2"),
    specField("hold", "Legal hold active", "checkbox", true),
    specField(
      "notes",
      "Review notes",
      "textarea",
      "Review delivery terms and the agreed renewal date.",
    ),
  ],
  [
    step(
      "Draft",
      "Review contract",
      "In review",
      "Compare parties, terms and the current document version.",
    ),
    step(
      "In review",
      "Activate contract",
      "Active",
      "Confirm the reviewed version.",
    ),
    step(
      "Active",
      "Archive contract",
      "Archived",
      "A legal hold must be resolved before archiving.",
      "hold",
    ),
  ],
  "Keep commitments and evidence together",
);
defineModule(
  "people_payroll",
  "people",
  "My requests|Team requests|Employees|Payroll summaries",
  "Leave request",
  "Mali · annual leave",
  "Request leave",
  [
    specField("employee", "Employee", "text", "Mali Srisai"),
    specField("start", "First day", "date", "2026-09-14"),
    specField("days", "Leave days", "number", 2),
    specField(
      "notes",
      "Handover note",
      "textarea",
      "The morning shift and customer follow-ups are covered.",
    ),
  ],
  [
    step(
      "Draft",
      "Submit leave request",
      "Pending approval",
      "Review dates and handover coverage.",
    ),
    step(
      "Pending approval",
      "Approve leave",
      "Approved",
      "A manager reviews the request and records the decision.",
    ),
  ],
  "A better workday starts with supported people",
);
defineModule(
  "crm",
  "pipeline",
  "Pipeline|Accounts|Activities",
  "Opportunity",
  "Riverstone · autumn expansion",
  "Create opportunity",
  [
    specField("account", "Account", "text", "Riverstone Market"),
    specField("value", "Opportunity value (USD)", "number", 2400),
    specField(
      "notes",
      "Next step",
      "textarea",
      "Confirm quantities with the customer and prepare a quote.",
    ),
  ],
  [
    step(
      "New",
      "Qualify opportunity",
      "Qualified",
      "Confirm the account need and expected value.",
    ),
    step(
      "Qualified",
      "Record next step",
      "Proposal ready",
      "Add a specific next action.",
      "notes",
    ),
    step(
      "Proposal ready",
      "Hand off to Sales",
      "Handed off",
      "Open the quote with the opportunity context attached.",
      "",
      "sales",
    ),
  ],
  "Keep the next customer conversation moving",
);
defineModule(
  "marketing",
  "studio",
  "Campaigns|Content preview|Channel readiness|Schedule",
  "Campaign",
  "Autumn collection · customer email",
  "Create campaign",
  [
    specField("channel", "Channel", "select", "Email", [
      "Email",
      "Store display",
    ]),
    specField("verified", "Sample channel verified", "checkbox", false),
    specField("scheduled", "Scheduled date", "date", "2026-09-10"),
    specField(
      "notes",
      "Campaign message",
      "textarea",
      "Everyday essentials for a fresh start. Explore our autumn collection.",
    ),
  ],
  [
    step(
      "Draft",
      "Preview campaign",
      "In review",
      "Inspect the content as the customer will see it.",
    ),
    step(
      "In review",
      "Verify readiness",
      "Ready",
      "A verified sample channel is required.",
      "verified",
    ),
    step(
      "Ready",
      "Schedule campaign",
      "Scheduled",
      "Review timing and audience. Nothing will be sent.",
      "verified",
    ),
  ],
  "Thoughtful campaigns, ready for their audience",
);
defineModule(
  "manufacturing",
  "production",
  "Orders|Material readiness|Execution|Quality disposition",
  "Manufacturing order",
  "Autumn welcome kits · assembly",
  "Create manufacturing order",
  [
    specField("quantity", "Planned kits", "number", 100),
    specField("produced", "Reported good output", "number", 100),
    specField("materials", "Materials available", "checkbox", true),
    specField("quality", "Quality disposition approved", "checkbox", false),
    specField(
      "notes",
      "Execution notes",
      "textarea",
      "Assemble canvas tote, welcome card and product guide.",
    ),
  ],
  [
    step(
      "Draft",
      "Release order",
      "Released",
      "Review the approved specification.",
    ),
    step(
      "Released",
      "Reserve materials",
      "Materials reserved",
      "All required components must be available.",
      "materials",
    ),
    step(
      "Materials reserved",
      "Report output",
      "Quality review",
      "Record labor and good output.",
    ),
    step(
      "Quality review",
      "Complete order",
      "Completed",
      "Review the quality disposition and reported output.",
      "manufacturing",
    ),
  ],
  "Material readiness through finished goods",
);
defineModule(
  "planning",
  "planning",
  "Scenarios|Shortages|Recommendations|Comparison",
  "Planning scenario",
  "Autumn demand · base scenario",
  "Create scenario",
  [
    specField("demand", "Expected demand", "number", 180),
    specField("available", "Available units", "number", 80),
    specField(
      "notes",
      "Planning assumptions",
      "textarea",
      "Use confirmed store demand; keep a small replenishment buffer.",
    ),
  ],
  [
    step(
      "Draft",
      "Run scenario",
      "Recommendations ready",
      "Calculate the sample shortfall from current inputs.",
    ),
    step(
      "Recommendations ready",
      "Review recommendation",
      "Approved",
      "Compare base and buffered quantities.",
    ),
    step(
      "Approved",
      "Release recommendation",
      "Released",
      "Open a linked requisition. Stock is unchanged until received.",
      "",
      "procurement",
    ),
  ],
  "Explore the options before committing",
);
defineModule(
  "projects",
  "project",
  "Work breakdown|Board|Dependencies|Progress",
  "Project work item",
  "Autumn store refresh",
  "Add work item",
  [
    specField("owner", "Owner", "text", "Mali Srisai"),
    specField("progress", "Progress (%)", "number", 25),
    specField(
      "notes",
      "Scope and next milestone",
      "textarea",
      "Prepare the store for the autumn assortment launch.",
    ),
  ],
  [
    step(
      "Draft",
      "Link dependency",
      "Dependency review",
      "Link the requisition that this work depends on.",
      "reference",
    ),
    step(
      "Dependency review",
      "Approve dependency",
      "In progress",
      "Review the connected team and handoff.",
    ),
    step(
      "In progress",
      "Complete milestone",
      "Completed",
      "Record completion and preserve the dependency history.",
      "progress",
    ),
  ],
  "Every milestone has people and dependencies",
);
defineModule(
  "ecommerce",
  "catalog",
  "Channels|Listings|Orders|Returns",
  "Product listing",
  "Canvas tote · autumn listing",
  "Create listing",
  [
    specField("price", "Display price (USD)", "number", 24),
    specField("stock", "Sample stock available", "number", 100),
    specField(
      "notes",
      "Product description",
      "textarea",
      "A durable everyday canvas tote with a comfortable shoulder strap.",
    ),
  ],
  [
    step(
      "Draft",
      "Preview listing",
      "In review",
      "Inspect product wording, price and availability.",
    ),
    step(
      "In review",
      "Publish listing",
      "Published",
      "Approve the sample storefront listing.",
    ),
    step(
      "Published",
      "Inspect sample checkout",
      "Checkout reviewed",
      "Review quantity, reservation and price. No payment is charged.",
      "stock",
    ),
    step(
      "Checkout reviewed",
      "Create sample order",
      "Ordered",
      "Open the sales order created by this sample checkout.",
      "",
      "sales-order",
    ),
  ],
  "From product discovery to a reliable handoff",
);
defineModule(
  "benefits",
  "people",
  "Available plans|Enrollment|Dependents|Coverage",
  "Benefit enrollment",
  "Wellbeing plan · enrollment",
  "Choose coverage",
  [
    specField("coverage", "Coverage", "select", "Employee + dependent", [
      "Employee only",
      "Employee + dependent",
    ]),
    specField("dependent", "Dependent name", "text", "Ari Srisai"),
    specField("validated", "Dependent details verified", "checkbox", false),
    specField("window", "Enrollment window open", "checkbox", true),
    specField(
      "notes",
      "Enrollment note",
      "textarea",
      "Review the selected coverage and eligibility.",
    ),
  ],
  [
    step(
      "Open",
      "Validate coverage",
      "Ready to enroll",
      "The window must be open and dependent details valid.",
      "enrollment",
    ),
    step(
      "Ready to enroll",
      "Confirm enrollment",
      "Enrolled",
      "Review selected coverage and preserve the confirmation.",
      "enrollment",
    ),
  ],
  "Benefits that are easy to understand",
);
defineModule(
  "learning",
  "learning",
  "Assigned courses|Course catalog|Progress|Results",
  "Course enrollment",
  "Receiving essentials · assignment",
  "Open assigned course",
  [
    specField("attended", "Sample lesson attended", "checkbox", false),
    specField("score", "Sample result (%)", "number", 85),
    specField(
      "notes",
      "What I will use at work",
      "textarea",
      "Count the cartons, inspect damage and link the receiving note.",
    ),
  ],
  [
    step(
      "Assigned",
      "Record attendance",
      "In progress",
      "Watch the local captioned lesson, then record sample attendance.",
      "attended",
    ),
    step(
      "In progress",
      "Complete course",
      "Completed",
      "A passing sample result is required.",
      "score",
    ),
  ],
  "Small lessons that help every day",
);
defineModule(
  "talent",
  "pipeline",
  "Requisitions|Applications|Interviews|Offers",
  "Hiring application",
  "Store associate · application review",
  "Review application",
  [
    specField("candidate", "Candidate (restricted)", "text", "Jamie Reed"),
    specField("position", "Position", "text", "Store associate"),
    specField(
      "notes",
      "Interview evidence",
      "textarea",
      "Demonstrated customer care, clear communication and shift availability.",
    ),
  ],
  [
    step(
      "Application",
      "Review application",
      "Interview",
      "Review the application in this restricted workspace.",
    ),
    step(
      "Interview",
      "Record interview",
      "Offer review",
      "Capture evidence supporting the hiring decision.",
      "evidence",
    ),
    step(
      "Offer review",
      "Approve offer",
      "Hired",
      "Review the offer and open the People handoff.",
      "",
      "people_payroll",
    ),
  ],
  "A thoughtful, traceable hiring experience",
);
defineModule(
  "field_service",
  "schedule",
  "Dispatch schedule|Service orders|Parts|Visit evidence",
  "Service order",
  "Riverstone · cold-room visit",
  "Create service order",
  [
    specField("scheduled", "Visit date", "date", "2026-09-06"),
    specField("technician", "Technician", "text", "Dao Arun"),
    specField("parts", "Reserved sensor kits", "number", 1),
    specField(
      "notes",
      "Visit summary",
      "textarea",
      "Replace the temperature sensor and verify safe readings.",
    ),
  ],
  [
    step(
      "Open",
      "Schedule visit",
      "Scheduled",
      "Confirm the visit, technician and reserved parts.",
    ),
    step(
      "Scheduled",
      "Dispatch order",
      "Dispatched",
      "Review the site and sample dispatch details.",
    ),
    step(
      "Dispatched",
      "Complete visit",
      "Completed",
      "Completion requires visit evidence.",
      "evidence",
    ),
    step(
      "Completed",
      "Release billing",
      "Billing released",
      "Review completed work before opening the finance handoff.",
      "evidence",
      "finance",
    ),
  ],
  "The right person, prepared for the visit",
);
defineModule(
  "agent",
  "service",
  "Agent catalog|Versions|Assignments|Proposals",
  "Agent assignment",
  "Handover assistant · sample assignment",
  "Start sample assignment",
  [
    specField("agentName", "Agent", "text", "Handover assistant"),
    specField("versionLabel", "Version", "text", "v1.2"),
    specField(
      "notes",
      "Assignment brief",
      "textarea",
      "Draft a handover summary from the linked work records.",
    ),
  ],
  [
    step(
      "Version review",
      "Approve version",
      "Approved",
      "Review the sample capability and restricted sources.",
    ),
    step(
      "Approved",
      "Start assignment",
      "Output ready",
      "Prepare deterministic sample output; no model is called.",
    ),
    step(
      "Output ready",
      "Accept as draft",
      "Accepted draft",
      "Keep the output attributed and unpublished until a human uses it.",
    ),
  ],
  "Helpful drafts, accountable decisions",
);
defineModule(
  "collaboration",
  "community",
  "Threads|Mentions|Following|Topics",
  "Discussion thread",
  "Autumn launch · working thread",
  "Create discussion",
  [
    specField(
      "notes",
      "Discussion topic",
      "textarea",
      "What should the receiving and sales teams know before launch?",
    ),
  ],
  [
    step(
      "Draft",
      "Publish discussion",
      "Open",
      "Link work and mention a colleague before publishing.",
    ),
    step(
      "Open",
      "Mark answered",
      "Answered",
      "Keep the conversation and its references available.",
    ),
  ],
  "Conversations close to the work",
);
const JOURNEYS = {
  revenue: {
    name: "From opportunity to revenue",
    description: "Customer context follows the quote, order and journal.",
    modules: ["crm", "sales", "finance"],
    edges: ["quoted from", "billed by"],
  },
  supply: {
    name: "From project to available stock",
    description: "Follow the need, approval, receipt and delivery handoff.",
    modules: ["projects", "planning", "procurement", "inventory"],
    edges: ["planned by", "depends on", "fulfilled by"],
  },
  hiring: {
    name: "From application to first day",
    description: "A private hiring decision becomes an employee handoff.",
    modules: ["talent", "people_payroll", "learning"],
    edges: ["assigned through", "prepared by"],
  },
  service: {
    name: "From reported issue to recovery",
    description:
      "The quality gate stays visible through repair and the site visit.",
    modules: [
      "support",
      "quality",
      "maintenance",
      "assets",
      "field_service",
      "operations",
    ],
    edges: [
      "blocked by",
      "resolved through",
      "maintains",
      "serviced by",
      "handed over to",
    ],
  },
};

window.ERPX_MODULES = { MODULES, JOURNEYS };
