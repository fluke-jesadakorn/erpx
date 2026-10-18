import { finalRootOwnerProjectionSources as filesProjectionSources } from "../../files/public/final-root-owner-projection-sources";
import {supportAttachmentEffects} from "../../files/public/support-attachment-effects";
import "server-only";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Row } from "@/server/modules/commerce/public/commerce";
import type { PreparedAttachmentBinding } from "@/server/modules/files/public/attachments/types";
import { assertion } from "@/server/modules/workflow/public/commands";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { attachmentsV2ReadOnly } from "@/server/modules/files/public/attachments/feature";
import { getSupportTicket } from "./support";
import { supportRecipientEligibility } from "./support-recipient";
import { nativeSupportRuntime } from "./command-runtime";
import { createSupportEvidenceGuard, createSupportEvidence, createBindSupportEvidence, createRevokeSupportEvidence } from "../application/support-evidence";
const eligible = supportRecipientEligibility();
function nativeGuard(a: WorkContext, supplied?: SqlDatabase) { const runtime = nativeSupportRuntime(a, supplied); return createSupportEvidenceGuard({ ...runtime, ticket: getSupportTicket, eligible: async (a, id) => { const db = await runtime.database(); return Boolean(await db.prepare(`SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligible.replaceAll('t.owner_id', '?::text')}`).bind(id, a.userId).first()); }, guard: async (a, id, expectedVersion, key, manager) => { const db = await runtime.database(); return { native: [assertion(db, key, 'support-evidence', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${manager ? eligible.replaceAll('t.owner_id', '?::text') : eligible + ' AND t.owner_id=?'})`, [id, a.companyId, expectedVersion, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId])] }; } }); }
export async function supportEvidenceGuard(db: SqlDatabase, a: WorkContext, id: string, expectedVersion: number, key: string) { const { row, mutation } = await nativeGuard(a, db)(a, id, expectedVersion, key); return { row, guards: mutation.native as SqlStatement[] }; }
export function supportEvidenceEvent(db: SqlDatabase, id: string, key: string, actor: string, action: string, evidence: Record<string, unknown>, reason: string) {
    return db.prepare('INSERT INTO support_ticket_events SELECT ?,id,version,?,?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?').bind(`support-evidence-${key}`, action, actor, reason, JSON.stringify(evidence), id);
}
export async function supportEvidence(a: WorkContext, id: string) {
    const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.evidence");
    const data = await createSupportEvidence({ ticket: getSupportTicket, readOnly: attachmentsV2ReadOnly, items: async (id) => {
            const write = await runtime.database();
            return (await write.prepare(`SELECT a.id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.created_at,a.created_by,a.revoked_at,a.legal_hold
   FROM (${filesProjectionSources.attachments}) a WHERE a.owner_type='support_ticket' AND a.owner_id=? AND (a.company_id IS NULL OR EXISTS(
     SELECT 1 FROM (${filesProjectionSources.attachment_links}) l JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
     WHERE l.attachment_id=a.id AND l.state='bound' AND s.company_id=a.company_id AND s.target_type='support_ticket' AND s.target_id=a.owner_id))
   ORDER BY a.created_at,a.id`).bind(id).all<Row>()).results;
        }, eligible: async (a, id) => { const write = await runtime.database(); return Boolean(await write.prepare(`SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligible.replaceAll('t.owner_id', '?::text')}`).bind(id, a.userId).first()); } })(a, id); return { ...data, canChange: data.canChange && access.settleExisting, canShare: data.canShare && access.settleExisting };
}
export async function bindSupportEvidence(a: WorkContext, id: string, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createBindSupportEvidence({ ...runtime, readOnly: attachmentsV2ReadOnly, ticket: getSupportTicket, binding: async (a, id, input, key, contract) => {
            const write = await runtime.database(), prepared = await prepareAttachmentBinding(write, a, {
                attachmentSetIds: input.attachmentSetIds, bindingContract: contract, targetId: id,
                expectedTargetVersion: input.expectedVersion, commandId: key,
                bindingId: `support-evidence:${key}`,
            });
            return { native: prepared, manifest: prepared.manifest };
        }, prepare: async (command, input, binding) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command, prepared = binding.native as PreparedAttachmentBinding;
            return { native: [
                    ...prepared.guards,
                    write.prepare('UPDATE support_tickets SET version=version+1,updated_at=? WHERE id=? AND version=?').bind(now, id, input.expectedVersion),
                    ...prepared.steps,
                    supportEvidenceEvent(write, id, key, a.userId, 'attachment-bind', { manifest: prepared.manifest }, input.reason),
                ] };
        } })(a, id, raw, key);
}
export async function revokeSupportEvidence(a: WorkContext, id: string, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createRevokeSupportEvidence({ ...runtime, readOnly: attachmentsV2ReadOnly, guard: async (a, id, version, key) => nativeGuard(a, await runtime.database())(a, id, version, key), prepare: async (command, input, guard) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command, guards = guard.native as SqlStatement[];
            return { native: [
                    ...guards, assertion(write, key, 'attachment', `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE id=? AND owner_type='support_ticket' AND owner_id=? AND revoked_at IS NULL AND legal_hold=0)`, [input.attachmentId, id]),
                    supportAttachmentEffects(write).revoke(input.attachmentId,now),
                    write.prepare('UPDATE support_tickets SET version=version+1,updated_at=? WHERE id=?').bind(now, id),
                    supportEvidenceEvent(write, id, key, a.userId, 'attachment-revoke', input, input.reason),
                ] };
        } })(a, id, raw, key);
}
