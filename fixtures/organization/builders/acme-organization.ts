import type { BusinessRole } from "../../../src/lib/identity_access/business-roles";

/** Explicit fictional staffing plan. Roles are assigned here, never inferred at runtime. */
export const demoCompany = "acme-co";
export const demoPrefix = "demo-org-v1";
export const demoExpiry = Date.UTC(2027, 8, 14);
export const demoBranches = [
  { id: "bangkok", name: "Bangkok", country: "TH", surname: "Suwan" },
  { id: "new-york", name: "New York", country: "US", surname: "Bennett" },
] as const;
export const demoDepartments = [
  { id: "operations", name: "Operations", specialist: "Operations Coordinator" },
  { id: "finance", name: "Finance / Treasury", specialist: "Treasury Officer" },
  { id: "people", name: "People / HR", specialist: "People Operations Specialist" },
  { id: "accounting", name: "Accounting", specialist: "Accountant" },
  { id: "sales", name: "Sales", specialist: "Sales Representative" },
  { id: "purchasing", name: "Purchasing", specialist: "Purchasing Specialist" },
  { id: "warehouse", name: "Warehouse / Logistics", specialist: "Warehouse Coordinator" },
  { id: "it", name: "IT", specialist: "IT Support Specialist" },
  { id: "customer-service", name: "Customer Service", specialist: "Customer Support Specialist" },
] as const;
const firstNames = ["Anya", "Ben", "Clara", "Dev", "Elena", "Finn", "Gia", "Hana", "Ivan", "Jade", "Kai", "Lara", "Mason", "Nora", "Omar", "Pim", "Quinn", "Rina", "Seth", "Tara", "Uri", "Vera", "Wade", "Xena", "Yara", "Zane", "Ada", "Bryn", "Cleo", "Dara", "Evan", "Fern", "Gavin", "Hazel", "Ida", "Jonah", "Kira", "Leon", "Mira", "Nolan", "Opal", "Paolo", "Reva", "Soren", "Tessa", "Victor"];
export type DemoGrant = { role: BusinessRole; scope: "holding" | "branch" | "department" };
export type DemoStaff = {
  key: string; personId: string; engagementId: string; positionId: string; userId: string;
  name: string; email: string; branchId: string; departmentId: string; country: string;
  title: string; level: number; managerPersonId: string; reportsToPositionId: string;
  grants: DemoGrant[];
};
export function demoInitialPassword(person: Pick<DemoStaff, "name">) { return `${person.name.split(" ")[0]}-ERPX-2026!`; }
export function demoStaffPlan(ceo: { personId: string; positionId: string }): DemoStaff[] {
  const plan: DemoStaff[] = [];
  for (const branch of demoBranches) {
    let ordinal = 0;
    const add = (department: string, seat: string, title: string, level: number, manager: { personId: string; positionId: string }, grants: DemoGrant[]) => {
      const key = `${demoPrefix}-${branch.id}-${department}-${seat}`;
      const person: DemoStaff = { key, personId: `${key}-person`, engagementId: `${key}-engagement`, positionId: `${key}-position`, userId: `${key}-user`,
        name: `${firstNames[ordinal++]} ${branch.surname}`, email: `demo.${branch.id}.${department}.${seat}@erpx.dev`,
        branchId: branch.id, departmentId: `dept:${branch.id}:${department}`, country: branch.country, title, level,
        managerPersonId: manager.personId, reportsToPositionId: manager.positionId,
        grants: [{ role: "employee", scope: level === 2 ? "branch" : "department" }, ...grants] };
      plan.push(person); return person;
    };
    const director = add("operations", "director", `${branch.name} Branch Director`, 2, ceo, [{ role: "people_manager", scope: "branch" }, { role: "operations_manager", scope: "branch" }]);
    for (const department of demoDepartments) {
      let manager: DemoStaff = director;
      let supervisor: DemoStaff = director;
      for (const [index, seat, level] of [[0, "manager", 3], [1, "supervisor", 4], [2, "senior", 5], [3, "staff", 6], [4, "associate", 6]] as const) {
        const grants: DemoGrant[] = [];
        const grant = (role: BusinessRole, scope: DemoGrant["scope"] = "department") => grants.push({ role, scope });
        if (level <= 4) grant("people_manager");
        // Accounting serves both branches; preparers, approvers and payers are distinct people.
        if (department.id === "accounting" && index < 4) grant(level <= 4 ? "finance_approver" : "finance_preparer", "holding");
        if (department.id === "finance" && index < 4) grant("treasury", "holding");
        if (department.id === "people") {
          if (index === 0) { grant("hr_admin", "holding"); grant("payroll_approver", "holding"); }
          if (index === 1) grant("hr_admin", "branch");
          if (index === 2) grant("payroll_preparer", "holding");
        }
        if (department.id === "sales") grant(level <= 4 ? "sales_manager" : "sales_operator", "branch");
        if (department.id === "purchasing") grant(level <= 4 ? "purchasing_manager" : "purchasing_operator", "branch");
        if (department.id === "warehouse" && index < 4) grant("warehouse_manager", "branch");
        if (department.id === "operations" && level <= 4) grant("operations_manager", "branch");
        if (department.id === "it" && index === 0) grant("it_admin", "holding");
        const title = level === 3 ? `${department.name} Manager` : level === 4 ? `${department.name} Supervisor` : `${level === 5 ? "Senior " : ""}${department.specialist}${index === 4 ? " (Associate)" : ""}`;
        const person = add(department.id, seat, title, level, level === 3 ? director : level === 4 ? manager : supervisor, grants);
        if (level === 3) manager = person;
        if (level === 4) supervisor = person;
      }
    }
  }
  return plan;
}
