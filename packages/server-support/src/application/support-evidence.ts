import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { SupportCommandPorts, SupportRow, SupportMutation } from "@erpx/server-support/application/command";
import { supportEvidenceActive } from "@erpx/server-support/domain/support-evidence";
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
