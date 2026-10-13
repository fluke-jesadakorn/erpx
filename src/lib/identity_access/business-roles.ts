export const businessRoles = {
  support_agent: ["support"],
  support_manager: ["support"],
  quality_inspector: ["quality"],
  quality_manager: ["quality"],
  legal_counsel: ["legal", "collaboration"],
  legal_manager: ["legal", "collaboration"],
  legal_requester: ["legal", "collaboration"],
  executive_author: ["collaboration"],
  pms_reservation: ["pms"],
  pms_frontdesk: ["pms"],
  pms_cashier: ["pms"],
  pms_housekeeping: ["pms"],
  pms_supervisor: ["pms"],
  pms_maintenance: ["pms"],
  pms_night_auditor: ["pms"],
  pms_manager: ["pms"],
  pos_cashier: ["pos"],
  pos_server: ["pos"],
  pos_kitchen: ["pos"],
  pos_manager: ["pos", "inventory"],
  it_admin: ["collaboration"],
  employee: ["people_payroll", "collaboration"],
  people_manager: ["people_payroll", "collaboration"],
  hr_admin: ["people_payroll", "collaboration"],
  payroll_preparer: ["people_payroll"],
  payroll_approver: ["people_payroll"],
  finance_preparer: ["finance"],
  finance_approver: ["finance", "pms"],
  treasury: ["finance", "people_payroll"],
  sales_operator: ["sales", "crm", "customer_360", "inventory", "support"],
  sales_manager: ["sales", "crm", "customer_360", "inventory", "support", "marketing"],
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
