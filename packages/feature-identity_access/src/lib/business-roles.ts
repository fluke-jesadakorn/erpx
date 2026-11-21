import { businessRoles } from "@erpx/contracts/business-role-modules";
export { businessRoles };
export type BusinessRole = keyof typeof businessRoles;
export function businessRoleLabel(role: string) {
  return role === "it_admin" ? "IT administrator" : role.replaceAll("_", " ");
}

export function modulesForRole(role: string, assigned: string[]): string[] {
  const permitted = businessRoles[role as BusinessRole] as
    | readonly string[]
    | undefined;
  return permitted ? assigned.filter((id) => permitted.includes(id)) : assigned;
}
