import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/commercial-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {prepareJournal} from '@erpx/finance-server/public/journal-planning';
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { createCommerceGoodsApplication, type CommerceGoodsUnit, type Row } from "@erpx/commerce-server/application/commerce-goods-accounting";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { returnCompletionValues } from "@erpx/commerce-server/public/commerce-return-accounting";
import { attachmentsV2Readable } from "@erpx/files-server/public/attachments/feature";
import { CommerceGoodsFinanceFacts } from "@erpx/finance-server/public/commerce-goods-accounting-facts";
function unit(db: SqlDatabase): CommerceGoodsUnit<SqlStatement> {
    return {
        readFinanceAccounts1: facts => CommerceGoodsFinanceFacts(db).readFinanceAccounts1(facts),
        readFinanceAccounts2: facts => CommerceGoodsFinanceFacts(db).readFinanceAccounts2(facts),
        readFinanceJournalLines1: facts => CommerceGoodsFinanceFacts(db).readFinanceJournalLines1(facts),
        prepareCommerceFulfillments1: (facts: {
            id: string | number | null;
        }) => db.prepare("SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE").bind(facts.id),
        // Write authority moved to the native Commerce axum command route.
        prepareCommerceInvoiceGoodsCarrying1: (_facts: {
            id: string | number | null; companyId: string; chartId: string; currency: string;
            debit_account_id: string | number | null; fxRateMicros: number; fact6: string;
            fact7: string | null; fact8: string | null; fact9: boolean; fact10: string;
            sourceJson: string; receiptBase: number; invoiceBase: number; difference: number;
            fact15: string | null; fact16: string | null; fact17: string | null;
            userId: string; fact19: number;
        }) => { throw new Error('COMMERCE_OWNER_WRITE_UNAVAILABLE: native goods-carrying write required'); },
        readCommerceInvoiceGoodsCarrying1: (facts: {
            invoiceId: string;
            companyId: string;
        }) => db.prepare("SELECT * FROM commerce_invoice_goods_carrying WHERE invoice_id=? AND company_id=?").bind(facts.invoiceId, facts.companyId).first<Row>(),
        guardRule1: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 FROM commerce_fulfillments f
      JOIN commerce_fulfillment_lines l ON l.fulfillment_id=f.id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=f.journal_id
      WHERE f.id=? AND f.invoice_id=? AND f.version=? AND j.id=? AND j.version=? AND j.status='posted'
      AND l.id=? AND l.quantity_milli=? AND l.value_minor=? AND l.subtotal_minor=? AND l.tax_minor=?
      AND l.stock_currency IS NOT DISTINCT FROM ?::text)`, facts.values),
        guardRule2: facts => CommerceGoodsFinanceFacts(db).guardRule2(facts),
        prepareJournal: (arg0: WorkContext, arg1: {
            chartId: string;
            periodId: string;
            entryDate: string;
            currency: string;
            isOpening: false;
            sourceType: string;
            sourceId: string;
            memo: string;
            lines: {
                accountId: string;
                debit: string;
                credit: string;
            }[];
        } | {
            chartId: string;
            periodId: string;
            entryDate: string;
            currency: string;
            sourceType: string;
            sourceId: string;
            memo: string;
            isOpening: false;
            lines: {
                accountId: string;
                debit: string;
                credit: string;
            }[];
        }, arg2: string, arg3: "posted" | "draft") => prepareJournal(db, arg0, arg1, arg2, arg3)
    };
}
const application = createCommerceGoodsApplication<SqlStatement>({ clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), hash: sha256, forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write), forContext: async () => { throw new Error("No registry context for this service"); }, attachmentReadable: attachmentsV2Readable, returnCompletionValues });
export function prepareCommerceInvoiceGoods(db: SqlDatabase, actor: WorkContext, input: {
    invoice: Row;
    journal: Awaited<ReturnType<typeof prepareJournal>>;
    key: string;
    periodId: string;
    entryDate: string;
    gainAccountId?: string;
    lossAccountId?: string;
}) { return application.prepareCommerceInvoiceGoods(unit(db), actor, input); }
export function prepareReturnGoodsFx(db: SqlDatabase, actor: WorkContext, input: {
    invoiceId: string;
    returnId: string;
    key: string;
    periodId: string;
    entryDate: string;
    stockBase: number;
    clearingAccountId: string;
    creditPostings: Array<{
        accountId: string;
        baseDebit: number;
        baseCredit: number;
    }>;
}) { return application.prepareReturnGoodsFx(unit(db), actor, input); }
