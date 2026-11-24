import { idCompanyStatusVersionGuard } from './finance-read-tail-r20-sql';
import { OPEN_PERIOD_GUARD_SQL, OPEN_PERIOD_IDS_SQL } from './finance-read-tail-r16-sql';
import { createOwnedJournalReversal } from '@erpx/server-finance/application/owned-reversal';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { financeOwnedReversal } from '@erpx/server-collaboration/adapters/native-finance-owned-reversal';
/** Owner supplies the journal reference; never exposed as a public journal command. */
export async function prepareOwnedJournalReversal(db: SqlDatabase, actor: WorkContext, journalId: string, sourceType: "book_adjustment" | "business_recognition" | "instrument_payment" | "asset_policy_depreciation", sourceId: string, input: {
    periodId: string;
    entryDate: string;
    reason: string;
}, key: string) {
    const { id, now, source, journalSourceType } = await createOwnedJournalReversal({ error: (...args) => new ApiError(...args), now: Date.now, source: (actor, journalId, sourceType, sourceId) => db.prepare("SELECT version,entry_date FROM finance_journals WHERE id=? AND company_id=? AND source_type=? AND source_id=? AND status='posted'").bind(journalId, actor.companyId, sourceType, sourceId).first<{
            version: number;
            entry_date: string;
        }>() })(actor, journalId, sourceType, sourceId, input, key);
    return {
        id,
        steps: [
            assertion(db, key, "source-journal", idCompanyStatusVersionGuard("finance_journals", "posted"), [journalId, actor.companyId, source.version]),
            assertion(db, key, "reversal-period", OPEN_PERIOD_GUARD_SQL, [input.periodId, actor.companyId, input.entryDate, input.entryDate]),          await financeOwnedReversal(db, actor.companyId).reversalJournal({ id, companyId: actor.companyId, periodId: input.periodId, journalNumber: `REV-${key}`, entryDate: input.entryDate, sourceType: journalSourceType, reason: input.reason, userId: actor.userId, now, sourceJournalId: journalId }),          await financeOwnedReversal(db, actor.companyId).reversalLines({ id, journalId: id, sourceJournalId: journalId }),          await financeOwnedReversal(db, actor.companyId).reverseSource({ userId: actor.userId, now, sourceJournalId: journalId }),
        ],
    };
}
