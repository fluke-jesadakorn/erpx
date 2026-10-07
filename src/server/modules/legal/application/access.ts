import type { WorkContext } from "../../identity_access/public/authorization";
import type { LegalItem } from "../domain/model";
export type LegalAccessRepository = {
  visible(id: string): Promise<Omit<LegalItem, "data"> | null>;
  enabled(): Promise<boolean>;
  editor(id: string): Promise<boolean>;
  data(id: string, kind: LegalItem["kind"]): Promise<{ data: LegalItem["data"] } | null>;
  eligible(id: string, scope: { branch_id: string | null; department_id: string | null }, permission: string | null): Promise<boolean>;
};
export function createLegalAccessApplication(ports: { actor: WorkContext; repository: LegalAccessRepository; require(permission: string): void; error(status: number, code: string, message: string): Error }) {
  const fail = (message: string, code = "LEGAL_INVALID", status = 422): never => { throw ports.error(status, code, message); };
  async function access(id: string, permission = "legal.read"): Promise<LegalItem> {
    ports.require(permission);
    const row = await ports.repository.visible(id);
    if (!row) return fail("Legal record is unavailable in your current scope.", "LEGAL_NOT_FOUND", 404);
    if (permission !== "legal.read" && !await ports.repository.enabled()) fail("Legal is read-only until enabled for this Company.", "LEGAL_DISABLED", 409);
    if (permission === "legal.edit" && !await ports.repository.editor(id)) fail("An editor membership is required.", "LEGAL_EDITOR_REQUIRED", 403);
    const detail = await ports.repository.data(id, row.kind);
    return { ...row, data: typeof detail?.data === "string" ? JSON.parse(detail.data) : detail?.data ?? {} };
  }
  async function eligible(id: string, scope = { branch_id: ports.actor.branchId, department_id: ports.actor.departmentId }, permission: string | null = null) {
    if (!await ports.repository.eligible(id, scope, permission)) fail("Choose an active person with access to this Company and record scope.", "LEGAL_PERSON_UNAVAILABLE");
  }
  return { access, eligible, enabled: ports.repository.enabled };
}
