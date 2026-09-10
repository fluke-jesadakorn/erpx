import type { ModuleDefinition, ModuleSection } from "../module-registry";
import type { PreviewField, PreviewRecord, PreviewStep } from "./samples";

type FieldSpec = [
  label: string,
  value: string | number | boolean,
  type?: string,
];
const locations = ["Bangkok store", "Central warehouse", "Head office"];
const people = ["Mali Srisai", "Nisa Chen", "Dao Arun"];
const vendors = ["Siam Supplies", "Cedar Distribution", "ThaiMRO"];
const customers = ["Acme Market", "Siam Retail", "Bangkok Foods"];
const field = (
  label: string,
  value: string | number | boolean,
  type?: string,
): PreviewField => ({
  key: label.toLowerCase().replace(/[^a-z0-9]+/g, "_"),
  label,
  value,
  type:
    type ??
    (typeof value === "number"
      ? "number"
      : typeof value === "boolean"
        ? "checkbox"
        : "text"),
  options: [],
});

// Presentation-only sample schemas. They are never accepted as business command payloads.
function fieldsFor(
  definition: ModuleDefinition,
  section: ModuleSection,
  index: number,
): FieldSpec[] {
  const label = section.label.toLowerCase();
  const id = definition.id;
  const owner = people[index];
  const location = locations[index];
  const amount = [18400, 9200, 4150][index];
  const note: FieldSpec = ["Notes", section.description, "textarea"];
  if (id === "finance" && /report/.test(label))
    return [
      [
        "Statement",
        ["Trial balance", "Income statement", "Balance sheet"][index],
      ],
      ["Period", "September 2026"],
      ["Currency", "USD"],
      ["Debit (USD)", amount],
      ["Credit (USD)", amount],
      ["Prepared by", owner],
      note,
    ];
  if (id === "commercial" && /entitlement|catalog/.test(label))
    return [
      ["Plan", ["Enterprise", "Professional", "Team"][index]],
      ["Product", "Sample workspace"],
      ["Seat allowance", [100, 50, 15][index]],
      ["Billing currency", "USD"],
      ["Effective date", "2026-10-01", "date"],
      note,
    ];
  if (id === "identity_access" && /connector|conflict/.test(label))
    return [
      [
        "Directory",
        [
          "Sample staff directory",
          "Sample partner directory",
          "Sample contractor directory",
        ][index],
      ],
      ["Mapping owner", owner],
      ["Matching key", "Work email"],
      ["Pending identities", [3, 2, 0][index]],
      ["Last review", "2026-09-06", "date"],
      note,
    ];
  if (id === "identity_access" && /role|permission/.test(label))
    return [
      [
        "Role",
        ["Inventory controller", "Finance reviewer", "Service coordinator"][
          index
        ],
      ],
      ["Scope", location],
      ["Assigned people", [8, 3, 5][index]],
      ["Permission set", section.description],
      ["Review owner", owner],
      note,
    ];
  if (id === "identity_access" && /people|principal/.test(label))
    return [
      ["Employee name", owner],
      ["Work email", `person${index + 1}@example.test`],
      ["Department", ["Operations", "Finance", "Customer service"][index]],
      ["Location", location],
      ["Review owner", "Sample workspace administrator"],
      note,
    ];
  if (id === "benefits" && /plan|coverage/.test(label))
    return [
      [
        "Plan",
        ["Health coverage", "Wellbeing allowance", "Family coverage"][index],
      ],
      ["Provider", "Sample benefits provider"],
      ["Coverage limit (USD)", [25000, 1200, 40000][index]],
      ["Eligibility", "Active sample employees"],
      ["Effective date", "2026-10-01", "date"],
      note,
    ];
  if (id === "talent" && /requisition/.test(label))
    return [
      [
        "Role",
        ["Warehouse specialist", "Finance analyst", "Service coordinator"][
          index
        ],
      ],
      ["Hiring manager", owner],
      ["Open positions", index + 1],
      ["Location", location],
      ["Target start", "2026-10-01", "date"],
      note,
    ];
  if (id === "customer_authority")
    return [
      ["Customer", customers[index]],
      ["Source identity", `sample-customer-${index + 1}`],
      ["Authority", "Sample customer directory"],
      [
        "Review reason",
        /duplicate/.test(label)
          ? "Similar name and contact information"
          : section.label,
      ],
      ["Assigned reviewer", owner],
      ["Evidence reference", `IDENT-00${index + 1}`],
      note,
    ];
  if (id === "support" && /session|capabilit/.test(label))
    return [
      ["Customer", customers[index]],
      ["Support operator", owner],
      ["Allowed scope", "Inspect a fictional ticket"],
      ["Expires", "2026-09-07", "date"],
      ["Approval reference", `SUP-00${index + 1}`],
      note,
    ];
  if (id === "support" && /incident/.test(label))
    return [
      [
        "Incident",
        [
          "Delayed order updates",
          "Receipt delivery delay",
          "Catalog display issue",
        ][index],
      ],
      ["Severity", ["High", "Medium", "Low"][index]],
      ["Incident owner", owner],
      ["Affected sample customers", [3, 2, 1][index]],
      ["Next update", "2026-09-07", "date"],
      note,
    ];
  if (/attachment|document access/.test(label))
    return [
      ["Document title", `${section.label} · ${location}`],
      ["Filename", `sample-evidence-${index + 1}.pdf`],
      ["Owner", owner],
      ["Size (KB)", [240, 180, 320][index]],
      ["Access scope", "Authorized sample team"],
      ["Review date", "2026-09-06", "date"],
      note,
    ];
  if (id === "pos" && /register/.test(label))
    return [
      ["Register", `Sample register ${index + 1}`],
      ["Location", location],
      ["Cashier", owner],
      ["Opening cash (USD)", 200],
      ["Expected cash (USD)", [840, 630, 420][index]],
      ["Counted cash (USD)", [840, 625, 420][index]],
      note,
    ];
  if (["pos", "ecommerce"].includes(id) && /refund|return|void/.test(label))
    return [
      ["Original sale", `SALE-2026-00${index + 1}`],
      ["Customer", customers[index]],
      ["Item", ["Canvas tote", "Store display", "Packing set"][index]],
      ["Quantity", index + 1],
      ["Amount (USD)", [40, 85, 20][index]],
      [
        "Reason",
        ["Damaged on arrival", "Incorrect item", "Duplicate order"][index],
      ],
      note,
    ];
  if (id === "quality")
    return [
      ["Inspection", `QA-2026-00${index + 1}`],
      ["Lot", `LOT-SEP-0${index + 1}`],
      ["Inspector", owner],
      ["Samples checked", 20],
      ["Defects found", [2, 1, 0][index]],
      [
        "Disposition",
        ["Hold for review", "Recheck required", "Release proposed"][index],
      ],
      note,
    ];
  if (id === "planning")
    return [
      [
        "Scenario",
        ["Baseline demand", "Store expansion", "Peak season"][index],
      ],
      ["Site", location],
      ["Planning horizon", "September 2026"],
      ["Demand (units)", [120, 80, 150][index]],
      ["Available capacity (units)", [100, 90, 140][index]],
      ["Planner", owner],
      note,
    ];
  if (id === "manufacturing" && /cost|labor|output|genealogy/.test(label))
    return [
      ["Production order", `MO-2026-00${index + 1}`],
      ["Output lot", `LOT-SEP-0${index + 1}`],
      ["Completed units", [100, 60, 40][index]],
      ["Labor hours", [18, 12, 8][index]],
      ["Recorded cost (USD)", amount],
      ["Supervisor", owner],
      note,
    ];
  if (id === "marketing" && /domain|provider/.test(label))
    return [
      ["Domain", `campaign${index + 1}.example.test`],
      ["Provider", "Fictional delivery provider"],
      ["Responsible owner", owner],
      [
        "Review status",
        [
          "Configuration review",
          "Verification pending",
          "Sample setup complete",
        ][index],
      ],
      ["Purpose", "Sample campaign delivery"],
      note,
    ];
  if (id === "marketing" && /engagement/.test(label))
    return [
      [
        "Campaign",
        ["Autumn launch", "Store opening", "Customer welcome"][index],
      ],
      ["Period", "September 2026"],
      ["Delivered", [1000, 600, 400][index]],
      ["Opened", [420, 270, 160][index]],
      ["Clicked", [95, 64, 38][index]],
      note,
    ];
  if (id === "marketing" && /form|profile|lead/.test(label))
    return [
      ["Form", ["Product interest", "Store visit", "Newsletter signup"][index]],
      ["Audience", "Sample customers"],
      ["Owner", owner],
      ["Submissions", [24, 18, 12][index]],
      ["Consent wording", "Sample contact permission"],
      note,
    ];
  if (id === "ecommerce" && /listing|channel/.test(label))
    return [
      [
        "Channel",
        ["Sample online store", "Sample partner shop", "Sample marketplace"][
          index
        ],
      ],
      ["Product", ["Canvas tote", "Packing set", "Store display"][index]],
      ["SKU", `SKU-00${index + 1}`],
      ["Price (USD)", [40, 20, 85][index]],
      ["Available units", [100, 60, 40][index]],
      note,
    ];
  if (id === "projects" && /dependenc|graph|handoff/.test(label))
    return [
      [
        "Work package",
        ["Store fixtures", "Warehouse layout", "Customer training"][index],
      ],
      ["Depends on", ["Design approval", "Site survey", "Access setup"][index]],
      ["Receiving owner", owner],
      ["Target date", "2026-09-30", "date"],
      ["Dependency type", "Finish to start"],
      note,
    ];
  if (id === "projects" && /time|cost/.test(label))
    return [
      [
        "Project",
        ["Store refresh", "Warehouse upgrade", "Customer rollout"][index],
      ],
      ["Contributor", owner],
      ["Week beginning", "2026-09-07", "date"],
      ["Recorded hours", [24, 18, 12][index]],
      ["Cost (USD)", [1200, 900, 600][index]],
      note,
    ];
  if (id === "benefits" && /dependent/.test(label))
    return [
      ["Employee", owner],
      ["Dependent", `Sample dependent ${index + 1}`],
      ["Relationship", ["Spouse", "Child", "Child"][index]],
      ["Coverage plan", "Family coverage"],
      ["Eligibility review", "Pending document review"],
      note,
    ];
  if (id === "talent" && /offer|proposal|hire/.test(label))
    return [
      ["Candidate", `Sample candidate ${index + 1}`],
      [
        "Role",
        ["Operations specialist", "Finance analyst", "Service coordinator"][
          index
        ],
      ],
      ["Hiring manager", owner],
      ["Proposed start", "2026-10-01", "date"],
      ["Compensation (USD / month)", [2400, 2800, 2600][index]],
      ["Review status", "Independent review required"],
      note,
    ];
  if (id === "field_service" && /agreement|billing/.test(label))
    return [
      ["Customer", customers[index]],
      ["Agreement", `SVC-2026-00${index + 1}`],
      ["Covered site", location],
      ["Service period", "September 2026"],
      ["Amount (USD)", amount],
      ["Completed visits", index + 1],
      note,
    ];
  if (id === "agent" && /session|suggestion|invocation/.test(label))
    return [
      ["Session", `sample-session-${index + 1}`],
      ["Assistant", "Sample work assistant"],
      ["Requested by", owner],
      ["Source record", `WORK-00${index + 1}`],
      ["Review requirement", "Human review before any business action"],
      ["Evidence", "Fictional invocation context"],
      note,
    ];
  if (id === "finance") {
    if (/account/.test(label))
      return [
        ["Account code", ["1100", "2100", "4100"][index]],
        [
          "Account name",
          ["Cash and equivalents", "Accounts payable", "Sales revenue"][index],
        ],
        ["Account type", ["Asset", "Liability", "Revenue"][index]],
        ["Currency", "USD"],
        note,
      ];
    if (/payable/.test(label))
      return [
        ["Supplier", vendors[index]],
        ["Invoice reference", `INV-2026-10${index}`],
        ["Invoice date", "2026-09-01", "date"],
        ["Due date", "2026-09-30", "date"],
        ["Amount (USD)", amount],
        note,
      ];
    if (/receivable|aging/.test(label))
      return [
        ["Customer", customers[index]],
        ["Invoice amount (USD)", amount],
        ["Outstanding (USD)", amount],
        ["Due date", "2026-09-30", "date"],
        ["Aging band", "Current"],
        note,
      ];
    if (/bank/.test(label))
      return [
        ["Bank account", "Operating account · USD"],
        ["Statement date", "2026-09-05", "date"],
        ["Statement reference", `STMT-09-00${index + 1}`],
        ["Transaction amount (USD)", amount],
        ["Match status", ["Unmatched", "Suggested match", "Matched"][index]],
        note,
      ];
    if (/close|period/.test(label))
      return [
        ["Period", "September 2026"],
        ["Company", location],
        ["Prepared by", owner],
        ["Reconciliation complete", index === 2],
        ["Independent reviewer", "Assigned reviewer"],
        note,
      ];
    if (/payment/.test(label))
      return [
        ["Payee", vendors[index]],
        ["Amount (USD)", amount],
        ["Requested date", "2026-09-15", "date"],
        ["Payment reference", `PAY-2026-0${index + 1}`],
        note,
      ];
    if (/opening/.test(label))
      return [
        ["Account code", `${1100 + index * 100}`],
        ["Opening debit (USD)", amount],
        ["Opening credit (USD)", amount],
        ["Effective date", "2026-09-01", "date"],
        note,
      ];
  }
  if (id === "sales" || id === "crm") {
    if (/fulfill|handoff|order/.test(label))
      return [
        ["Customer", customers[index]],
        ["Order reference", `SO-2026-0${index + 1}`],
        ["Quantity ordered", [100, 60, 40][index]],
        ["Quantity fulfilled", [0, 30, 40][index]],
        ["Delivery date", "2026-09-18", "date"],
        note,
      ];
    return [
      ["Customer", customers[index]],
      ["Owner", owner],
      ["Value (USD)", amount],
      ["Next step", "Confirm customer requirements"],
      ["Expected date", "2026-09-18", "date"],
      note,
    ];
  }
  if (id === "people_payroll") {
    if (/leave|request/.test(label))
      return [
        ["Employee", owner],
        ["First day", "2026-09-14", "date"],
        ["Leave days", index + 1],
        [
          "Handover plan",
          `The ${location.toLowerCase()} team will cover pending work.`,
          "textarea",
        ],
      ];
    if (/expense/.test(label))
      return [
        ["Employee", owner],
        ["Expense date", "2026-09-05", "date"],
        ["Merchant", vendors[index]],
        ["Amount (USD)", [85, 140, 65][index]],
        ["Purpose", "Customer site visit"],
        note,
      ];
    if (/payroll/.test(label))
      return [
        ["Pay period", "September 2026"],
        ["Business unit", location],
        ["Employees", [24, 18, 12][index]],
        ["Currency", "USD"],
        ["Prepared by", owner],
        ["Independent review complete", index === 2],
        note,
      ];
    return [
      ["Employee name", owner],
      ["Work email", `person${index + 1}@example.test`],
      ["Department", ["Operations", "Finance", "Customer service"][index]],
      ["Location", location],
      ["Start date", "2026-09-01", "date"],
      note,
    ];
  }
  if (["procurement", "inventory", "manufacturing", "planning"].includes(id)) {
    if (/supplier/.test(label))
      return [
        ["Supplier", vendors[index]],
        ["Contact", "Supply coordination"],
        ["Lead time (days)", [7, 14, 5][index]],
        ["Terms", "Net 30"],
        note,
      ];
    if (/invoice|match/.test(label))
      return [
        ["Supplier", vendors[index]],
        ["Purchase order", `PO-2026-0${index + 1}`],
        ["Invoice amount (USD)", amount],
        ["Received quantity", [100, 60, 40][index]],
        ["Matched", index === 2],
        note,
      ];
    return [
      ["Item", ["Canvas tote", "Packaging carton", "Store display"][index]],
      ["SKU", ["TOTE-001", "PACK-020", "DISP-014"][index]],
      ["Location", location],
      ["Quantity", [100, 60, 40][index]],
      ["Required date", "2026-09-18", "date"],
      ["Owner", owner],
      note,
    ];
  }
  if (id === "identity_access")
    return [
      ["Person or role", owner],
      ["Assignment scope", location],
      [
        "Permission group",
        ["Operations", "Finance", "Customer service"][index],
      ],
      ["Valid until", "2026-12-31", "date"],
      ["Review owner", "Workspace administrator"],
      note,
    ];
  if (id === "organization")
    return [
      ["Name", location],
      ["Parent company", "Sample company"],
      ["Node type", ["Branch", "Warehouse", "Office"][index]],
      ["Manager", owner],
      note,
    ];
  if (["customer_360", "customer_authority"].includes(id))
    return [
      ["Customer", customers[index]],
      ["Customer reference", `CUST-00${index + 1}`],
      ["Account owner", owner],
      ["Contact email", `customer${index + 1}@example.test`],
      ["Source", "Sample customer directory"],
      note,
    ];
  if (id === "support")
    return [
      ["Ticket reference", `TKT-2026-0${index + 1}`],
      ["Customer", customers[index]],
      ["Assigned to", owner],
      ["Priority", ["High", "Normal", "Low"][index]],
      ["Response target", "2026-09-07", "date"],
      ["Description", section.description, "textarea"],
    ];
  if (
    [
      "assets",
      "maintenance",
      "quality",
      "operations",
      "field_service",
    ].includes(id)
  )
    return [
      ["Reference", `OPS-2026-0${index + 1}`],
      ["Site", location],
      ["Responsible person", owner],
      [
        "Asset or order",
        ["Receiving equipment", "Cooling unit", "Delivery vehicle"][index],
      ],
      ["Scheduled date", "2026-09-10", "date"],
      ["Evidence required", true],
      note,
    ];
  if (id === "commercial") {
    if (/invoice/.test(label))
      return [
        ["Customer", customers[index]],
        ["Invoice reference", `BILL-2026-00${index + 1}`],
        ["Billing period", "September 2026"],
        ["Amount (USD)", [720, 360, 240][index]],
        ["Due date", "2026-09-30", "date"],
        note,
      ];
    return [
      ["Customer", customers[index]],
      ["Plan", ["Enterprise", "Professional", "Team"][index]],
      ["Seats", [30, 15, 10][index]],
      ["Billing period", "Monthly"],
      ["Renewal date", "2026-10-01", "date"],
      note,
    ];
  }
  if (id === "legal")
    return [
      ["Document title", `${section.label} · ${customers[index]}`],
      ["Counterparty", customers[index]],
      ["Document owner", owner],
      ["Version", index + 1],
      ["Effective date", "2026-09-01", "date"],
      ["Retention hold", index === 1],
      note,
    ];
  if (id === "learning")
    return [
      [
        "Course",
        ["Receiving essentials", "Clear handovers", "Workspace basics"][index],
      ],
      ["Assigned to", owner],
      ["Due date", "2026-09-30", "date"],
      ["Progress (%)", [0, 50, 100][index]],
      ["Caption language", "English"],
      note,
    ];
  if (id === "benefits")
    return [
      ["Employee", owner],
      [
        "Plan",
        ["Health coverage", "Wellbeing allowance", "Family coverage"][index],
      ],
      ["Enrollment period", "September 2026"],
      ["Effective date", "2026-10-01", "date"],
      ["Eligibility reviewed", index === 2],
      note,
    ];
  if (id === "talent")
    return [
      [
        "Role",
        ["Warehouse specialist", "Finance analyst", "Service coordinator"][
          index
        ],
      ],
      ["Candidate", `Sample candidate ${index + 1}`],
      ["Hiring manager", owner],
      ["Next interview", "2026-09-15", "date"],
      ["Privacy review", "Restricted sample profile"],
      note,
    ];
  if (id === "projects")
    return [
      [
        "Project",
        ["Store refresh", "Warehouse upgrade", "Customer rollout"][index],
      ],
      ["Work package", `${index + 1}.1`],
      ["Owner", owner],
      ["Due date", "2026-09-30", "date"],
      ["Progress (%)", [10, 50, 100][index]],
      note,
    ];
  if (id === "marketing" || id === "brand")
    return [
      ["Name", ["Autumn launch", "Store identity", "Team welcome"][index]],
      ["Owner", owner],
      ["Audience", ["Customers", "Store teams", "All colleagues"][index]],
      ["Version", index + 1],
      ["Review date", "2026-09-12", "date"],
      note,
    ];
  if (id === "ecommerce" || id === "pos")
    return [
      ["Order reference", `ORD-2026-00${index + 1}`],
      [
        "Channel",
        ["Online store", "Bangkok register", "Partner channel"][index],
      ],
      ["Customer", customers[index]],
      ["Total (USD)", [240, 180, 95][index]],
      ["Currency", "USD"],
      note,
    ];
  if (id === "agent")
    return [
      [
        "Agent",
        ["Work assistant", "Knowledge assistant", "Review assistant"][index],
      ],
      ["Version", `${index + 1}.0`],
      ["Assigned by", owner],
      ["Pool", "Approved sample assistants"],
      ["Scope", location],
      note,
    ];
  if (id === "platform_administration")
    return [
      ["Resource", `sample-workspace-${index + 1}`],
      ["Operation", section.label],
      ["Responsible operator", "Sample platform operator"],
      ["Environment", "Fictional preview environment"],
      ["Last review", "2026-09-06", "date"],
      note,
    ];
  if (id === "collaboration")
    return [
      ["Topic", `${section.label} · ${location}`],
      ["Participants", people.join(", ")],
      ["Audience", "Sample team"],
      ["Linked record", `WORK-00${index + 1}`],
      note,
    ];
  return [
    ["Name", `${section.label} · ${location}`],
    ["Owner", owner],
    ["Review date", "2026-09-15", "date"],
    note,
  ];
}
export function capabilitySamples(
  definition: ModuleDefinition,
  section: ModuleSection,
): { records: PreviewRecord[]; fields: PreviewField[]; steps: PreviewStep[] } {
  const readOnly = [
    "analytics",
    "directory",
    "library",
    "health",
    "timeline",
  ].includes(section.kind);
  const statuses = readOnly
    ? ["Available", "In review", "Active"]
    : ["Draft", "In review", "Completed"];
  const records = locations.map((location, index) => {
    const fields = fieldsFor(definition, section, index).map(
      ([label, value, type]) => field(label, value, type),
    );
    const titleField = fields.find((item) =>
      [
        "name",
        "employee",
        "employee_name",
        "customer",
        "supplier",
        "course",
        "project",
        "agent",
        "document_title",
        "role",
        "item",
        "resource",
      ].includes(item.key),
    );
    return {
      id: `sample--${section.id}--${index + 1}`,
      title: String(titleField?.value ?? `${section.label} · ${location}`),
      description: `${section.label} · ${location}`,
      status: statuses[index],
      fields,
      source: "sample" as const,
    };
  });
  const steps: PreviewStep[] = [
    {
      status: "Prepare",
      action: "Inspect the source information",
      next: "Review",
      help: section.description,
      gate: "",
      handoff: "",
    },
    {
      status: "Review",
      action: `Review ${section.label.toLowerCase()}`,
      next: "Ready",
      help: "Check the fields, related records, and the responsibility for the next step. This preview does not perform business actions.",
      gate: "",
      handoff: "",
    },
  ];
  return { records, fields: records[0].fields, steps };
}
