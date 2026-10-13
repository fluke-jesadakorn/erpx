import { ServiceNavigation } from "@/components/field_service/service-navigation";
import type { PermissionActor } from "@/lib/identity_access/iam";
import { type NavigationScopeKind } from "@/lib/work_projection/workplace-navigation";
export function PeopleNavigation({
  base,
  actor,
  active,
  scopeKind,
  canPayroll = [
    "holding_owner",
    "hr_admin",
    "people_manager",
    "payroll_preparer",
    "payroll_approver",
    "auditor",
  ].includes(actor.role) &&
    !base.includes("/branches/") &&
    !base.includes("/departments/"),
}: {
  base: string;
  actor: PermissionActor & { beta?: boolean };
  active:
    | "roster"
    | "directory"
    | "users"
    | "organization"
    | "access"
    | "payroll"
    | "settings"
    | "handbook"
    | "learn"
    | "evaluation";

  scopeKind: NavigationScopeKind;
  canPayroll?: boolean;
}) {
  const ids: Record<typeof active, string> = {
    directory: "people-directory",
    users: "people-users-access",
    evaluation: "people-evaluation",
    roster: "people-directory",
    organization: "people-directory",
    payroll: "people-payroll",
    access: "people-access",
    settings: "people-directory",
    handbook: "handbook",
    learn: "learn",
  };
  return (
    <ServiceNavigation
      context={{
        workspaceBase: base,
        scopeKind,
        beta: actor.beta,
        role: actor.role,
        permissions: actor.iam?.permissions ?? actor.permissions,
      }}
      groupId="people"
      activeId={ids[active]}
      exclude={canPayroll ? [] : ["people-payroll"]}
    />
  );
}
