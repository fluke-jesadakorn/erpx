/* Preserved module fixtures from the original ERPX mockup. No live business data. */
const GROUPS = [
  /* Real business departments, ordered by daily-use priority (money-in first, systems last).
   Contexts inside each dept follow the real workflow order (e.g. Sales: POS checkout → quote → online → pipeline). */
  ["Sales & Revenue", ["pos", "sales", "ecommerce", "crm"], "shopping-cart"],
  ["Marketing & Brand", ["marketing", "brand"], "megaphone"],
  [
    "Customer & Service",
    ["customer_360", "support", "field_service", "customer_authority"],
    "life-buoy",
  ],
  [
    "Operations & Supply",
    [
      "planning",
      "procurement",
      "inventory",
      "operations",
      "manufacturing",
      "projects",
    ],
    "boxes",
  ],
  ["Quality & Assets", ["quality", "maintenance", "assets"], "badge-check"],
  ["Finance & Legal", ["commercial", "finance", "legal"], "wallet"],
  [
    "People & Talent",
    ["talent", "people_payroll", "learning", "benefits"],
    "users",
  ],
  [
    "Platform & IT",
    [
      "organization",
      "identity_access",
      "platform_administration",
      "agent",
      "collaboration",
    ],
    "server",
  ],
];

const LICON = {
  brand: "palette",
  commercial: "credit-card",
  identity_access: "shield-check",
  organization: "building-2",
  platform_administration: "server",
  customer_360: "users",
  customer_authority: "key-round",
  support: "life-buoy",
  finance: "wallet",
  sales: "file-text",
  pos: "shopping-cart",
  assets: "package",
  inventory: "boxes",
  maintenance: "wrench",
  operations: "activity",
  procurement: "clipboard-list",
  quality: "badge-check",
  legal: "scale",
  people_payroll: "user",
  crm: "heart",
  marketing: "megaphone",
  manufacturing: "factory",
  planning: "calendar-days",
  projects: "kanban",
  ecommerce: "store",
  benefits: "gift",
  learning: "graduation-cap",
  talent: "user-plus",
  field_service: "hard-hat",
  agent: "bot",
  collaboration: "messages-square",
};

const CTX = {
  brand: {
    n: "Brand",
    lane: "publish approvals · brand-owner",
    tags: ["#brand", "#identity"],
    m: [
      ["Open drafts", "4"],
      ["Published", "27"],
      ["Pending review", "2"],
      ["Locales", "th/en"],
    ],
    rows: [
      ["DRAFT-118", "Signup hero refresh", "identity · v3", "Draft", "b-def"],
      [
        "DRAFT-117",
        "Invoice logo lockup",
        "billing · v1",
        "In review",
        "b-warn",
      ],
      ["BRD-102", "Core mark v2", "identity · published", "Posted", "b-ok"],
    ],
  },
  commercial: {
    n: "Commercial",
    lane: "grace/overdue · billing-owner",
    tags: ["#billing", "#plan"],
    m: [
      ["Active plans", "6"],
      ["Subscriptions", "1,204"],
      ["Grace", "3"],
      ["MRR", "$48.2K"],
    ],
    rows: [
      ["PLAN-pro-26", "Pro annual", "$29/seat · Stripe", "Posted", "b-ok"],
      ["SUB-8841", "Acme Holding", "grace +2d", "Pending", "b-warn"],
      ["PLAN-team-26", "Team monthly", "$12/seat · draft", "Draft", "b-def"],
    ],
  },
  identity_access: {
    n: "Identity & Access",
    lane: "expiry <7d · security",
    tags: ["#access", "#roles"],
    m: [
      ["Principals", "312"],
      ["Expiring <7d", "5"],
      ["Break-glass", "0"],
      ["Reviews due", "2"],
    ],
    rows: [
      [
        "GRANT-441",
        "Mali · finance-approver",
        "expires Apr 6",
        "Active",
        "b-ok",
      ],
      [
        "GRANT-439",
        "Somchai · warehouse",
        "expires Mar 30",
        "Expiring",
        "b-warn",
      ],
      ["AUDIT-Q1", "Access review", "8 scopes", "Pending", "b-warn"],
    ],
  },
  organization: {
    n: "Organization",
    lane: "suspend/archive · platform",
    tags: ["#org", "#holding"],
    m: [
      ["Holdings", "3"],
      ["Companies", "11"],
      ["Branches", "9"],
      ["Teams", "42"],
    ],
    rows: [
      ["ACME", "Acme Holding", "3 companies", "Active", "b-ok"],
      ["ACME-BKK", "Bangkok branch", "5 teams", "Active", "b-ok"],
      ["ACME-CNX", "Chiang Mai branch", "suspended", "Suspended", "b-def"],
    ],
  },
  platform_administration: {
    n: "Platform Admin",
    lane: "failed jobs · platform",
    tags: ["#platform"],
    m: [
      ["Holdings live", "3"],
      ["Backups OK", "3/3"],
      ["Workers", "12"],
      ["PITR drill", "pass"],
    ],
    rows: [
      ["PROV-77", "Acme Holding", "completed", "Posted", "b-ok"],
      ["PITR-mar", "Point-in-time drill", "passed", "Posted", "b-ok"],
      ["BKUP-310", "Nightly backup", "verified", "Posted", "b-ok"],
    ],
  },
  customer_360: {
    n: "Customer 360",
    lane: "none (read-only)",
    tags: ["#customer"],
    m: [
      ["Profiles", "8,412"],
      ["Touched 7d", "1,204"],
      ["Open tickets", "56"],
      ["At risk", "7"],
    ],
    rows: [
      ["CUST-acme", "Acme Holding", "3 open · $18.4K pipe", "Active", "b-ok"],
      ["CUST-siam", "Siam Retail", "1 open · $9.2K pipe", "Active", "b-ok"],
      ["CUST-bkkf", "Bangkok Foods", "0 open", "Quiet", "b-def"],
    ],
  },
  customer_authority: {
    n: "Customer Authority",
    lane: "merges · privacy",
    tags: ["#access"],
    m: [
      ["Bindings", "212"],
      ["Pending merge", "1"],
      ["Audits 30d", "14"],
      ["Restricted", "6"],
    ],
    rows: [
      ["BIND-901", "Acme portal access", "company scope", "Active", "b-ok"],
      ["MERGE-12", "Siam duplicate", "awaiting privacy", "Pending", "b-warn"],
      ["AUD-331", "Q1 access audit", "complete", "Posted", "b-ok"],
    ],
  },
  support: {
    n: "Support",
    lane: "SLA <2h · support",
    tags: ["#ticket", "#sla", "#incident"],
    m: [
      ["Open", "56"],
      ["SLA risk", "7"],
      ["Incidents", "1"],
      ["CSAT", "4.6"],
    ],
    rows: [
      ["TICKET-509", "Stock mismatch at BKK", "SLA 1h12m", "Open", "b-warn"],
      ["TICKET-508", "VAT on receipt", "SLA 6h", "Open", "b-def"],
      ["INC-014", "Bank import delay", "broadcast", "Posted", "b-ok"],
    ],
  },
  finance: {
    n: "Finance",
    lane: "period-gate · finance-approver",
    tags: ["#month-close", "#journal"],
    m: [
      ["Trial balance", "Balanced"],
      ["Open period", "Mar 2026"],
      ["Unposted", "12"],
      ["FX reval", "Pending"],
    ],
    rows: [
      [
        "J-2026-117",
        "Sales posting Mar 28",
        "$18,400 = $18,400",
        "Posted",
        "b-ok",
      ],
      ["J-2026-118", "Accrual Mar 29", "$9,200 draft", "Draft", "b-def"],
      ["J-2026-119", "FX revaluation", "$4,150 pending", "Pending", "b-warn"],
    ],
  },
  sales: {
    n: "Sales",
    lane: "expiring <3d · sales-approver",
    tags: ["#quote", "#win"],
    m: [
      ["Open quotes", "128"],
      ["Sent", "84"],
      ["Accepted", "31"],
      ["Expiring", "4"],
    ],
    rows: [
      ["QT-2026-042", "Acme Holding", "$18,400 · Net 30", "Sent", "b-warn"],
      ["QT-2026-041", "Siam Retail", "$9,200 · Net 15", "Accepted", "b-ok"],
      ["QT-2026-040", "Bangkok Foods", "$4,150 · draft", "Draft", "b-def"],
    ],
  },
  pos: {
    n: "Point of Sale",
    lane: "refunds · store-manager",
    tags: ["#sale", "#refund"],
    m: [
      ["Today", "$12,480"],
      ["Sales", "214"],
      ["Refunds", "2"],
      ["Shift", "open"],
    ],
    rows: [
      ["SALE-9921", "Register 2", "$184.00 · card", "Posted", "b-ok"],
      ["SALE-9920", "Register 1", "$62.50 · cash", "Posted", "b-ok"],
      ["REF-118", "SALE-9904", "$24.00 bounded", "Pending", "b-warn"],
    ],
  },
  assets: {
    n: "Assets",
    lane: "degraded · ops",
    tags: ["#asset"],
    m: [
      ["In service", "342"],
      ["Degraded", "3"],
      ["In maintenance", "5"],
      ["Retired YTD", "11"],
    ],
    rows: [
      ["ASSET-042", "Forklift FL-02", "degraded", "Degraded", "b-warn"],
      ["ASSET-107", "HVAC rooftop", "in service", "Active", "b-ok"],
      ["ASSET-098", "Van V-11", "maintenance", "Pending", "b-warn"],
    ],
  },
  inventory: {
    n: "Inventory",
    lane: "shortages · warehouse",
    tags: ["#stock"],
    m: [
      ["SKUs", "4,120"],
      ["Shortages", "6"],
      ["Reserved", "312"],
      ["Moves today", "48"],
    ],
    rows: [
      ["MOVE-883", "WIDGET-A ×200", "WH1→WH2", "Posted", "b-ok"],
      ["MOVE-884", "WIDGET-B ×50", "reserved", "Pending", "b-warn"],
      ["MOVE-885", "GADGET-C ×12", "shortage", "Blocked", "b-def"],
    ],
  },
  maintenance: {
    n: "Maintenance",
    lane: "blocked · maintenance-lead",
    tags: ["#workorder"],
    m: [
      ["Open WO", "18"],
      ["Blocked", "2"],
      ["Due today", "5"],
      ["Done 7d", "22"],
    ],
    rows: [
      ["WO-771", "FL-02 hydraulics", "blocked on parts", "Blocked", "b-def"],
      ["WO-772", "HVAC filter", "scheduled", "Pending", "b-warn"],
      ["WO-770", "Dock light", "complete + evidence", "Posted", "b-ok"],
    ],
  },
  operations: {
    n: "Operations",
    lane: "escalations · ops-lead",
    tags: ["#ops"],
    m: [
      ["Exceptions", "9"],
      ["Escalated", "2"],
      ["Work queue", "34"],
      ["Resolved 7d", "41"],
    ],
    rows: [
      ["EXC-301", "Pick shortfall WH2", "escalated", "Pending", "b-warn"],
      ["EXC-299", "Carrier delay", "acknowledged", "Open", "b-def"],
      ["EXC-295", "Cycle count gap", "resolved", "Posted", "b-ok"],
    ],
  },
  procurement: {
    n: "Procurement",
    lane: "approvals · procurement-approver",
    tags: ["#po", "#receipt"],
    m: [
      ["Open REQ", "23"],
      ["Pending approval", "6"],
      ["POs", "41"],
      ["Receipts", "35"],
    ],
    rows: [
      [
        "REQ-2026-014",
        "MRO spares · $6.8K",
        "pending approval",
        "Pending",
        "b-warn",
      ],
      ["PO-7731", "REQ-2026-011 · issued", "awaiting receipt", "Sent", "b-def"],
      ["GR-552", "PO-7719 · 3-way match", "accepted", "Posted", "b-ok"],
    ],
  },
  quality: {
    n: "Quality",
    lane: "failed checks · quality-lead",
    tags: ["#quality"],
    m: [
      ["Open checks", "12"],
      ["Failed", "1"],
      ["Released", "30"],
      ["Holds", "2"],
    ],
    rows: [
      ["LOT-331", "Resin batch · fail", "blocks ops", "Blocked", "b-def"],
      ["LOT-332", "Resin batch · pass", "released", "Posted", "b-ok"],
      ["LOT-330", "Steel coil", "in check", "Pending", "b-warn"],
    ],
  },
  legal: {
    n: "Legal",
    lane: "approvals · legal-approver",
    tags: ["#contract"],
    m: [
      ["Matters", "17"],
      ["Contracts", "64"],
      ["Holds", "3"],
      ["Pending sign", "4"],
    ],
    rows: [
      ["CTR-2201", "MSA Acme · v4", "awaiting sign", "Pending", "b-warn"],
      ["CTR-2198", "DPA Siam · signed", "active", "Posted", "b-ok"],
      ["HOLD-09", "Litigation hold", "3 docs frozen", "Active", "b-def"],
    ],
  },
  people_payroll: {
    n: "People & Payroll",
    lane: "decisions · hr-approver",
    tags: ["#leave", "#payroll"],
    m: [
      ["Headcount", "186"],
      ["Leave open", "9"],
      ["Expense open", "14"],
      ["Payroll run", "Mar · draft"],
    ],
    rows: [
      ["LEAVE-881", "Mali · Apr 3–5", "pending", "Pending", "b-warn"],
      ["EXP-1204", "Somchai · $182", "pending", "Pending", "b-warn"],
      ["RUN-mar", "Payroll Mar 2026", "draft", "Draft", "b-def"],
    ],
  },
  crm: {
    n: "CRM",
    lane: "stalled >14d · sales-lead",
    tags: ["#opportunity"],
    m: [
      ["Open opps", "64"],
      ["Pipeline", "$1.2M"],
      ["Stalled", "5"],
      ["New 7d", "9"],
    ],
    rows: [
      [
        "OPP-2026-011",
        "Acme expansion",
        "$180K · negotiation",
        "Open",
        "b-warn",
      ],
      ["OPP-2026-014", "Siam rollout", "$96K · qualified", "Open", "b-def"],
      ["OPP-2026-009", "BKK Foods", "$44K · won", "Posted", "b-ok"],
    ],
  },
  marketing: {
    n: "Marketing",
    lane: "verify · marketing-ops",
    tags: ["#campaign"],
    m: [
      ["Campaigns", "8"],
      ["Leads 30d", "312"],
      ["Journeys", "5"],
      ["Domains", "3 verified"],
    ],
    rows: [
      ["CMP-spring", "Spring push · email", "scheduled", "Pending", "b-warn"],
      ["CMP-q1rev", "Q1 review", "sent 12.4K", "Posted", "b-ok"],
      ["LEAD-990", "Trade-show batch", "312 captured", "Open", "b-def"],
    ],
  },
  manufacturing: {
    n: "Manufacturing",
    lane: "disposition · shop-lead",
    tags: ["#workorder"],
    m: [
      ["Orders", "26"],
      ["In stage", "8"],
      ["Blocked", "1"],
      ["Yield", "98.2%"],
    ],
    rows: [
      ["MFG-551", "Bracket run ×5K", "in stage 3/5", "Open", "b-def"],
      ["MFG-549", "Housing run", "blocked QA", "Blocked", "b-def"],
      ["MFG-548", "Gasket run", "complete", "Posted", "b-ok"],
    ],
  },
  planning: {
    n: "Planning",
    lane: "releases · planner",
    tags: ["#plan"],
    m: [
      ["Scenarios", "4"],
      ["Shortages", "6"],
      ["Released", "2"],
      ["Horizon", "12w"],
    ],
    rows: [
      ["SCN-apr", "April plan · 6 shortages", "frozen", "Posted", "b-ok"],
      ["SCN-may", "May draft", "2 shortages", "Draft", "b-def"],
      ["REC-77", "expedite resin", "released", "Pending", "b-warn"],
    ],
  },
  projects: {
    n: "Projects",
    lane: "blocked · pm",
    tags: ["#project"],
    m: [
      ["Active", "14"],
      ["Milestones 7d", "6"],
      ["Blocked", "2"],
      ["On track", "86%"],
    ],
    rows: [
      [
        "PROJ-2026-001",
        "ERP rollout · WBS-01",
        "dependsOn REQ-014",
        "Open",
        "b-warn",
      ],
      ["PROJ-2026-004", "WH slotting v2", "on track", "Open", "b-def"],
      ["PROJ-2026-002", "POS pilot", "complete", "Posted", "b-ok"],
    ],
  },
  ecommerce: {
    n: "E-commerce",
    lane: "returns · store-ops",
    tags: ["#listing"],
    m: [
      ["Listings", "128"],
      ["Carts active", "46"],
      ["Orders 7d", "210"],
      ["Returns", "4"],
    ],
    rows: [
      ["LST-771", "Widget Pro", "published", "Posted", "b-ok"],
      ["ORD-5521", "Cart checkout", "to sales", "Sent", "b-def"],
      ["RET-44", "Defective unit", "pending", "Pending", "b-warn"],
    ],
  },
  benefits: {
    n: "Benefits",
    lane: "closing <7d",
    tags: ["#benefits"],
    m: [
      ["Plans", "5"],
      ["Enrolled", "172"],
      ["Window", "open to Apr 7"],
      ["Pending", "9"],
    ],
    rows: [
      ["PLAN-hplus", "Health+ 2026", "enrollment open", "Open", "b-warn"],
      ["ENR-881", "Mali · family", "elected", "Posted", "b-ok"],
      ["ENR-879", "Somchai · single", "pending docs", "Pending", "b-warn"],
    ],
  },
  learning: {
    n: "Learning",
    lane: "overdue · people-lead",
    tags: ["#learning"],
    m: [
      ["Courses", "24"],
      ["Assigned", "86"],
      ["Completed", "61"],
      ["Overdue", "5"],
    ],
    rows: [
      ["CRS-safety", "Safety 101", "assigned", "Pending", "b-warn"],
      ["CRS-erp", "ERP basics", "completed", "Posted", "b-ok"],
      ["CRS-tax", "WHT advanced", "overdue", "Blocked", "b-def"],
    ],
  },
  talent: {
    n: "Talent",
    lane: "offers · hiring-manager",
    tags: ["#hiring"],
    m: [
      ["Requisitions", "7"],
      ["Pipeline", "43"],
      ["Offers", "2"],
      ["Erasure req", "0"],
    ],
    rows: [
      ["REQ-009", "Ops lead", "interviews", "Open", "b-warn"],
      [
        "OFFER-21",
        "Ops lead · candidate A",
        "pending sign",
        "Pending",
        "b-warn",
      ],
      ["REQ-007", "QA tech", "filled", "Posted", "b-ok"],
    ],
  },
  field_service: {
    n: "Field Service",
    lane: "exceptions · dispatcher",
    tags: ["#field"],
    m: [
      ["Orders", "19"],
      ["Dispatched", "8"],
      ["Exceptions", "2"],
      ["Billed", "11"],
    ],
    rows: [
      ["JOB-018", "Acme HVAC visit", "complete + evidence", "Posted", "b-ok"],
      ["JOB-019", "Siam POS install", "dispatched", "Sent", "b-def"],
      ["JOB-020", "BKK dock door", "part exception", "Blocked", "b-def"],
    ],
  },
  agent: {
    n: "Agents",
    lane: "approvals · owners",
    tags: ["#agent"],
    m: [
      ["Agents", "12"],
      ["Pools", "4"],
      ["Assignments", "23"],
      ["Proposed", "3"],
    ],
    rows: [
      [
        "ASN-301",
        "fill-payment-terms",
        "ready for review",
        "Pending",
        "b-warn",
      ],
      ["ASN-299", "triage-tickets", "running", "Open", "b-def"],
      ["AGV-pay-3", "payment-assistant v3", "approved", "Posted", "b-ok"],
    ],
  },
  collaboration: {
    n: "Collaboration",
    lane: "mentions inbox",
    tags: ["#thread"],
    m: [
      ["Threads", "148"],
      ["Mentions", "12"],
      ["Following", "34"],
      ["Tags followed", "6"],
    ],
    rows: [
      ["THR-771", "Month-close party", "24 replies", "Open", "b-warn"],
      ["THR-769", "Quote review QT-042", "resolved", "Posted", "b-ok"],
      ["THR-765", "Vendor onboarding", "7 replies", "Open", "b-def"],
    ],
  },
};

const FLOW = {
  sales: {
    stages: ["Draft", "Sent", "Accepted", "Order", "Fulfilled"],
    neg: ["Rejected", "Expired", "Cancelled"],
    done: ["Accepted", "Order", "Fulfilled"],
    alias: { "In review": "Sent" },
    acts: {
      Draft: [
        ["Send quote", "Sent", 0],
        ["Reject", "Rejected", 1],
      ],
      Sent: [
        ["Accept", "Accepted", 0],
        ["Expire", "Expired", 1],
        ["Reject", "Rejected", 1],
      ],
      Accepted: [["Create sales order", "Order", 1]],
      Order: [
        ["Mark fulfilled", "Fulfilled", 1],
        ["Cancel order", "Cancelled", 1],
      ],
    },
  },
  finance: {
    stages: ["Draft", "Pending", "Posted", "Closed"],
    neg: ["Rejected"],
    done: ["Posted", "Closed"],
    alias: {},
    acts: {
      Draft: [["Submit for posting", "Pending", 1]],
      Pending: [
        ["Post journal", "Posted", 1],
        ["Reject", "Rejected", 1],
      ],
      Posted: [["Close period", "Closed", 1]],
    },
  },
  pos: {
    stages: ["Open", "Pending", "Posted", "Refunded"],
    neg: [],
    done: ["Posted"],
    alias: {},
    acts: {
      Open: [["Tender", "Pending"]],
      Pending: [
        ["Confirm sale", "Posted"],
        ["Refund (bounded)", "Refunded", 1],
      ],
    },
  },
  procurement: {
    stages: ["Draft", "Pending", "Approved", "Ordered", "Received"],
    neg: ["Rejected"],
    done: ["Approved", "Ordered", "Received"],
    alias: { Sent: "Ordered", Posted: "Received" },
    acts: {
      Draft: [["Submit REQ", "Pending"]],
      Pending: [
        ["Approve REQ", "Approved", 1],
        ["Reject", "Rejected", 1],
      ],
      Approved: [["Issue PO", "Ordered", 1]],
      Ordered: [["Confirm receipt", "Received"]],
    },
  },
  inventory: {
    stages: ["Open", "Pending", "Posted"],
    neg: ["Blocked"],
    done: ["Posted"],
    alias: {},
    acts: {
      Open: [["Reserve", "Pending"]],
      Pending: [
        ["Confirm move", "Posted"],
        ["Flag shortage", "Blocked", 1],
      ],
      Blocked: [["Resolve", "Open"]],
    },
  },
  projects: {
    stages: ["Open", "In progress", "Done"],
    neg: ["Blocked"],
    done: ["Done"],
    alias: { Posted: "Done" },
    acts: {
      Open: [["Start", "In progress"]],
      "In progress": [
        ["Complete", "Done"],
        ["Block", "Blocked", 1],
      ],
      Blocked: [["Unblock", "In progress"]],
    },
  },
  support: {
    stages: ["Open", "Acknowledged", "Resolved"],
    neg: ["Escalated"],
    done: ["Resolved"],
    alias: { Posted: "Resolved" },
    acts: {
      Open: [
        ["Acknowledge", "Acknowledged"],
        ["Escalate", "Escalated", 1],
      ],
      Acknowledged: [["Resolve", "Resolved"]],
      Escalated: [["Resolve", "Resolved", 1]],
    },
  },
  crm: {
    stages: ["Open", "Qualified", "Negotiation", "Won"],
    neg: ["Lost"],
    done: ["Won"],
    alias: { Posted: "Won" },
    acts: {
      Open: [["Qualify", "Qualified"]],
      Qualified: [["Negotiate", "Negotiation"]],
      Negotiation: [
        ["Mark won", "Won", 1],
        ["Mark lost", "Lost", 1],
      ],
    },
  },
  talent: {
    stages: ["Open", "Interview", "Offer", "Hired"],
    neg: ["Rejected"],
    done: ["Hired"],
    alias: { Pending: "Interview", Posted: "Hired" },
    acts: {
      Open: [["Shortlist", "Interview"]],
      Interview: [["Extend offer", "Offer", 1]],
      Offer: [["Mark hired", "Hired", 1]],
    },
  },
  people_payroll: {
    stages: ["Draft", "Pending", "Approved", "Paid"],
    neg: ["Rejected"],
    done: ["Approved", "Paid"],
    alias: {},
    acts: {
      Draft: [["Submit", "Pending"]],
      Pending: [
        ["Approve", "Approved", 1],
        ["Reject", "Rejected", 1],
      ],
      Approved: [["Run payroll", "Paid", 1]],
    },
  },
  quality: {
    stages: ["Pending", "Passed", "Released"],
    neg: ["Failed", "Blocked"],
    done: ["Passed", "Released"],
    alias: { Posted: "Released" },
    acts: {
      Pending: [
        ["Pass check", "Passed"],
        ["Fail check", "Failed", 1],
      ],
      Passed: [["Release lot", "Released", 1]],
      Failed: [["Hold lot", "Blocked", 1]],
    },
  },
  maintenance: {
    stages: ["Open", "Pending", "Done"],
    neg: ["Blocked"],
    done: ["Done"],
    alias: { Posted: "Done" },
    acts: {
      Open: [["Schedule", "Pending"]],
      Pending: [["Complete + evidence", "Done", 1]],
      Blocked: [["Unblock", "Open"]],
    },
  },
  field_service: {
    stages: ["Dispatched", "In progress", "Done"],
    neg: ["Exception"],
    done: ["Done"],
    alias: { Sent: "Dispatched", Posted: "Done" },
    acts: {
      Dispatched: [["Start job", "In progress"]],
      "In progress": [
        ["Complete + evidence", "Done", 1],
        ["Raise exception", "Exception", 1],
      ],
      Exception: [["Resolve", "In progress"]],
    },
  },
  assets: {
    stages: ["Active", "Degraded", "Maintenance", "Retired"],
    neg: [],
    done: ["Active"],
    alias: { Pending: "Maintenance" },
    acts: {
      Active: [["Flag degraded", "Degraded"]],
      Degraded: [["Send to maintenance", "Maintenance", 1]],
      Maintenance: [
        ["Return to service", "Active"],
        ["Retire", "Retired", 1],
      ],
    },
  },
  ecommerce: {
    stages: ["Listed", "Ordered", "Shipped", "Delivered"],
    neg: ["Returned"],
    done: ["Delivered"],
    alias: { Posted: "Listed", Sent: "Shipped", Pending: "Ordered" },
    acts: {
      Ordered: [["Ship", "Shipped"]],
      Shipped: [["Deliver", "Delivered"]],
      Delivered: [["Accept return", "Returned", 1]],
    },
  },
  legal: {
    stages: ["Draft", "Pending", "Signed", "Active"],
    neg: ["Void"],
    done: ["Signed", "Active"],
    alias: { Posted: "Signed" },
    acts: {
      Draft: [["Send for sign", "Pending"]],
      Pending: [["Sign", "Signed", 1]],
      Signed: [["Activate", "Active"]],
      Active: [["Void", "Void", 1]],
    },
  },
  marketing: {
    stages: ["Draft", "Pending", "Sent"],
    neg: ["Cancelled"],
    done: ["Sent"],
    alias: { Open: "Draft", Posted: "Sent" },
    acts: {
      Draft: [["Schedule", "Pending"]],
      Pending: [
        ["Send campaign", "Sent", 1],
        ["Cancel", "Cancelled", 1],
      ],
    },
  },
  manufacturing: {
    stages: ["Open", "In stage", "Done"],
    neg: ["Blocked"],
    done: ["Done"],
    alias: { Posted: "Done" },
    acts: {
      Open: [["Release", "In stage", 1]],
      "In stage": [
        ["Complete", "Done"],
        ["Block QA", "Blocked", 1],
      ],
      Blocked: [["Clear QA", "In stage", 1]],
    },
  },
  planning: {
    stages: ["Draft", "Frozen", "Released"],
    neg: [],
    done: ["Frozen", "Released"],
    alias: { Posted: "Frozen", Pending: "Released" },
    acts: {
      Draft: [["Freeze scenario", "Frozen", 1]],
      Frozen: [["Release plan", "Released", 1]],
    },
  },
  operations: {
    stages: ["Open", "Acknowledged", "Resolved"],
    neg: ["Escalated"],
    done: ["Resolved"],
    alias: { Pending: "Acknowledged", Posted: "Resolved" },
    acts: {
      Open: [
        ["Acknowledge", "Acknowledged"],
        ["Escalate", "Escalated", 1],
      ],
      Acknowledged: [["Resolve", "Resolved"]],
    },
  },
  benefits: {
    stages: ["Open", "Elected", "Enrolled"],
    neg: [],
    done: ["Enrolled"],
    alias: { Posted: "Enrolled", Pending: "Elected" },
    acts: {
      Open: [["Elect", "Elected"]],
      Elected: [["Confirm enrollment", "Enrolled"]],
    },
  },
  learning: {
    stages: ["Assigned", "In progress", "Completed"],
    neg: ["Overdue"],
    done: ["Completed"],
    alias: { Pending: "Assigned", Posted: "Completed" },
    acts: {
      Assigned: [["Start", "In progress"]],
      "In progress": [["Complete", "Completed"]],
      Overdue: [["Reassign", "Assigned"]],
    },
  },
  agent: {
    stages: ["Proposed", "Running", "Approved"],
    neg: ["Rejected"],
    done: ["Approved"],
    alias: { Open: "Running", Pending: "Proposed", Posted: "Approved" },
    acts: {
      Proposed: [["Approve assignment", "Approved", 1]],
      Running: [["Complete run", "Approved"]],
    },
  },
  collaboration: {
    stages: ["Open", "Resolved"],
    neg: [],
    done: ["Resolved"],
    alias: { Posted: "Resolved" },
    acts: { Open: [["Resolve thread", "Resolved"]] },
  },
  commercial: {
    stages: ["Draft", "Pending", "Active"],
    neg: ["Cancelled", "Suspended"],
    done: ["Active"],
    alias: { Posted: "Active" },
    acts: {
      Draft: [["Activate plan", "Active", 1]],
      Pending: [["Approve", "Active", 1]],
      Active: [["Suspend", "Suspended", 1]],
      Suspended: [["Restore", "Active", 1]],
    },
  },
  brand: {
    stages: ["Draft", "In review", "Published"],
    neg: [],
    done: ["Published"],
    alias: { Posted: "Published" },
    acts: {
      Draft: [["Submit review", "In review"]],
      "In review": [["Publish", "Published", 1]],
    },
  },
  identity_access: {
    stages: ["Active", "Expiring", "Revoked"],
    neg: ["Revoked"],
    done: ["Active"],
    alias: { Pending: "Expiring" },
    acts: {
      Active: [["Revoke", "Revoked", 1]],
      Expiring: [
        ["Renew", "Active", 1],
        ["Revoke", "Revoked", 1],
      ],
    },
  },
  organization: {
    stages: ["Active", "Suspended", "Archived"],
    neg: ["Suspended", "Archived"],
    done: ["Active"],
    alias: {},
    acts: {
      Active: [["Suspend", "Suspended", 1]],
      Suspended: [
        ["Restore", "Active", 1],
        ["Archive", "Archived", 1],
      ],
    },
  },
  platform_administration: {
    stages: ["Pending", "Posted"],
    neg: [],
    done: ["Posted"],
    alias: {},
    acts: { Pending: [["Complete", "Posted"]] },
  },
  customer_authority: {
    stages: ["Active", "Pending", "Merged"],
    neg: [],
    done: ["Merged"],
    alias: { Posted: "Merged" },
    acts: {
      Pending: [["Approve merge", "Merged", 1]],
      Active: [["Restrict", "Pending", 1]],
    },
  },
};

const FIELDS = {
  sales: [
    ["notes", "text", "Notes", 2048, 1],
    ["payment_terms_days", "int", "Payment terms (days)", 365, 1],
    ["valid_until", "date", "Valid until", 0, 1],
    ["language", "choice", "Language", 0, 0, ["en", "th"]],
  ],
  procurement: [
    ["justification", "text", "Justification", 2048, 1],
    ["amount", "money", "Amount", 0, 1],
    [
      "vendor",
      "choice",
      "Vendor",
      0,
      0,
      ["ThaiMRO", "SiamParts", "FastSupply"],
    ],
  ],
  finance: [
    ["memo", "text", "Memo", 512, 1],
    ["amount", "money", "Amount", 0, 1],
    ["period", "choice", "Period", 0, 0, ["Mar 2026", "Apr 2026"]],
  ],
  support: [
    ["description", "text", "Description", 1024, 1],
    ["priority", "choice", "Priority", 0, 0, ["P1", "P2", "P3"]],
    ["channel", "choice", "Channel", 0, 0, ["portal", "phone", "field"]],
  ],
  projects: [
    ["summary", "text", "Summary", 512, 1],
    ["owner", "text", "Owner", 64, 1],
    ["due", "date", "Due", 0, 0],
  ],
  inventory: [
    ["qty", "int", "Qty", 0, 1],
    ["from", "text", "From", 32, 1],
    ["to", "text", "To", 32, 1],
  ],
  talent: [
    ["candidate", "text", "Candidate", 64, 1],
    ["stage", "choice", "Stage", 0, 0, ["Screen", "Interview", "Offer"]],
    ["salary", "money", "Offer salary", 0, 0],
  ],
  crm: [
    ["value", "money", "Value", 0, 1],
    ["stage", "choice", "Stage", 0, 0, ["Open", "Qualified", "Negotiation"]],
    ["close_date", "date", "Expected close", 0, 0],
  ],
  pos: [
    ["total", "money", "Total", 0, 1],
    ["method", "choice", "Method", 0, 0, ["cash", "card"]],
    ["register", "text", "Register", 16, 1],
  ],
  maintenance: [
    ["description", "text", "Description", 1024, 1],
    ["asset", "text", "Asset", 32, 1],
    ["evidence", "bool", "Evidence attached", 0, 0],
  ],
  quality: [
    ["result", "choice", "Result", 0, 0, ["pass", "fail"]],
    ["lot", "text", "Lot", 32, 1],
    ["inspector", "text", "Inspector", 64, 0],
  ],
  ecommerce: [
    ["price", "money", "Price", 0, 1],
    ["channel", "choice", "Channel", 0, 0, ["web", "app", "marketplace"]],
    ["stock", "int", "Stock", 0, 0],
  ],
  marketing: [
    ["subject", "text", "Subject", 256, 1],
    ["audience", "text", "Audience", 64, 1],
    ["send_at", "date", "Send at", 0, 0],
  ],
  manufacturing: [
    ["qty", "int", "Qty", 0, 1],
    ["line", "choice", "Line", 0, 0, ["L1", "L2"]],
    ["stage_note", "text", "Stage note", 256, 0],
  ],
  planning: [
    ["horizon", "choice", "Horizon", 0, 0, ["4w", "8w", "12w"]],
    ["note", "text", "Note", 512, 0],
  ],
  operations: [
    ["severity", "choice", "Severity", 0, 0, ["S1", "S2", "S3"]],
    ["note", "text", "Note", 512, 1],
  ],
  legal: [
    ["version", "text", "Version", 16, 1],
    ["counterparty", "text", "Counterparty", 64, 1],
    ["sign_date", "date", "Sign date", 0, 0],
  ],
  people_payroll: [
    ["kind", "choice", "Type", 0, 0, ["leave", "expense", "payroll"]],
    ["amount", "money", "Amount", 0, 0],
    ["dates", "text", "Dates", 64, 1],
  ],
  benefits: [
    ["plan", "choice", "Plan", 0, 0, ["Health+", "Dental", "Provident"]],
    ["member", "text", "Member", 64, 1],
    ["effective", "date", "Effective", 0, 0],
  ],
  learning: [
    ["progress", "int", "Progress %", 100, 0],
    ["course", "text", "Course", 64, 1],
    ["due", "date", "Due", 0, 0],
  ],
  field_service: [
    ["site", "text", "Site", 64, 1],
    ["tech", "text", "Technician", 64, 1],
    ["arrived", "bool", "On site", 0, 0],
  ],
  agent: [
    ["model", "text", "Model", 32, 1],
    ["version", "text", "Version", 16, 1],
    ["autopilot", "bool", "Autopilot", 0, 0],
  ],
  collaboration: [
    ["title", "text", "Title", 256, 1],
    ["participants", "text", "Participants", 128, 0],
  ],
  commercial: [
    ["plan", "choice", "Plan", 0, 0, ["Team", "Pro", "Enterprise"]],
    ["seats", "int", "Seats", 0, 1],
    ["billing", "choice", "Billing", 0, 0, ["Stripe", "Manual"]],
  ],
  brand: [
    ["title", "text", "Title", 256, 1],
    ["locale", "choice", "Locale", 0, 0, ["th", "en"]],
    ["version", "text", "Version", 16, 0],
  ],
  identity_access: [
    ["principal", "text", "Principal", 64, 1],
    ["role", "text", "Role", 64, 1],
    ["expires", "date", "Expires", 0, 1],
  ],
  organization: [
    ["name", "text", "Name", 64, 1],
    ["kind", "choice", "Kind", 0, 0, ["holding", "company", "branch", "team"]],
    ["parent", "text", "Parent", 64, 0],
  ],
  platform_administration: [
    ["job", "text", "Job", 64, 1],
    ["target", "text", "Target", 64, 1],
    ["window", "text", "Window", 32, 0],
  ],
  customer_authority: [
    ["subject", "text", "Subject", 128, 1],
    ["scope", "choice", "Scope", 0, 0, ["company", "branch"]],
    ["restricted", "bool", "Restricted", 0, 0],
  ],
};

const EDGES = {
  sales: [
    ["crm/OPP-2026-011", "quotedFrom"],
    ["projects/PROJ-2026-001", "dependsOn"],
    ["finance/J-2026-117", "billedBy"],
  ],
  finance: [
    ["sales/QT-2026-042", "billedBy"],
    ["people_payroll/RUN-mar", "authoredWith"],
  ],
  procurement: [
    ["projects/PROJ-2026-001", "dependsOn"],
    ["inventory/MOVE-883", "fulfills"],
  ],
  inventory: [
    ["procurement/PO-7731", "fulfills"],
    ["operations/EXC-301", "blocks"],
  ],
  projects: [
    ["procurement/REQ-2026-014", "dependsOn"],
    ["crm/OPP-2026-011", "quotedFrom"],
  ],
  support: [
    ["quality/LOT-331", "blocks"],
    ["inventory/MOVE-884", "dependsOn"],
  ],
  quality: [
    ["maintenance/WO-771", "blocks"],
    ["support/TICKET-509", "dependsOn"],
  ],
  maintenance: [
    ["assets/ASSET-042", "dependsOn"],
    ["field_service/JOB-020", "blocks"],
  ],
  assets: [["maintenance/WO-771", "blocks"]],
  field_service: [["assets/ASSET-042", "dependsOn"]],
  crm: [
    ["sales/QT-2026-042", "quotedFrom"],
    ["customer_360/CUST-acme", "dependsOn"],
  ],
  talent: [["projects/PROJ-2026-001", "authoredWith"]],
  people_payroll: [
    ["talent/OFFER-21", "dependsOn"],
    ["projects/PROJ-2026-001", "authoredWith"],
  ],
  operations: [["inventory/MOVE-885", "blocks"]],
  ecommerce: [
    ["sales/QT-2026-041", "quotedFrom"],
    ["inventory/MOVE-883", "dependsOn"],
  ],
  pos: [["finance/J-2026-117", "billedBy"]],
  marketing: [["crm/OPP-2026-014", "dependsOn"]],
  manufacturing: [["quality/LOT-332", "dependsOn"]],
  planning: [["inventory/MOVE-885", "dependsOn"]],
  legal: [["crm/OPP-2026-011", "dependsOn"]],
  benefits: [["people_payroll/RUN-mar", "dependsOn"]],
  learning: [["people_payroll/LEAVE-881", "dependsOn"]],
  agent: [["support/TICKET-509", "dependsOn"]],
  collaboration: [
    ["sales/QT-2026-042", "dependsOn"],
    ["finance/J-2026-119", "dependsOn"],
  ],
  commercial: [["finance/J-2026-117", "billedBy"]],
  brand: [["marketing/CMP-spring", "dependsOn"]],
  identity_access: [["people_payroll/LEAVE-881", "authoredWith"]],
  organization: [["commercial/SUB-8841", "dependsOn"]],
  platform_administration: [["organization/ACME", "dependsOn"]],
  customer_authority: [["customer_360/CUST-siam", "dependsOn"]],
  customer_360: [],
};
window.ERPX_SEED = { GROUPS, LICON, CTX, FLOW, FIELDS, EDGES };
