"use client";

import { workFetch } from "@/lib/work-fetch";
import Link from "next/link";
import { UploadCloud, Sparkles } from "lucide-react";
import { createReceiptUploader, validateReceipt } from "@/lib/expense-upload";
import { ExpenseAiPanel } from "./expense-ai-panel";
import { createExpenseDraftOnce } from "@/lib/expense-draft-create";
import styles from "./expense.module.css";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useHydrated } from "@/components/ui/use-hydrated";
import { minorToDecimal } from "@/lib/finance-money";
import type { ExpenseRow as Row } from "@/server/expenses";
import { useExpenseDraft } from "./use-expense-draft";
import type { ExpenseAiSuggestionEvent } from "@/lib/expense-ai";

type Fields = {
  personId: string;
  requestKind: string;
  payeePartyId: string;
  expenseDate: string;
  merchant: string;
  receiptReference: string;
  businessPurpose: string;
  categoryAccountId: string;
  currency: string;
  amount: string;
  tax: string;
};
export function ExpenseEditor({
  base,
  options,
  expense,
  initialOpen = false,
  receiptAi,
}: {
  base: string;
  options: {
    people: Row[];
    payees?:Row[];
    accounts: Row[];
    periods: Row[];
    selfUserId?: string;
  };
  expense?: Row;
  initialOpen?: boolean;
  receiptAi?: { canManageAi: boolean; evidenceKey: string };
}) {
  const router = useRouter(),
    hydrated = useHydrated(),
    attempt = useRef<{ body: string; key: string } | null>(null);
  const initial: Fields = {
    personId: String(expense?.employee_id ?? options.people.find(p => p.user_id === options.selfUserId)?.id ?? ""),
    payeePartyId: String(expense?.payee_party_id??""),
    requestKind: String(expense?.request_kind ?? "expense"),
    expenseDate: String(expense?.expense_date ?? ""),
    merchant: String(expense?.merchant ?? ""),
    receiptReference: String(expense?.receipt_reference ?? ""),
    businessPurpose: String(expense?.business_purpose ?? ""),
    categoryAccountId: String(expense?.category_account_id ?? ""),
    currency: String(expense?.currency ?? ""),
    amount: expense?.amount_minor
      ? minorToDecimal(Number(expense.amount_minor))
      : "",
    tax: expense?.tax_minor ? minorToDecimal(Number(expense.tax_minor)) : "",
  };
  const [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const [uploading, setUploading] = useState(false), [progress, setProgress] = useState(""), [createdId, setCreatedId] = useState("");
  const created = useRef<string | null>(null), creating = useRef(false), upload = useRef(createReceiptUploader()), retryFiles = useRef<File[]>([]);
  const createDraft = useRef(createExpenseDraftOnce());
  const [creationStarted, setCreationStarted] = useState(false);
  const storageKey = `erpx:expense:${base}:${options.selfUserId}:${expense?.id ?? "new"}`;
  const draft = useExpenseDraft(storageKey, initial),
    { fields, update: setFields } = draft;
  useEffect(() => {
    if (expense) {
      const newKey = `erpx:expense:${base}:${options.selfUserId}:new:created`;
      try { if (sessionStorage.getItem(newKey) === String(expense.id)) { sessionStorage.removeItem(newKey); sessionStorage.removeItem(newKey.slice(0, -8)); } } catch {}
      return;
    }
    try {
      const id = sessionStorage.getItem(`${storageKey}:created`);
      if (id) router.replace(`${base}/expenses/${encodeURIComponent(id)}`);
      else if (sessionStorage.getItem(`${storageKey}:creation`)) {
        creating.current = true;
        // Restore the pending command lock before allowing another draft action.
        // eslint-disable-next-line react-hooks/set-state-in-effect
        setCreationStarted(true);
        void createDraft.current(base, storageKey, {}).then(id => {
          created.current = id; setCreatedId(id);
          router.replace(`${base}/expenses/${encodeURIComponent(id)}`);
        }).catch(e => {
          try { setCreationStarted(!!sessionStorage.getItem(`${storageKey}:creation`)); } catch {}
          setError(e instanceof Error ? e.message : "Unable to recover your draft. Retry when connected.");
        }).finally(() => { creating.current = false; });
      }
    } catch {}
  }, [expense, storageKey, router, base, options.selfUserId]);
  useEffect(() => {
    const receive = (event: Event) => {
      const detail = (event as CustomEvent<ExpenseAiSuggestionEvent>).detail;
      if (detail.expenseId !== expense?.id || detail.paymentId) return;
      draft.receiveAi(detail);
    };
    window.addEventListener("erpx:expense-ai", receive);
    return () => window.removeEventListener("erpx:expense-ai", receive);
  });
  function change(name: keyof Fields, value: string) {
    setFields((previous) => {
      const next = { ...previous, [name]: value };
      if (name === "categoryAccountId") {
        const account = options.accounts.find((a) => a.id === value);
        if (account) next.currency = String(account.currency);
      }
      return next;
    }, name === "categoryAccountId" ? [name, "currency"] : [name]);
  }
  function aiSource(name: keyof Fields) {
    const field = draft.ai[name];
    if (!field) return null;
    return <span className={styles.aiSource}><Sparkles size={12} /> AI · {field.citations?.map(c => <a key={`${c.attachmentId}:${c.page}`} href={`/api/v1${base}/attachments/${c.attachmentId}/content?page=${c.page}`} target="_blank" rel="noreferrer">Receipt p.{c.page}</a>)}</span>;
  }
  async function receiptFirst(files: File[]) {
    if (creating.current || pending || !hydrated || (!files.length && !creationStarted) || draft.conflict) return;
    creating.current = true; setUploading(true); setError(""); retryFiles.current = files;
    try {
      files.forEach(file => validateReceipt(file));
      setCreationStarted(true);
      if (!created.current) {
        setProgress("Creating your draft…");
        created.current = await createDraft.current(base, storageKey, fields, draft.touched); setCreatedId(created.current);
      }
      for (const file of files) await upload.current(base, created.current, file, false, setProgress);
      draft.clear();
      router.push(`${base}/expenses/${created.current}`); router.refresh();
    } catch (e) {
      if (!created.current) { try { setCreationStarted(!!sessionStorage.getItem(`${storageKey}:creation`)); } catch {} }
      setError(e instanceof Error ? e.message : "Unable to upload receipt. Your draft is safe.");
    }
    finally { creating.current = false; setUploading(false); setProgress(""); }
  }
  async function save() {
    if (draft.conflict || creating.current || creationStarted) return;
    if (created.current) { router.push(`${base}/expenses/${created.current}`); return; }
    const body = JSON.stringify({
      ...fields,
      ...(expense ? { expectedVersion: Number(expense.version) } : {}),
    });
    if (attempt.current?.body !== body)
      attempt.current = { body, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const response = await workFetch(
        `/api/v1${base}/expenses${expense ? `/${expense.id}` : ""}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": attempt.current.key,
          },
          body,
        },
      );
      const result = (await response.json()) as {
        data: { id: string };
        error?: { message: string };
      };
      if (!response.ok)
        throw Error(result.error?.message ?? "Unable to save this draft.");
      attempt.current = null;
      draft.clear();
      if (!expense) router.push(`${base}/expenses/${result.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save this draft.");
    } finally {
      setPending(false);
    }
  }
  return (
    <details className={`card ${styles.editor}`} open={!!expense || initialOpen}>
      <summary>{expense ? "Edit claim" : "Create expense claim"}</summary>
      <p>
        Save a draft at any time. Attach your receipt, then review the details
        before submitting.
      </p>
      {expense && receiptAi && <ExpenseAiPanel compact base={base} expenseId={String(expense.id)} version={Number(expense.version)} {...receiptAi} />}
      {!expense && <div className={styles.dropzone} onDragOver={e => e.preventDefault()} onDrop={e => { e.preventDefault(); if (hydrated && !pending && !uploading && !createdId) void receiptFirst(Array.from(e.dataTransfer.files)); }}>
        <UploadCloud size={28} /><strong>Start with your receipt</strong><p>Drop receipts here, then review the claim. AI suggestions are optional when available.</p>
        <label className="button secondary">Choose receipts<input aria-label="Choose receipts" type="file" multiple accept="image/jpeg,image/png,image/webp,application/pdf" disabled={!hydrated || pending || uploading || !!createdId || draft.conflict} onChange={e => { const files = Array.from(e.target.files ?? []); void receiptFirst(files); e.target.value = ""; }} /></label>
        <span>JPG, PNG, WebP or PDF · Up to 10 MB each · A draft is created automatically</span>
      </div>}
      {progress && <p role="status">{progress}</p>}
      {createdId && error && <div className="notice"><p>Your draft is saved. Retry the upload or continue from the saved claim.</p><button type="button" className="button secondary" disabled={uploading} onClick={() => void receiptFirst(retryFiles.current)}>Retry receipt upload</button> <Link href={`${base}/expenses/${createdId}`}>Open saved draft</Link></div>}
      {!createdId && creationStarted && error && <div className="notice"><p>Your draft request is saved in this browser. Retry to recover the same draft.</p><button type="button" className="button secondary" disabled={uploading} onClick={() => void receiptFirst(retryFiles.current)}>Recover draft and retry</button></div>}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {draft.notice}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <fieldset disabled={!hydrated || pending || uploading || creationStarted || !!createdId} className={styles.fields}>
          <div className="business-form">
            <label>
              Claimant
              <select
                value={fields.personId}
                disabled={!!expense || !hydrated}
                onChange={(e) => change("personId", e.target.value)}
              >
                <option value="">Myself</option>
                {options.people.map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.legal_name)}
                  </option>
                ))}
              </select>
            </label>
            <label>Request type<select value={fields.requestKind} onChange={e=>change("requestKind",e.target.value)}><option value="expense">Expense claim</option><option value="payment">Payment request</option></select></label>{fields.requestKind==="payment"&&<label>Payee<select value={fields.payeePartyId} onChange={e=>change("payeePartyId",e.target.value)}><option value="">Requester (employee payment)</option>{options.payees?.filter(p=>p.currency===fields.currency).map(p=><option key={String(p.id)} value={String(p.id)}>{String(p.name)}</option>)}</select></label>}
              <label>
              Expense date
              <input
                type="date"
                value={fields.expenseDate}
                onChange={(e) => change("expenseDate", e.target.value)}
              />
              {aiSource("expenseDate")}
            </label>
            <label>
              Merchant
              <input
                aria-label="Merchant"
                maxLength={200}
                value={fields.merchant}
                onChange={(e) => change("merchant", e.target.value)}
              />
              {aiSource("merchant")}
            </label>
            <label>
              Receipt / supporting document number
                <input
                aria-label="Receipt number"
                maxLength={100}
                value={fields.receiptReference}
                onChange={(e) => change("receiptReference", e.target.value)}
              />
              {aiSource("receiptReference")}
            </label>
            <label>
              Expense category
              <select
                value={fields.categoryAccountId}
                onChange={(e) => change("categoryAccountId", e.target.value)}
              >
                <option value="">Choose later</option>
                {options.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)} · {String(a.currency)}
                  </option>
                ))}
              </select>
              {aiSource("categoryAccountId")}
            </label>
            <label>
              Currency
              <input
                maxLength={3}
                value={fields.currency}
                onChange={(e) =>
                  change("currency", e.target.value.toUpperCase())
                }
              />
              {aiSource("currency")}
            </label>
            <label>
              Net amount
              <input
                type="number"
                step="0.01"
                min="0.01"
                value={fields.amount}
                onChange={(e) => change("amount", e.target.value)}
              />
              {aiSource("amount")}
            </label>
            <label>
              Recoverable tax
              <input
                type="number"
                step="0.01"
                min="0"
                value={fields.tax}
                onChange={(e) => change("tax", e.target.value)}
              />
              {aiSource("tax")}
            </label>
          </div>
          <label>
            Business purpose
            <textarea
              maxLength={2000}
              value={fields.businessPurpose}
              onChange={(e) => change("businessPurpose", e.target.value)}
            />
              {aiSource("businessPurpose")}
          </label>
          <p className="muted">
            Include non-recoverable tax in the net amount.
          </p>
          <button
            className="button"
            disabled={pending || !hydrated || draft.conflict}
          >
            {pending ? "Saving…" : "Save draft"}
          </button>
        </fieldset>
      </form>
    </details>
  );
}
