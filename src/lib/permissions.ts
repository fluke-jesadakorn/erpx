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
  ["Approved payroll; statutory adapter required"],
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
for (const p of definitions) {
  if (["iam.self", "self.requests", "self.tasks"].includes(p.id))
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
for (const p of definitions) {
  if (
    [
      "iam.roles.manage",
      "iam.directory.manage",
      "iam.recovery",
      "company.owner",
    ].includes(p.id)
  )
    p.permittedScopes = ["holding"];
  if (
    [
      "inventory.read",
      "inventory.prepare",
      "inventory.transfer.review",
    ].includes(p.id)
  )
    p.permittedScopes = ["holding", "branch"];
  if (p.id === "inventory.approve") p.permittedScopes = ["holding"];
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
