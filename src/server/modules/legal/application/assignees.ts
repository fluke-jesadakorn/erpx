import type { WorkContext } from "../../identity_access/public/authorization";
import type { LegalItem } from "../domain/model";
import { legalPeopleQuery, type LegalPeopleQuery, type LegalPerson } from "../domain/people";
export function createLegalPeopleQuery(ports: {
  actor: WorkContext; require(): void; error(status: number, code: string, message: string): Error;
  item(id: string): Promise<LegalItem>;
  activeUnit(id: string, kind: string): Promise<boolean>; departmentInBranch(department: string, branch: string): Promise<boolean>;
  people(query: LegalPeopleQuery, scope: { branch: string | null; department: string | null; permission: string | null; itemId: string | null }): Promise<LegalPerson[]>;
}) {
  return async (raw: unknown = {}) => {
    ports.require(); const q = legalPeopleQuery.parse(raw), a = ports.actor, item = q.itemId ? await ports.item(q.itemId) : null;
    const branch = item ? item.branch_id : q.branchId ?? a.branchId, department = item ? item.department_id : q.departmentId ?? a.departmentId;
    if ((a.branchId && branch !== a.branchId) || (a.departmentId && department !== a.departmentId)) throw ports.error(403, "LEGAL_SCOPE", "Choose people in your current organization scope.");
    for (const [id, kind] of [[q.branchId, "branch"], [q.departmentId, "department"]]) if (id && !await ports.activeUnit(id, kind!)) throw ports.error(403, "LEGAL_SCOPE", "Choose an active unit in this Company.");
    if (department && branch && !await ports.departmentInBranch(department, branch)) throw ports.error(403, "LEGAL_SCOPE", "Choose a Department in this Branch.");
    const permission = q.purpose === "reviewer" ? "legal.approve" : null, rows = await ports.people(q, { branch, department, permission, itemId: item?.id ?? null });
    return [...new Map(rows.map(person => [person.id, person])).values()].map(person => person.id === a.userId ? { ...person, name: a.name } : person);
  };
}
