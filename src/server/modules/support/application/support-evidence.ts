import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { AttachmentBindingContract } from "@/server/modules/files/public/attachments/types";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { SupportCommandPorts, SupportCommand, SupportRow, SupportMutation } from "./command";
import { supportEvidenceBindSchema, supportEvidenceRevokeSchema, supportEvidenceActive, supportEvidenceBindingContract, type SupportEvidenceBind, type SupportEvidenceRevoke, type SupportEvidenceBinding } from "../domain/support-evidence";
function requireSupportEvidenceWrite(ports: Pick<SupportCommandPorts, "error"> & {
    readOnly(): boolean;
}) { if (ports.readOnly())
    throw ports.error(503, 'ATTACHMENTS_READ_ONLY', 'Support evidence changes are disabled during attachment rollback. Existing clean bound evidence remains readable.'); }
export function createSupportEvidenceGuard(ports: Pick<SupportCommandPorts, "error"> & {
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    eligible(a: WorkContext, id: string): Promise<boolean>;
    guard(a: WorkContext, id: string, expectedVersion: number, key: string, manager: boolean): Promise<SupportMutation>;
}) { return async (a: WorkContext, id: string, expectedVersion: number, key: string) => { const row = await ports.ticket(a, id); if (row.source_kind === 'field_service')
    throw ports.error(409, 'SOURCE_WORKSPACE_REQUIRED', 'Continue this case in Field Service.'); if (!Number.isSafeInteger(expectedVersion) || expectedVersion < 1)
    throw ports.error(422, 'VERSION_REQUIRED', 'Support evidence requires the current case version.'); const manager = hasPermission(a, 'support.manage'); if (!manager && !(hasPermission(a, 'support.work') && row.owner_id === a.userId))
    throw ports.error(403, 'SUPPORT_EVIDENCE_OWNER', 'Only the assigned colleague or a Support manager may change case evidence.'); if (!await ports.eligible(a, id))
    throw ports.error(409, 'SUPPORT_EVIDENCE_ASSIGNMENT', 'Your current employment and assignment no longer permit changes to this case evidence.'); if (!supportEvidenceActive(row.status))
    throw ports.error(409, 'SUPPORT_EVIDENCE_LOCKED', 'Evidence is frozen once a resolution is proposed. Return the case for rework before changing it.'); return { row, mutation: await ports.guard(a, id, expectedVersion, key, manager) }; }; }
export function createSupportEvidence(ports: {
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    items(id: string): Promise<SupportRow[]>;
    eligible(a: WorkContext, id: string): Promise<boolean>;
    readOnly(): boolean;
}) { return async (a: WorkContext, id: string) => { const row = await ports.ticket(a, id); if (!hasPermission(a, 'support.read'))
    return { items: [] as SupportRow[], canChange: false, canShare: false }; const items = await ports.items(id), actorEligible = row.source_kind !== 'field_service' && await ports.eligible(a, id), mutable = row.source_kind !== 'field_service' && !ports.readOnly() && actorEligible && supportEvidenceActive(row.status); return { items, canChange: mutable && (hasPermission(a, 'support.manage') || (hasPermission(a, 'support.work') && row.owner_id === a.userId && Number(row.owner_eligible) > 0)), canShare: mutable && hasPermission(a, 'support.manage') }; }; }
export function createBindSupportEvidence(ports: SupportCommandPorts & {
    readOnly(): boolean;
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    binding(a: WorkContext, id: string, input: SupportEvidenceBind, key: string, contract: AttachmentBindingContract): Promise<SupportEvidenceBinding>;
    prepare(command: SupportCommand, input: SupportEvidenceBind, binding: SupportEvidenceBinding): Promise<SupportMutation>;
}) { return async (a: WorkContext, id: string, raw: unknown, key: string) => { const input = supportEvidenceBindSchema.parse(raw), op = 'support.evidence.bind', prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; requireSupportEvidenceWrite(ports); const row = await ports.ticket(a, id); if (row.source_kind === 'field_service')
    throw ports.error(409, 'SOURCE_WORKSPACE_REQUIRED', 'Continue this case in Field Service.'); const prepared = await ports.binding(a, id, input, key, supportEvidenceBindingContract), now = ports.now(), command: SupportCommand = { actor: a, key, operation: op, id, input, now, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1, manifest: prepared.manifest }, scope: { companyId: a.companyId, branchId: row.branch_id ? String(row.branch_id) : null, departmentId: row.department_id ? String(row.department_id) : null } }; return ports.commit(command, await ports.prepare(command, input, prepared)); }; }
export function createRevokeSupportEvidence(ports: SupportCommandPorts & {
    readOnly(): boolean;
    guard(a: WorkContext, id: string, expectedVersion: number, key: string): Promise<{
        row: SupportRow;
        mutation: SupportMutation;
    }>;
    prepare(command: SupportCommand, input: SupportEvidenceRevoke, guard: SupportMutation): Promise<SupportMutation>;
}) { return async (a: WorkContext, id: string, raw: unknown, key: string) => { const input = supportEvidenceRevokeSchema.parse(raw), op = 'support.evidence.revoke', prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; requireSupportEvidenceWrite(ports); const { row, mutation } = await ports.guard(a, id, input.expectedVersion, key), now = ports.now(), command: SupportCommand = { actor: a, key, operation: op, id, input, now, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1 }, scope: { companyId: a.companyId, branchId: row.branch_id ? String(row.branch_id) : null, departmentId: row.department_id ? String(row.department_id) : null } }; return ports.commit(command, await ports.prepare(command, input, mutation)); }; }
