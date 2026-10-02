import { moduleRegistry, moduleAvailability } from "@/data/module-registry";
import { businessRoles } from "./business-roles";
import type {
  AccessDecision,
  PermissionActor,
  PermissionDefinition,
} from "./iam";

const owner = "holding_owner";
const financeRead = [
  owner,
  "finance_preparer",
  "finance_approver",
  "treasury",
  "auditor",
];
const financeWrite = [
  owner,
  "finance_preparer",
  "finance_approver",
  "treasury",
];
const hr = [owner, "hr_admin"];
const stock = [owner, "warehouse_manager", "operations_manager"];
const staff = [owner, ...Object.keys(businessRoles)];
const noticeBroadcasters = [owner, "hr_admin", ...Object.keys(businessRoles).filter((role) => role.endsWith("_manager"))];
const definitions: PermissionDefinition[] = [];
function define(
  id: string,
  label: string,
  module: string,
  roles: readonly string[],
  conditions: readonly string[] = [],
  risk: PermissionDefinition["risk"] = "normal",
) {
  definitions.push({
    id,
    label,
    module,
    roles,
    conditions,
    risk,
    availability: "live",
    namespace: "customer",
  });
}

define("manage_erpx.access", "Access Manage ERPX", "manage_erpx", [owner], ["ERPX Holding and active Platform assignment"], "privileged");
definitions[definitions.length - 1].permittedScopes = ["holding"];
define("billing.usage.read", "View measured usage", "billing", [owner, "operations_manager"], ["Holding owners see their Holding; branch operations managers see only their selected Branch"]);
definitions[definitions.length - 1].permittedScopes = ["holding", "branch"];
define("inbox.notice.broadcast", "Broadcast a staff notice", "collaboration", noticeBroadcasters, ["Current Company, Branch, or Department assignment scope"], "privileged");

for(const action of ["read","create","work","manage"] as const) define(`support.${action}`,`Support ${action}`,"support",action==='manage'?[owner,"support_manager"]:action==='work'?[owner,"support_agent","support_manager"]:[owner,"support_agent","support_manager","sales_operator","sales_manager"],["Source scope; current eligible owner; independent recorded customer confirmation"],action==='manage'?'privileged':'normal');
const legalPermissions = ["read", "create", "edit", "review", "approve", "sign", "share", "export", "hold", "hold.release", "manage", "ai"] as const;
define("legal.destroy","Authorize Legal destruction","legal",[owner,"legal_manager"],["Independent approval; no active hold; approved retention policy"]);
for (const action of legalPermissions) define(`legal.${action}`, `Legal: ${action}`, "legal", action === "manage" ? [owner, "legal_manager"] : [owner, "legal_counsel", "legal_manager", ...(["read","create","edit"].includes(action)?["legal_requester"]:[])], ["Explicit membership for confidential and restricted records; requesters may access only their own requests"]);
define("iam.read", "View identities and access", "identity_access", [
  owner,
  "hr_admin",
  "it_admin",
  "auditor",
]);
define(
  "iam.manage",
  "Manage identities and assignments",
  "identity_access",
  hr,
  [],
  "privileged",
);
define(
  "iam.roles.manage",
  "Create and revise custom roles",
  "identity_access",
  [owner],
  [],
  "privileged",
);
define(
  "iam.directory.manage",
  "Manage directory connections",
  "identity_access",
  [owner, "it_admin"],
  [],
  "privileged",
);
define("iam.directory.apply", "Apply directory and resource changes", "identity_access", [owner, "it_admin"], ["Preview and fresh MFA"], "privileged");
define(
  "iam.review",
  "Review sensitive access changes",
  "identity_access",
  [owner],
  ["Independent reviewer and fresh MFA"],
  "privileged",
);
define(
  "iam.export",
  "Export access evidence",
  "identity_access",
  [owner, "auditor"],
  [],
  "privileged",
);
define(
  "iam.recovery",
  "Manage emergency access",
  "identity_access",
  [owner],
  ["Fresh MFA and independent review"],
  "privileged",
);
define("iam.self", "View own access and sessions", "identity_access", staff);
define(
  "company.owner",
  "Company ownership administration",
  "organization",
  [owner],
  [],
  "privileged",
);
define(
  "organization.manage",
  "Manage organization and approval templates",
  "organization",
  hr,
);
define("people.read", "View employee directory", "people_payroll", [
  ...hr,
  "people_manager",
]);
define("people.manage", "Maintain employee details", "people_payroll", [
  ...hr,
  "people_manager",
]);
define("people.custom_attributes.write", "Edit person custom attribute values", "people_payroll", [owner, "hr_admin", "people_manager", "it_admin"], ["Person in selected Company; field access rules"], "privileged");
define(
  "people.documents.read",
  "Read restricted employee documents",
  "people_payroll",
  hr,
  ["Exact employee scope and purpose policy"],
  "privileged",
);
define(
  "people.documents.manage",
  "Manage restricted employee documents",
  "people_payroll",
  hr,
  ["Exact employee scope, purpose policy and current record state"],
  "privileged",
);
define(
  "payroll.documents.read",
  "Read restricted payroll documents",
  "people_payroll",
  [owner, "hr_admin", "payroll_preparer", "payroll_approver", "auditor"],
  ["Holding scope and exact payroll record"],
  "privileged",
);
define(
  "payroll.documents.manage",
  "Manage restricted payroll documents",
  "people_payroll",
  [owner, "hr_admin", "payroll_preparer"],
  ["Holding scope and exact payroll record"],
  "privileged",
);
define(
  "talent.documents.read",
  "Read restricted talent documents",
  "people_payroll",
  [owner, "hr_admin", "people_manager"],
  ["Assigned requisition or candidate scope and exact document purpose"],
  "privileged",
);
define(
  "talent.documents.manage",
  "Manage restricted talent documents",
  "people_payroll",
  hr,
  ["Assigned requisition or candidate scope and exact document purpose"],
  "privileged",
);
define(
  "learning.documents.read",
  "Read learning documents",
  "people_payroll",
  [owner, "hr_admin", "people_manager"],
  ["Exact course, enrollment or employee scope"],
  "privileged",
);
define(
  "learning.documents.manage",
  "Manage learning documents",
  "people_payroll",
  hr,
  ["Exact course, enrollment or employee scope"],
  "privileged",
);
define(
  "benefits.review",
  "Review benefit records",
  "people_payroll",
  [owner, "hr_admin", "people_manager"],
  ["Exact benefit record and independent workflow step"],
  "privileged",
);
define(
  "benefits.documents.read",
  "Read restricted benefit documents",
  "people_payroll",
  hr,
  ["Exact benefit record; employee ownership never grants access to internal HR documents"],
  "privileged",
);
define(
  "benefits.documents.manage",
  "Manage restricted benefit documents",
  "people_payroll",
  hr,
  ["Exact benefit record and purpose policy"],
  "privileged",
);
define(
  "people.compensation.read",
  "View compensation",
  "people_payroll",
  [...hr, "payroll_preparer", "payroll_approver"],
  ["Own record or authorized staff scope"],
);
define("payroll.read", "View payroll", "people_payroll", [
  ...hr,
  "payroll_preparer",
  "payroll_approver",
  "finance_approver",
  "people_manager",
  "auditor",
]);
define("payroll.prepare", "Prepare payroll", "people_payroll", [
  ...hr,
  "payroll_preparer",
]);
define(
  "payroll.approve",
  "Approve payroll",
  "people_payroll",
  [owner, "people_manager", "payroll_approver"],
  ["Assigned reviewer; different preparer"],
  "privileged",
);
define("finance.read", "View finance", "finance", financeRead);
define("finance.write", "Prepare accounting records", "finance", financeWrite);
define(
  "finance.approve",
  "Approve and post accounting records",
  "finance",
  [owner, "finance_approver"],
  ["Independent approver; open period and valid workflow"],
  "privileged",
);
define(
  "finance.pay",
  "Execute treasury payments",
  "finance",
  [owner, "finance_approver", "treasury"],
  ["Approved obligation and independent payer"],
  "privileged",
);
define("expense.read", "View expense accounting", "finance", financeRead);
define(
  "expense.prepare",
  "Prepare expense accounting",
  "finance",
  ["finance_preparer"],
  ["Assigned accounting preparer"],
);
define(
  "expense.approve",
  "Approve expense accounting",
  "finance",
  [owner, "finance_approver"],
  ["Assigned independent accounting approver"],
  "privileged",
);
define(
  "expense.pay",
  "Execute expense payments",
  "finance",
  ["treasury"],
  ["Assigned independent treasury operator"],
  "privileged",
);
define(
  "spending.approve",
  "Approve staff spending requests",
  "people_payroll",
  ["people_manager"],
  ["Saved reporting route and independent person"],
  "privileged",
);
define("commerce.read", "View business operations", "sales", [
  ...financeRead,
  "sales_manager",
  "sales_operator",
  "purchasing_manager",
  "purchasing_operator",
  "warehouse_manager",
  "operations_manager",
]);
define("commerce.sales", "Prepare sales orders", "sales", [
  owner,
  "sales_manager",
  "sales_operator",
]);
define("commerce.purchase", "Prepare purchasing orders", "procurement", [
  owner,
  "purchasing_manager",
  "purchasing_operator",
]);
define(
  "commerce.approve",
  "Approve commercial orders",
  "sales",
  [owner, "sales_manager", "purchasing_manager"],
  ["Order state and independent reviewer"],
  "privileged",
);
define("inventory.read", "View inventory counts", "inventory", [
  ...stock,
  "finance_preparer",
  "finance_approver",
  "treasury",
  "auditor",
]);
define(
  "inventory.prepare",
  "Prepare inventory operations",
  "inventory",
  stock,
  ["Branch scope; department scope unavailable"],
);
define(
  "inventory.approve",
  "Approve inventory adjustments",
  "inventory",
  [owner, "finance_approver"],
  ["Company scope and independent reviewer"],
  "privileged",
);
define(
  "inventory.transfer.review",
  "Review stock transfers",
  "inventory",
  [...stock, "finance_approver"],
  ["Target branch or company scope; independent reviewer"],
  "privileged",
);
define("quality.read", "quality read", "quality", ["holding_owner", "warehouse_manager", "operations_manager", "quality_inspector", "quality_manager", "auditor"], ["Whole warehouse/product scope; independent inspection and release"], "normal");
define("quality.hold", "quality hold", "quality", ["holding_owner", "warehouse_manager", "operations_manager", "quality_inspector", "quality_manager"], ["Whole warehouse/product scope; independent inspection and release"], "normal");
define("quality.inspect", "quality inspect", "quality", ["holding_owner", "quality_inspector", "quality_manager"], ["Whole warehouse/product scope; independent inspection and release"], "normal");
define("quality.release", "quality release", "quality", ["holding_owner", "quality_manager"], ["Whole warehouse/product scope; independent inspection and release"], "privileged");
define(
  "records.review",
  "Review module records",
  "collaboration",
  [owner, "people_manager"],
  ["Independent reviewer and record scope"],
  "privileged",
);
define(
  "ai.manage",
  "Manage AI provider connections",
  "collaboration",
  [owner, "it_admin"],
  [],
  "privileged",
);
for (const [id, label] of [["executive.access", "Open Executive"], ["executive.report.create", "Create executive reports"], ["executive.report.edit", "Edit executive reports"], ["executive.report.share", "Share executive reports"], ["executive.data.distribute", "Distribute selected executive aggregates"]]) {
  define(id, label, "collaboration", id === "executive.data.distribute" ? [owner] : [owner,"executive_author"], ["Current executive eligibility; private ownership and source policies still apply"], "privileged");
}
define("ai.context.cross", "Continue AI conversations across authorized pages", "collaboration", [owner,"executive_author"], ["Current assignment and source permissions still apply"], "privileged");
for (const [role, modules] of Object.entries(businessRoles))
  for (const moduleId of modules) {
    const id = `module.${moduleId}.read`,
      existing = definitions.find((p) => p.id === id);
    if (existing) existing.roles = [...existing.roles, role];
    else
      define(id, `Open ${moduleId.replaceAll("_", " ")}`, moduleId, [
        owner,
        role,
      ]);
  }

// Finance owns the controlled Payroll → GL posting handoff. Keep the
// module entitlement aligned with migration 0276, which grants the same
// read-only Payroll surface alongside payroll.read.
const peoplePayrollRead = definitions.find(
  (permission) => permission.id === "module.people_payroll.read",
);
if (peoplePayrollRead && !peoplePayrollRead.roles.includes("finance_approver"))
  peoplePayrollRead.roles = [...peoplePayrollRead.roles, "finance_approver"];

define("records.read", "Read live module records", "collaboration", staff, [
  "Live module entitlement and record scope",
]);
define(
  "records.write",
  "Create and edit live module records",
  "collaboration",
  staff,
  ["Live module entitlement, record state and scope"],
);
// Workflow eligibility is a permission; saved route labels remain readable evidence.
for (const role of [owner, ...Object.keys(businessRoles)])
  define(
    `workflow.${role}`,
    `Participate as ${role.replaceAll("_", " ")} in assigned workflows`,
    "workflow",
    [role],
    [
      "Saved workflow route, current staff eligibility, scope and independent person",
    ],
  );
define(
  "people.compensation.payroll",
  "Read compensation for payroll",
  "people_payroll",
  ["payroll_preparer", "payroll_approver"],
  ["Authorized staff scope"],
);
define(
  "people.payroll.details",
  "Read staff and payroll details",
  "people_payroll",
  [owner, "hr_admin", "people_manager", "payroll_preparer", "payroll_approver"],
  ["Staff record scope"],
);
define(
  "people.compensation.approve",
  "Approve compensation and statutory configuration",
  "people_payroll",
  [owner, "hr_admin", "payroll_approver"],
  ["Independent reviewer and record state"],
);
define(
  "people.time.manage",
  "Manage time and payroll preparation",
  "people_payroll",
  [owner, "hr_admin", "people_manager", "payroll_preparer"],
  ["Active staff scope"],
);
define(
  "payroll.configuration.read",
  "Read statutory payroll configuration",
  "people_payroll",
  [owner, "hr_admin", "payroll_preparer", "payroll_approver", "auditor"],
);
define(
  "payroll.calculate",
  "Calculate payroll preview",
  "people_payroll",
  [owner, "payroll_preparer", "payroll_approver", "finance_preparer"],
  ["Valid run state and assigned preparer"],
);
define(
  "payroll.payment.prepare",
  "Prepare approved payroll payment",
  "people_payroll",
  [owner, "treasury", "finance_approver", "payroll_preparer"],
  ["Approved payroll run posted to the Finance ledger"],
);
define(
  "payroll.payment.execute",
  "Execute approved payroll payment",
  "people_payroll",
  [owner, "treasury"],
  ["Approved payment; independent payer"],
);
define("payroll.payment.read", "Read payroll payment runs", "people_payroll", [
  owner,
  "treasury",
  "finance_approver",
  "auditor",
]);
define(
  "expense.review.read",
  "Read routed expense accounting",
  "finance",
  [owner, "hr_admin", "finance_preparer", "finance_approver", "treasury"],
  ["Saved workflow visibility and record scope"],
);
define(
  "expense.audit.read",
  "Inspect expenses in accounting scope",
  "finance",
  [
    owner,
    "hr_admin",
    "finance_preparer",
    "finance_approver",
    "treasury",
    "auditor",
  ],
  ["Expense scope"],
);
define(
  "finance.period.select",
  "Select an open accounting period",
  "finance",
  [owner, "finance_preparer", "finance_approver"],
  ["Open period"],
);
define(
  "self.requests",
  "Participate in own requests",
  "people_payroll",
  staff,
  [
    "Linked person owns or participates in this request; active staff eligibility where required",
  ],
);
define("self.tasks", "Participate in assigned tasks", "collaboration", staff, [
  "Named assignee, requester or service team membership; current workflow state",
]);
const posStaff = [owner, "pos_cashier", "pos_server", "pos_kitchen", "pos_manager"];
const pmsStaff = [owner, "pms_reservation", "pms_frontdesk", "pms_cashier", "pms_housekeeping", "pms_supervisor", "pms_maintenance", "pms_night_auditor", "pms_manager"];
define("pms.read", "View hotel operations", "pms", [...pmsStaff,...financeRead]);
define("pms.configure", "Configure hotel rooms and rates", "pms", [owner,"pms_manager"]);
define("pms.reservations", "Manage hotel reservations", "pms", [owner,"pms_reservation","pms_frontdesk","pms_manager"]);
define("pms.frontdesk", "Check in and check out guests", "pms", [owner,"pms_frontdesk","pms_manager"]);
define("pms.cashier", "Manage guest folios and receipts", "pms", [owner,"pms_cashier","pms_frontdesk","pms_manager","treasury"]);
define("pms.housekeeping", "Work on hotel rooms and supplies", "pms", [owner,"pms_housekeeping","pms_supervisor","pms_maintenance","pms_manager"]);
define("pms.inspect", "Inspect hotel room readiness", "pms", [owner,"pms_supervisor","pms_manager"], ["Independent room inspection"]);
define("pms.maintenance", "Manage hotel maintenance blocks", "pms", [owner,"pms_maintenance","pms_manager"]);
define("pms.audit", "Run hotel end of day", "pms", [owner,"pms_night_auditor","pms_manager"]);
define("pms.approve", "Review hotel refunds, credit and cash differences", "pms", [owner,"pms_manager","finance_approver"], ["Independent reviewer"], "privileged");
define("pms.accounting.prepare", "Prepare PMS accounting mappings", "pms", [owner,"finance_preparer"], [], "privileged");
define("pms.accounting.approve", "Approve PMS accounting mappings", "pms", [owner,"finance_approver"], ["Independent reviewer"], "privileged");
define("pms.reports", "View PMS financial reports", "pms", [owner,"pms_manager","pms_night_auditor",...financeRead]);
for (const permission of definitions.filter(p=>p.id.startsWith("pms."))) permission.permittedScopes=["holding","branch"];
define("pos.read", "View point of sale", "pos", [...posStaff, ...financeRead]);
define("pos.sell", "Sell and manage POS shifts", "pos", [owner,"pos_cashier","pos_server","pos_manager"]);
define("pos.kitchen", "Operate POS kitchen", "pos", posStaff);
define("pos.approve", "Review POS refunds and cash differences", "pos", [owner,"pos_manager","finance_approver"], ["Independent reviewer"], "privileged");
define("pos.configure", "Configure POS and posting accounts", "pos", [owner,"finance_approver"], [], "privileged");
define("pos.payments.configure", "Connect company Stripe account", "pos", [owner,"finance_approver"], [], "privileged");
define("pos.reports", "Read POS financial reports", "pos", [owner,"pos_manager",...financeRead]);
for (const p of definitions.filter(p=>p.id.startsWith("pos."))) p.permittedScopes = p.id.includes("configure") ? ["holding"] : ["holding","branch"];

define("leave.self", "Use personal leave self-service", "people_payroll", staff);
define("leave.policy.manage", "Manage company leave policies", "people_payroll", hr, [], "privileged");
definitions.find(p=>p.id==='leave.policy.manage')!.permittedScopes=['holding'];
define(
  "leave.evidence.read",
  "Read restricted leave evidence",
  "people_payroll",
  hr,
  ["Requester, current eligible approver, recorded decision maker, or authorized HR only"],
  "privileged",
);
const rosterManagers = [owner, "hr_admin", "people_manager", "operations_manager"];
define("calendar.availability.read", "View availability dates without private details", "people_payroll", rosterManagers);
define("calendar.appointments.manage", "Manage appointments in scope", "people_payroll", rosterManagers);
define("calendar.announcements.publish", "Publish scoped announcements", "collaboration", rosterManagers);
define("roster.read", "View team rosters", "people_payroll", rosterManagers);
define("roster.manage", "Create and publish team rosters", "people_payroll", rosterManagers);
define("roster.approve", "Approve roster changes independently", "people_payroll", rosterManagers, ["Independent manager in roster scope"]);
define("self.roster", "Participate in own roster", "people_payroll", staff, ["Active staff in the roster department"]);

for (const p of definitions) {
  if (["iam.self", "self.requests", "self.tasks", "self.roster", "leave.self"].includes(p.id))
    p.identityBased = true;
}
for (const m of moduleRegistry)
  if (moduleAvailability(m) === "preview")
    definitions.push({
      id: `preview.${m.id}`,
      label: `${m.name} (preview)`,
      module: m.id,
      namespace: "customer",
      availability: "preview",
      risk: "normal",
      roles: [],
      conditions: [],
    });
for (const p of definitions.filter((p) => p.module === "identity_access"))
  definitions.push({
    ...p,
    id: `platform.${p.id}`,
    namespace: "platform",
    roles:
      p.id === "iam.self" || p.id === "iam.read" || p.id === "iam.export"
        ? ["platform_admin", "platform_auditor"]
        : ["platform_admin"],
  });
definitions.push({
  id: "platform.executive.read",
  label: "View Platform Executive insights",
  module: "platform_administration",
  roles: ["platform_admin", "platform_auditor"],
  conditions: ["PlatformContext and control-plane data only"],
  risk: "normal",
  availability: "live",
  namespace: "platform",
  permittedScopes: ["platform"],
});
for (const p of definitions) {
  if (
    [
      "iam.roles.manage",
      "iam.directory.manage",
      "iam.directory.apply",
      "iam.recovery",
      "company.owner",
    ].includes(p.id)
  )
    p.permittedScopes = ["holding"];
  if (
    [
      "quality.read", "quality.hold", "quality.inspect", "quality.release",
      "inventory.read",
      "inventory.prepare",
      "inventory.transfer.review",
    ].includes(p.id)
  )
    p.permittedScopes = ["holding", "branch"];
  if (p.id === "inventory.approve") p.permittedScopes = ["holding"];
  if (p.id.startsWith("payroll.documents.")) p.permittedScopes = ["holding"];
}
export const permissionCatalog: readonly PermissionDefinition[] = definitions;
export function permissionDefinition(id: string) {
  return permissionCatalog.find((p) => p.id === id);
}
export function builtInPermissions(role: string): string[] {
  return permissionCatalog
    .filter((p) => p.roles.includes(role))
    .map((p) => p.id);
}
export function hasPermission(
  actor: PermissionActor,
  permission: string,
): boolean {
  const definition = permissionDefinition(permission);
  if (!definition || definition.availability !== "live") return false;
  const scope = actor.scope?.kind ?? actor.assignmentScope;
  if (
    scope &&
    definition.permittedScopes &&
    !definition.permittedScopes.includes(scope)
  )
    return false;
  if (definition.identityBased) return !!actor.userId;
  const permissions = actor.iam?.permissions ?? actor.permissions;
  return permissions
    ? permissions.includes(permission)
    : definition.roles.includes(actor.role);
}
export function evaluateAccess(
  actor: PermissionActor,
  permission: string,
  options: {
    inspect?: boolean;
    valid?: boolean;
    reason?: string;
    expiresAt?: number;
    source?: string;
  } = {},
): AccessDecision {
  const p = permissionDefinition(permission),
    allowed = hasPermission(actor, permission);
  const result =
    !p || p.availability !== "live"
      ? "unavailable"
      : options.valid === false ||
          (!allowed && (!p.identityBased || !options.inspect))
        ? "denied"
        : options.inspect && (p.conditions.length || p.identityBased)
          ? "conditional"
          : "allowed";
  return {
    result,
    permission,
    role: actor.role,
    revision: actor.iam?.roleRevision ?? actor.policyRevision ?? 1,
    assignmentId: actor.assignmentId,
    scope: actor.assignmentScope,
    source: options.source,
    expiresAt: options.expiresAt,
    reasons: [
      options.reason ??
        (p?.identityBased
          ? "Identity-based participation, evaluated independently of the selected role."
          : !p
            ? "This capability is unavailable."
            : options.valid === false
              ? "The assignment is not active in this scope."
              : allowed
                ? "Granted by the selected role."
                : p.permittedScopes &&
                    (actor.scope?.kind ?? actor.assignmentScope) &&
                    !p.permittedScopes.includes(
                      (actor.scope?.kind ?? actor.assignmentScope)!,
                    )
                  ? "This action is unavailable in the selected workspace scope."
                  : "The selected role does not grant this permission."),
    ],
    conditions: allowed || p?.identityBased ? [...(p?.conditions ?? [])] : [],
  };
}

// Temporary adapters for existing callers. The result is always a catalog key;
// an unknown policy denies access instead of falling back to role-name checks.
export function permissionForRoles(roles: readonly string[]): string {
  const key = [...new Set(roles)].sort().join(",");
  return (
    permissionCatalog.find(
      (p) =>
        p.namespace === "customer" &&
        [...new Set(p.roles)].sort().join(",") === key,
    )?.id ?? "unavailable"
  );
}
export function workflowPermission(role: string) {
  return `workflow.${role}`;
}
export function hasWorkflowPermission(actor: PermissionActor, role: string) {
  return hasPermission(actor, workflowPermission(role));
}
