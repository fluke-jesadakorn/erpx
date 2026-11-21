import "server-only";
import { assertion } from "@erpx/server-workflow/public/command-assertions";
import { readSourceRecord, visibleSourceRecord, sourceRecordGuards } from "@erpx/server-records/public/source-record-policy";
import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/final-root-owner-projection-sources";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Row } from "@erpx/server-commerce/public/commerce";
import { getSupportTicket } from "@erpx/server-support/adapters/support";
import { attachmentsV2ReadOnly } from "@erpx/server-files/public/attachments/feature";
import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { nativeSupportReadRuntime } from "@erpx/server-support/adapters/read-runtime";
import { createNativeSupportTicketRead } from "@erpx/server-support/public/support-ticket-read";
import { createSupportEvidence, createSupportEvidenceGuard } from "@erpx/server-support/application/support-evidence";
const eligible = supportRecipientEligibility();
const nativeTicketRead = createNativeSupportTicketRead({ read: readSourceRecord, visible: visibleSourceRecord, guards: sourceRecordGuards });

/**
 * Read-only Support case policy for Files-side staging. The native Support
 * evidence command remains the only binder/revoker; this guard lets the Files
 * policy reject a ticket whose status, version, ownership or current
 * employment no longer permits evidence changes before any draft is written.
 */
export async function supportEvidenceGuard(db: SqlDatabase, a: WorkContext, id: string, expectedVersion: number, key: string) {
    const runtime = nativeSupportReadRuntime(a, db);
    const guard = createSupportEvidenceGuard({
        ...runtime,
        ticket: nativeTicketRead.getSupportTicket,
        eligible: async (a, id) => { const write = await runtime.database(); return Boolean(await write.prepare(`SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligible.replaceAll('t.owner_id', '?::text')}`).bind(id, a.userId).first()); },
        guard: async (a, id, expectedVersion, key, manager) => { const write = await runtime.database(); return { native: [assertion(write, key, 'support-evidence', `EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${manager ? eligible.replaceAll('t.owner_id', '?::text') : eligible + ' AND t.owner_id=?'})`, [id, a.companyId, expectedVersion, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId])] }; },
    });
    return guard(a, id, expectedVersion, key);
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
