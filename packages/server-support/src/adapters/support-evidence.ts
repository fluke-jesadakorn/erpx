import{createNativeSupportEvidencePlanner}from '@erpx/server-support/public/support-evidence-planning';
import{readSourceRecord,visibleSourceRecord,sourceRecordGuards}from '@erpx/server-records/public/source-record-policy';
const{nativeGuard,supportEvidenceGuard,supportEvidenceEvent}=createNativeSupportEvidencePlanner({read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards});
import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/final-root-owner-projection-sources";
import {supportAttachmentEffects} from "@erpx/server-files/public/support-attachment-effects";
import "server-only";
import type { SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { PreparedAttachmentBinding } from "@erpx/server-files/public/attachments/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { attachmentsV2ReadOnly } from "@erpx/server-files/public/attachments/feature";
import {getSupportTicket} from '@erpx/server-support/adapters/support';
import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { createSupportEvidence, createBindSupportEvidence, createRevokeSupportEvidence } from "@erpx/server-support/application/support-evidence";
const eligible = supportRecipientEligibility();

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

export{supportEvidenceGuard,supportEvidenceEvent};
