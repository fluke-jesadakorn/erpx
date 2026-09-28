import "server-only";

import { registeredDatabases, type RegistryEnv } from "@/lib/holding-registry";
import { isTransactionalSqlDatabase } from "./runtime/types";
import type { StripeObject } from "./billing-stripe";

const TEST_COMPANY = "erpx_billing_sandbox";
type Account = "accounts_receivable" | "revenue" | "sandbox_cash" | "customer_credit";
type LocalInvoice = {
  id: string; holding_id: string; stripe_id: string; total_satang: number;
};
type Credit = { id: string; pre: number; post: number; status: "issued" | "void" };

function satang(value: unknown, field: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
    throw new Error(`INVALID_STRIPE_${field}`);
  return value;
}

/** A test Company ledger physically owned by ERPX Holding h003, separate from Finance reporting. */
export async function syncBillingTestFinance(
  env: RegistryEnv,
  local: LocalInvoice,
  stripeInvoice: StripeObject,
  creditNotes: StripeObject[],
  now = Date.now(),
) {
  if (!local.stripe_id || stripeInvoice.id !== local.stripe_id || stripeInvoice.livemode !== false)
    throw new Error("TEST_FINANCE_STRIPE_SCOPE_MISMATCH");
  if (stripeInvoice.currency !== "thb" || satang(stripeInvoice.total, "TOTAL") !== Number(local.total_satang))
    throw new Error("TEST_FINANCE_AMOUNT_MISMATCH");
  const metadata = stripeInvoice.metadata as Record<string, unknown> | undefined;
  if (metadata?.erpx_invoice_id !== local.id || metadata?.holding_id !== local.holding_id)
    throw new Error("TEST_FINANCE_SOURCE_MISMATCH");
  const status = String(stripeInvoice.status);
  if (!["draft", "open", "paid", "void", "uncollectible"].includes(status))
    throw new Error("TEST_FINANCE_STATUS_INVALID");
  const paid = satang(stripeInvoice.amount_paid ?? 0, "AMOUNT_PAID");
  if (status !== "paid" && paid > 0) throw new Error("PARTIAL_PAYMENT_REQUIRES_RECONCILIATION");
  if (status === "paid" && paid === 0 && Number(local.total_satang) > 0)
    throw new Error("PAID_AMOUNT_MISSING");
  const credits: Credit[] = creditNotes.map(note => {
    if (note.livemode !== false || note.invoice !== local.stripe_id ||
      !["issued","void"].includes(String(note.status)))
      throw new Error("TEST_FINANCE_CREDIT_SCOPE_MISMATCH");
    return { id: note.id, pre: satang(note.pre_payment_amount ?? 0, "PRE_CREDIT"),
      post: satang(note.post_payment_amount ?? 0, "POST_CREDIT"), status: note.status as Credit["status"] };
  });
  const unique = new Set(credits.map(note => note.id));
  if (unique.size !== credits.length) throw new Error("TEST_FINANCE_DUPLICATE_CREDIT");
  const totalCredits = credits.reduce((sum, note) =>
    note.status === "issued" ? sum + note.pre + note.post : sum, 0);
  if (totalCredits > Number(local.total_satang)) throw new Error("TEST_FINANCE_CREDIT_EXCEEDS_INVOICE");
  const activePreCredits = credits.reduce((sum, note) =>
    note.status === "issued" ? sum + note.pre : sum, 0);
  if (status === "paid" && paid + activePreCredits !== Number(local.total_satang))
    throw new Error("TEST_FINANCE_PAYMENT_RECONCILIATION_REQUIRED");

  const write = registeredDatabases(env, "h003").write;
  if (!isTransactionalSqlDatabase(write)) throw new Error("TEST_FINANCE_TRANSACTION_REQUIRED");
  return write.transaction(async tx => {
    const existing = await tx.prepare(`SELECT source_billing_invoice_id,customer_holding_id,total_satang
      FROM billing_test_finance_documents WHERE stripe_invoice_id=? FOR UPDATE`)
      .bind(local.stripe_id).first<{source_billing_invoice_id:string;customer_holding_id:string;total_satang:number}>();
    if (existing && (existing.source_billing_invoice_id !== local.id ||
      existing.customer_holding_id !== local.holding_id || Number(existing.total_satang) !== Number(local.total_satang)))
      throw new Error("TEST_FINANCE_DOCUMENT_CONFLICT");
    if (!existing) await tx.prepare(`INSERT INTO billing_test_finance_documents
      (stripe_invoice_id,source_billing_invoice_id,customer_holding_id,company_id,currency,
      total_satang,stripe_status,observed_at,created_at) VALUES(?,?,?,?,'THB',?,?,?,?)`)
      .bind(local.stripe_id,local.id,local.holding_id,TEST_COMPANY,
        local.total_satang,status,now,now).run();
    else await tx.prepare(`UPDATE billing_test_finance_documents
      SET stripe_status=?,observed_at=? WHERE stripe_invoice_id=?`)
      .bind(status,now,local.stripe_id).run();

    async function entry(key: string, kind: "issued" | "receipt" | "void" | "credit" | "credit_reversal",
      amount: number, debit: Account, credit: Account) {
      if (amount === 0) return;
      const prior = await tx.prepare(`SELECT entry_kind,amount_satang FROM billing_test_finance_entries
        WHERE source_key=?`).bind(key).first<{entry_kind:string;amount_satang:number}>();
      if (prior) {
        if (prior.entry_kind !== kind || Number(prior.amount_satang) !== amount)
          throw new Error("TEST_FINANCE_ENTRY_CONFLICT");
        return;
      }
      await tx.prepare(`INSERT INTO billing_test_finance_entries
        (source_key,stripe_invoice_id,entry_kind,amount_satang,effective_at,created_at)
        VALUES(?,?,?,?,?,?)`).bind(key,local.stripe_id,kind,amount,now,now).run();
      await tx.prepare(`INSERT INTO billing_test_finance_lines
        (source_key,ordinal,account_code,debit_satang,credit_satang)
        VALUES(?,?,?, ?,0),(?,?,?,0,?)`)
        .bind(key,0,debit,amount,key,1,credit,amount).run();
    }

    const issuedKey = `${local.stripe_id}:issued`;
    const priorIssue = await tx.prepare(`SELECT 1 FROM billing_test_finance_entries WHERE source_key=?`)
      .bind(issuedKey).first();
    const issued = status !== "draft" && (status !== "void" || !!priorIssue);
    if (issued) await entry(issuedKey,"issued",Number(local.total_satang),"accounts_receivable","revenue");
    if (issued) for (const note of credits) {
      if (note.status === "issued") {
        await entry(`${note.id}:pre`,"credit",note.pre,"revenue","accounts_receivable");
        await entry(`${note.id}:post`,"credit",note.post,"revenue","customer_credit");
      } else {
        const priorPre = await tx.prepare("SELECT 1 FROM billing_test_finance_entries WHERE source_key=?")
          .bind(`${note.id}:pre`).first();
        const priorPost = await tx.prepare("SELECT 1 FROM billing_test_finance_entries WHERE source_key=?")
          .bind(`${note.id}:post`).first();
        if (priorPre) await entry(`${note.id}:pre:reversal`,"credit_reversal",note.pre,
          "accounts_receivable","revenue");
        if (priorPost) await entry(`${note.id}:post:reversal`,"credit_reversal",note.post,
          "customer_credit","revenue");
      }
    }
    if (status === "paid") await entry(`${local.stripe_id}:receipt`,"receipt",paid,
      "sandbox_cash","accounts_receivable");
    if (status === "void" && issued) {
      const remaining = Number(local.total_satang) - activePreCredits;
      if (remaining < 0) throw new Error("TEST_FINANCE_VOID_EXCEEDS_RECEIVABLE");
      await entry(`${local.stripe_id}:void`,"void",remaining,"revenue","accounts_receivable");
    }
    const result = await tx.prepare(`SELECT
      COALESCE(SUM(CASE WHEN entry_kind='issued' THEN amount_satang ELSE 0 END),0)::bigint issued,
      COALESCE(SUM(CASE WHEN entry_kind='receipt' THEN amount_satang ELSE 0 END),0)::bigint received,
      COALESCE(SUM(CASE WHEN entry_kind='void' THEN amount_satang ELSE 0 END),0)::bigint reversed,
      COALESCE(SUM(CASE WHEN entry_kind='credit' THEN amount_satang WHEN entry_kind='credit_reversal' THEN -amount_satang ELSE 0 END),0)::bigint credited
      FROM billing_test_finance_entries WHERE stripe_invoice_id=?`)
      .bind(local.stripe_id).first<{issued:number;received:number;reversed:number;credited:number}>();
    if (!result) throw new Error("TEST_FINANCE_RESULT_MISSING");
    const ar = await tx.prepare(`SELECT COALESCE(SUM(l.debit_satang-l.credit_satang),0)::bigint balance
      FROM billing_test_finance_lines l JOIN billing_test_finance_entries e ON e.source_key=l.source_key
      WHERE e.stripe_invoice_id=? AND l.account_code='accounts_receivable'`)
      .bind(local.stripe_id).first<{balance:number}>();
    const receivable = Number(ar?.balance ?? 0);
    if (receivable < 0) throw new Error("TEST_FINANCE_NEGATIVE_RECEIVABLE");
    return { companyId: TEST_COMPANY, invoiceSatang: Number(local.total_satang),
      receivableSatang: receivable, receivedSatang: Number(result.received),
      reversedSatang: Number(result.reversed), creditedSatang: Number(result.credited),
      stripeStatus: status };
  });
}
