import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import 'server-only';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {prepareJournal} from '@erpx/server-finance/public/finance';
import { ApiError } from "@erpx/server-runtime/public/http";
import { createCommerceReturnsApplication, type CommerceReturnsUnit } from "@erpx/server-commerce/application/commerce-return-accounting";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { attachmentsV2Readable } from "@erpx/server-files/public/attachments/feature";
import type { Posting } from "@erpx/server-commerce/application/commerce-return-accounting";
import { CommerceReturnsFinanceFacts } from "@erpx/server-finance/public/commerce-return-accounting-facts";
function unit(db: SqlDatabase): CommerceReturnsUnit<SqlStatement> {
    return {
        readFinanceJournalLines1: facts => CommerceReturnsFinanceFacts(db).readFinanceJournalLines1(facts),
        readFinanceJournalLines2: facts => CommerceReturnsFinanceFacts(db).readFinanceJournalLines2(facts),
        readFinanceJournalLines3: facts => CommerceReturnsFinanceFacts(db).readFinanceJournalLines3(facts),
        readCommerceReturns1: (facts: {
            id: string;
            returnId: string;
        }) => db.prepare(`SELECT l.account_id,SUM(l.credit_minor-l.debit_minor) transaction_amount,
   SUM(l.base_credit_minor-l.base_debit_minor) base_amount FROM commerce_returns r JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.journal_id=r.credit_journal_id
   WHERE r.invoice_id=? AND r.id<>? AND r.status IN ('approved','posted') GROUP BY l.account_id`).bind(facts.id, facts.returnId).all<{
            account_id: string;
            transaction_amount: number;
            base_amount: number;
        }>(),
        prepareFinanceInvoices1: facts => CommerceReturnsFinanceFacts(db).prepareFinanceInvoices1(facts),
        prepareFinanceJournalLines1: facts => CommerceReturnsFinanceFacts(db).prepareFinanceJournalLines1(facts),
        guardRule1: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
        }) => assertion(db, facts.key, facts.guardId, `COALESCE((SELECT SUM(r.total_minor) FROM commerce_returns r WHERE r.invoice_id=? AND r.id<>? AND r.status IN ('approved','posted')),0)=?`, facts.values),
        guardRule2: facts => CommerceReturnsFinanceFacts(db).guardRule2(facts),
        prepareJournal: (arg0: WorkContext, arg1: {
            chartId: string;
            periodId: string;
            entryDate: string;
            currency: string;
            fxRate: string;
            memo: string;
            sourceType: string;
            sourceId: string;
            isOpening: false;
            lines: {
                accountId: string;
                debit: string;
                credit: string;
            }[];
        }, arg2: string, arg3: "draft") => prepareJournal(db, arg0, arg1, arg2, arg3)
    };
}
const application = createCommerceReturnsApplication<SqlStatement>({ clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), hash: sha256, forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write), forContext: async () => { throw new Error("No registry context for this service"); }, attachmentReadable: attachmentsV2Readable, returnCompletionValues });
export function prepareReturnCredit(write: SqlDatabase, a: WorkContext, input: {
    key: string;
    invoiceId: string;
    returnId: string;
    periodId: string;
    entryDate: string;
    postings: Posting[];
}) { return application.prepareReturnCredit(unit(write), a, input); }
export function returnCompletionValues(input: {
    totalMinor: number;
    baseCreditMinor: number;
    outstandingMinor: number;
    baseOutstandingMinor: number;
}): {
    creditedMinor: number;
    refundMinor: number;
    baseCreditedMinor: number;
    baseRefundMinor: number;
    realizedFxMinor: number;
} { return application.returnCompletionValues(input); }
