import "server-only";
import { assertion } from "@erpx/workflow-server/public/command-assertions";
import { readSourceRecord, visibleSourceRecord, sourceRecordGuards } from "@erpx/records-server/public/source-record-policy";
import { nativeSupportReadAll, SUPPORT_RD_EVIDENCE_ITEMS } from "@erpx/support-server/adapters/native-support-read";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { Row } from "@erpx/commerce-server/public/commerce";
import { getSupportTicket } from "@erpx/support-server/adapters/support";
import { attachmentsV2ReadOnly } from "@erpx/files-server/public/attachments/feature";
import { supportRecipientEligibility } from "@erpx/support-server/adapters/support-recipient";
import { nativeSupportRuntime } from "@erpx/support-server/adapters/command-runtime";
import { ticketByIdSql, ticketGuardExistsSql } from './ticket-guard-sources';
import { nativeSupportReadRuntime } from "@erpx/support-server/adapters/read-runtime";
import { createNativeSupportTicketRead } from "@erpx/support-server/public/support-ticket-read";
import { createSupportEvidence, createSupportEvidenceGuard } from "@erpx/support-server/application/support-evidence";
const eligible = supportRecipientEligibility();
const nativeTicketRead = createNativeSupportTicketRead({ read: readSourceRecord, visible: visibleSourceRecord, guards: sourceRecordGuards });
/** Canonical owner-local eligibility witness projection for Support evidence.
 * One fixed text serves the Files-stage guard and the evidence reader so the
 * two call sites cannot drift; predicates and binds stay byte-identical. */
const evidenceEligibilitySelect = () =>
    ticketByIdSql('1 AS ok', eligible.replaceAll('t.owner_id', '?::text'));

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
        eligible: async (a, id) => { const write = await runtime.database(); return Boolean(await write.prepare(evidenceEligibilitySelect()).bind(id, a.userId).first()); },
        guard: async (a, id, expectedVersion, key, manager) => { const write = await runtime.database(); return { native: [assertion(write, key, 'support-evidence', ticketGuardExistsSql(`t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${manager ? eligible.replaceAll('t.owner_id', '?::text') : eligible + ' AND t.owner_id=?'}`), [id, a.companyId, expectedVersion, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId])] }; },
    });
    return guard(a, id, expectedVersion, key);
}

export async function supportEvidence(a: WorkContext, id: string) {
    const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.evidence");
    const data = await createSupportEvidence({ ticket: getSupportTicket, readOnly: attachmentsV2ReadOnly, items: async (id) => {
            const write = await runtime.database();
            return (await nativeSupportReadAll<Row>(SUPPORT_RD_EVIDENCE_ITEMS, [id], write)).results;
        }, eligible: async (a, id) => { const write = await runtime.database(); return Boolean(await write.prepare(evidenceEligibilitySelect()).bind(id, a.userId).first()); } })(a, id); return { ...data, canChange: data.canChange && access.settleExisting, canShare: data.canShare && access.settleExisting };
}
