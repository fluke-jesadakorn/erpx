import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { FinanceError } from "../domain/invoice-line-revision";
export type OwnedJournalSource = "book_adjustment" | "business_recognition" | "instrument_payment" | "asset_policy_depreciation";
export type OwnedReversalInput = {
    periodId: string;
    entryDate: string;
    reason: string;
};
export function createOwnedJournalReversal(ports: {
    source(actor: WorkContext, journalId: string, sourceType: OwnedJournalSource, sourceId: string): Promise<{
        version: number;
        entry_date: string;
    } | null>;
    now(): number;
    error: FinanceError;
}) {
    return async (actor: WorkContext, journalId: string, sourceType: OwnedJournalSource, sourceId: string, input: OwnedReversalInput, key: string) => {
        const source = await ports.source(actor, journalId, sourceType, sourceId);
        if (!source)
            throw ports.error(409, "SOURCE_JOURNAL_CHANGED", "The originating journal is unavailable for reversal.");
        if (input.entryDate < source.entry_date)
            throw ports.error(422, "REVERSAL_BEFORE_SOURCE", "Reverse on or after the original accounting date.");
        return { id: `owned-reversal-${key}`, now: ports.now(), source, journalSourceType: sourceType === "book_adjustment" ? "book_adjustment" : "reversal" };
    };
}
