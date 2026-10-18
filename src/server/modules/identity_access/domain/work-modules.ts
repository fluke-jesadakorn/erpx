import { modulesForRole } from "@/lib/identity_access/business-roles";

/** The owner policy used by both an acting Work context and product presentation. */
export function effectiveWorkModules(role: string, assigned: readonly string[], permissions: readonly string[], beta: boolean) {
  const modules = beta ? [] : role.startsWith("custom_")
    ? [...new Set(permissions.filter(permission => permission.startsWith("module.")).map(permission => permission.split(".")[1]))]
    : modulesForRole(role, [...assigned]);
  return permissions.includes("module.people_payroll.read") ? [...new Set([...modules, "people_payroll"])] : modules.filter(id => id !== "people_payroll");
}
