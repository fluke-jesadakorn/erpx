export const businessRoles = {
  it_admin: ["collaboration"],
  employee: ["people_payroll", "collaboration"],
  people_manager: ["people_payroll", "collaboration"],
  hr_admin: ["people_payroll", "collaboration"],
  payroll_preparer: ["people_payroll"],
  payroll_approver: ["people_payroll"],
  finance_preparer: ["finance"],
  finance_approver: ["finance"],
  treasury: ["finance", "people_payroll"],
  sales_operator: ["sales", "crm", "customer_360", "inventory"],
  sales_manager: ["sales", "crm", "customer_360", "inventory"],
  purchasing_operator: ["procurement", "inventory"],
  purchasing_manager: ["procurement", "inventory"],
  warehouse_manager: ["inventory", "operations"],
  operations_manager: ["inventory", "operations", "procurement"],
  auditor: ["finance", "people_payroll", "inventory"],
} as const;
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
