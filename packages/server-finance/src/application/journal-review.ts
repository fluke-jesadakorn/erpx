import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { FinanceRow } from '@erpx/server-finance/domain/finance';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export type JournalPostPlan = {
    journalId: string;
    debit: number;
    now: number;
    opening: boolean;
};
/**
 * Independent-review facts for a manual journal post. The post/reverse commands
 * themselves are native Finance owner commands; this application helper only
 * prepares the reviewed plan that the batch command embeds.
 */
export interface JournalReviewPorts {
    error: FinanceError;
    now(): number;
    debit(journalId: string): Promise<{
        amount: number;
    } | null>;
}
export function createJournalReview(ports: JournalReviewPorts) {
    async function prepareJournalPost(actor: WorkContext, journal: FinanceRow) {
        const journalId = String(journal.id);
        if (samePerson(actor, journal.created_by) || !hasPermission(actor, "finance.approve"))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different finance approver must post a manual journal.");
        const debit = await ports.debit(journalId);
        return { journalId, debit: Number(debit?.amount ?? 0), now: ports.now(), opening: journal.source_type === 'opening_import' };
    }
    return { prepareJournalPost };
}
