import "server-only";
import { registeredDatabases, type RegistryEnv } from "@/lib/holding-registry";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { StripeObject } from "@/server/modules/billing/public/billing-stripe";
import { createBillingTestFinanceApplication, type BillingTestFinanceUnit } from "../application/billing-test-finance";
import type { SqlDatabase, SqlStatement } from "../../runtime/public/runtime/types";
import { holdingResources } from "../../runtime/public/runtime";
import { requirePermission } from "../../identity_access/public/iam-permissions";
import { ApiError } from "../../runtime/public/http";
import { sha256 } from "../../runtime/public/crypto";
import { returnCompletionValues } from "../public/commerce-return-accounting";
import { attachmentsV2Readable } from "../../files/public/attachments/feature";
import type { LocalInvoice, Account } from "../application/billing-test-finance";
function unit(db: SqlDatabase): BillingTestFinanceUnit<SqlStatement> {
    return {
        readBillingTestFinanceDocuments1: (facts: {
            stripe_id: string;
        }) => db.prepare(`SELECT source_billing_invoice_id,customer_holding_id,total_satang
      FROM billing_test_finance_documents WHERE stripe_invoice_id=? FOR UPDATE`).bind(facts.stripe_id).first<{
            source_billing_invoice_id: string;
            customer_holding_id: string;
            total_satang: number;
        }>(),
        writeBillingTestFinanceDocuments1: (facts: {
            stripe_id: string;
            id: string;
            holding_id: string;
            TEST_COMPANY: "erpx_billing_sandbox";
            total_satang: number;
            status: string;
            now: number;
            now2: number;
        }) => db.prepare(`INSERT INTO billing_test_finance_documents
      (stripe_invoice_id,source_billing_invoice_id,customer_holding_id,company_id,currency,
      total_satang,stripe_status,observed_at,created_at) VALUES(?,?,?,?,'THB',?,?,?,?)`).bind(facts.stripe_id, facts.id, facts.holding_id, facts.TEST_COMPANY, facts.total_satang, facts.status, facts.now, facts.now2).run(),
        writeBillingTestFinanceDocuments2: (facts: {
            status: string;
            now: number;
            stripe_id: string;
        }) => db.prepare(`UPDATE billing_test_finance_documents
      SET stripe_status=?,observed_at=? WHERE stripe_invoice_id=?`).bind(facts.status, facts.now, facts.stripe_id).run(),
        readBillingTestFinanceEntries1: (facts: {
            key: string;
        }) => db.prepare(`SELECT entry_kind,amount_satang FROM billing_test_finance_entries
        WHERE source_key=?`).bind(facts.key).first<{
            entry_kind: string;
            amount_satang: number;
        }>(),
        writeBillingTestFinanceEntries1: (facts: {
            key: string;
            stripe_id: string;
            kind: "void" | "issued" | "receipt" | "credit" | "credit_reversal";
            amount: number;
            now: number;
            now2: number;
        }) => db.prepare(`INSERT INTO billing_test_finance_entries
        (source_key,stripe_invoice_id,entry_kind,amount_satang,effective_at,created_at)
        VALUES(?,?,?,?,?,?)`).bind(facts.key, facts.stripe_id, facts.kind, facts.amount, facts.now, facts.now2).run(),
        writeBillingTestFinanceLines1: (facts: {
            key: string;
            fact1: 0;
            debit: Account;
            amount: number;
            key2: string;
            fact5: 1;
            credit: Account;
            amount2: number;
        }) => db.prepare(`INSERT INTO billing_test_finance_lines
        (source_key,ordinal,account_code,debit_satang,credit_satang)
        VALUES(?,?,?, ?,0),(?,?,?,0,?)`).bind(facts.key, facts.fact1, facts.debit, facts.amount, facts.key2, facts.fact5, facts.credit, facts.amount2).run(),
        readBillingTestFinanceEntries2: (facts: {
            issuedKey: string;
        }) => db.prepare(`SELECT 1 FROM billing_test_finance_entries WHERE source_key=?`).bind(facts.issuedKey).first(),
        readBillingTestFinanceEntries3: (facts: {
            fact0: string;
        }) => db.prepare("SELECT 1 FROM billing_test_finance_entries WHERE source_key=?").bind(facts.fact0).first(),
        readBillingTestFinanceEntries4: (facts: {
            fact0: string;
        }) => db.prepare("SELECT 1 FROM billing_test_finance_entries WHERE source_key=?").bind(facts.fact0).first(),
        readBillingTestFinanceEntries5: (facts: {
            stripe_id: string;
        }) => db.prepare(`SELECT
      COALESCE(SUM(CASE WHEN entry_kind='issued' THEN amount_satang ELSE 0 END),0)::bigint issued,
      COALESCE(SUM(CASE WHEN entry_kind='receipt' THEN amount_satang ELSE 0 END),0)::bigint received,
      COALESCE(SUM(CASE WHEN entry_kind='void' THEN amount_satang ELSE 0 END),0)::bigint reversed,
      COALESCE(SUM(CASE WHEN entry_kind='credit' THEN amount_satang WHEN entry_kind='credit_reversal' THEN -amount_satang ELSE 0 END),0)::bigint credited
      FROM billing_test_finance_entries WHERE stripe_invoice_id=?`).bind(facts.stripe_id).first<{
            issued: number;
            received: number;
            reversed: number;
            credited: number;
        }>(),
        readBillingTestFinanceLines1: (facts: {
            stripe_id: string;
        }) => db.prepare(`SELECT COALESCE(SUM(l.debit_satang-l.credit_satang),0)::bigint balance
      FROM billing_test_finance_lines l JOIN billing_test_finance_entries e ON e.source_key=l.source_key
      WHERE e.stripe_invoice_id=? AND l.account_code='accounts_receivable'`).bind(facts.stripe_id).first<{
            balance: number;
        }>(), transaction: work => {
            if (!isTransactionalSqlDatabase(db))
                throw new Error("TEST_FINANCE_TRANSACTION_REQUIRED");
            return db.transaction(tx => work(unit(tx)));
        }
    };
}
const application = createBillingTestFinanceApplication<SqlStatement, RegistryEnv>({ clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), hash: sha256, forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write), forContext: async (env) => unit(registeredDatabases(env, "h003").write), attachmentReadable: attachmentsV2Readable, returnCompletionValues });
export function syncBillingTestFinance(env: RegistryEnv, local: LocalInvoice, stripeInvoice: StripeObject, creditNotes: StripeObject[], now = Date.now()) { return application.syncBillingTestFinance(env, local, stripeInvoice, creditNotes, now); }
