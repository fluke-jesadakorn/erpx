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
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@/lib/ai/guided-intake";

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
  hasReceipt = false,
  enableSubmit = true,
}: {
  base: string;
  options: {
    people: Row[];
    payees?:Row[];
    accounts: Row[];
    periods: Row[];
    selfUserId?: string;
    companyId?: string;
    assignmentId?: string;
  };
  expense?: Row;
  initialOpen?: boolean;
  receiptAi?: { canManageAi: boolean; evidenceKey: string };
  hasReceipt?: boolean;
  enableSubmit?: boolean;
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
    currency: String(expense?.currency ?? options.accounts[0]?.currency ?? ""),
    amount: expense?.amount_minor
      ? minorToDecimal(Number(expense.amount_minor))
      : "",
    tax: expense?.tax_minor ? minorToDecimal(Number(expense.tax_minor)) : "",
  };
  const [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const [reviewing, setReviewing] = useState(false),
    [selectedFiles, setSelectedFiles] = useState<File[]>([]);
  const [uploading, setUploading] = useState(false), [progress, setProgress] = useState(""), [createdId, setCreatedId] = useState("");
  const created = useRef<string | null>(null), creating = useRef(false), upload = useRef(createReceiptUploader()), retryFiles = useRef<File[]>([]);
  const createDraft = useRef(createExpenseDraftOnce());
  const submitAttempt = useRef<{ version: number; key: string } | null>(null);
  const savedEdit = useRef<{ fields: string; version: number } | null>(null);
  const [creationStarted, setCreationStarted] = useState(false);
  const [guided,setGuided] = useState<Record<string,string>|null>(null);
  const storageKey = `erpx:expense:${base}:${options.selfUserId}:${expense?.id ?? "new"}`;
  const draft = useExpenseDraft(storageKey, initial),
    { fields, update: setFields } = draft;
  useEffect(() => {
    if (expense || !options.companyId || !options.selfUserId || !options.assignmentId) return;
    const restore=()=>setGuided(readGuidedIntakeHandoff(options.companyId!,options.selfUserId!,options.assignmentId!,base,"expense"));
    // Show suggestions first; never overwrite a recovered form without a choice.
    restore();
    window.addEventListener("erpx:guided-intake-handoff",restore);
    return()=>window.removeEventListener("erpx:guided-intake-handoff",restore);
  },[expense,options.companyId,options.selfUserId,options.assignmentId,base]);
  function applyGuided() {
    if (!guided || !options.companyId || !options.selfUserId || !options.assignmentId) return;
    setFields(previous => ({...previous,
      requestKind: guided.paymentIntent === "company_payment" ? "payment" : guided.paymentIntent === "reimbursement" ? "expense" : previous.requestKind,
      merchant: guided.merchant || previous.merchant,
      businessPurpose: guided.businessPurpose || previous.businessPurpose,
      expenseDate: guided.expenseDate || previous.expenseDate,
      amount: /^\d+(?:\.\d{1,2})?$/.test(guided.amount ?? "") ? guided.amount : previous.amount,
      currency: options.accounts.some(account => String(account.currency) === guided.currency) ? guided.currency : previous.currency,
    }));
    clearGuidedIntakeHandoff(options.companyId,options.selfUserId,options.assignmentId,base,"expense");
    setGuided(null);
  }
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
      if (name === "currency") {
        const account = options.accounts.find(a => String(a.id) === previous.categoryAccountId);
        if (account && String(account.currency) !== value) next.categoryAccountId = "";
        const payee = options.payees?.find(p => String(p.id) === previous.payeePartyId);
        if (payee && String(payee.currency) !== value) next.payeePartyId = "";
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
  async function save(submit: boolean) {
    if (draft.conflict || creating.current || creationStarted) return;
    if (submit && !hasReceipt && !selectedFiles.length) {
      setError(fields.requestKind === "payment" ? "Attach a supporting document before submitting." : "Attach a receipt before submitting.");
      return;
    }
    if (created.current && !submit) { router.push(`${base}/expenses/${created.current}`); return; }
    const fieldSignature = JSON.stringify(fields);
    const expectedVersion = savedEdit.current?.version ?? Number(expense?.version ?? 1);
    const body = JSON.stringify({
      ...fields,
      ...(expense ? { expectedVersion } : {}),
    });
    setPending(true);
    setError("");
    try {
      selectedFiles.forEach(file => validateReceipt(file));
      let id = String(expense?.id ?? created.current ?? "");
      if (!expense && !created.current) {
        id = await createDraft.current(base, storageKey, fields, draft.touched);
        created.current = id;
        setCreatedId(id);
      } else if (expense && savedEdit.current?.fields !== fieldSignature) {
        if (attempt.current?.body !== body)
          attempt.current = { body, key: crypto.randomUUID() };
        const response = await workFetch(`/api/v1${base}/expenses/${expense.id}`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        });
        const result = (await response.json()) as { data?: { version?: number }; error?: { message: string } };
        if (!response.ok) throw Error(result.error?.message ?? "Unable to save this draft.");
        attempt.current = null;
        savedEdit.current = { fields: fieldSignature, version: Number(result.data?.version ?? expectedVersion + 1) };
      }
      for (const file of selectedFiles) {
        validateReceipt(file);
        await upload.current(base, id, file, false, setProgress);
      }
      if (submit) {
        const response = await workFetch(`/api/v1${base}/expenses/${id}`, { cache: "no-store" });
        const result = await response.json() as { data?: { expense?: { version: number; status: string } }; error?: { message: string } };
        if (!response.ok || !result.data?.expense) throw Error(result.error?.message ?? "Unable to review the saved claim.");
        if (result.data.expense.status !== "draft" && result.data.expense.status !== "rejected") {
          router.push(`${base}/expenses/${id}`);
          return;
        }
        const version = Number(result.data.expense.version);
        if (submitAttempt.current?.version !== version)
          submitAttempt.current = { version, key: crypto.randomUUID() };
        const sent = await workFetch(`/api/v1${base}/expenses/${id}/submit`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": submitAttempt.current.key },
          body: JSON.stringify({ expectedVersion: version }),
        });
        const outcome = await sent.json() as { error?: { message: string } };
        if (!sent.ok) throw Error(outcome.error?.message ?? "Unable to submit. Your draft is saved.");
        submitAttempt.current = null;
      }
      draft.clear();
      setSelectedFiles([]);
      if (!expense) router.push(`${base}/expenses/${id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save this draft.");
    } finally {
      setPending(false);
    }
  }
  return (
    <details className={`card ${styles.editor}`} open={!!expense || initialOpen}>
      <summary>{expense ? "Edit claim" : "New expense or payment request"}</summary>
      <p>Tell us what happened, attach evidence, and review the claim before sending it. You can save a draft at any time.</p>
      {guided && <div className="notice" role="status"><p>AI prepared details from your message. Review them before using them in this claim.</p><button type="button" className="button secondary" onClick={applyGuided}>Use prepared details</button> <button type="button" className="button ghost" onClick={()=>{if(options.companyId&&options.selfUserId&&options.assignmentId)clearGuidedIntakeHandoff(options.companyId,options.selfUserId,options.assignmentId,base,"expense");setGuided(null);}}>Discard suggestion</button></div>}
      {expense && receiptAi && <ExpenseAiPanel compact base={base} expenseId={String(expense.id)} version={Number(expense.version)} {...receiptAi} />}
      {!expense && <div className={styles.dropzone} onDragOver={e => e.preventDefault()} onDrop={e => { e.preventDefault(); if (hydrated && !pending && !uploading && !createdId) setSelectedFiles(Array.from(e.dataTransfer.files)); }}>
        <UploadCloud size={28} /><strong>Attach a receipt or supporting document</strong><p>Drop files here or choose them. Review your details before sending; AI reading is optional.</p>
        <label className="button secondary">Choose files<input aria-label="Choose receipts or supporting documents" type="file" multiple accept="image/jpeg,image/png,image/webp,application/pdf" disabled={!hydrated || pending || uploading || !!createdId || draft.conflict} onChange={e => { setSelectedFiles(Array.from(e.target.files ?? [])); e.target.value = ""; }} /></label>
        <span>JPG, PNG, WebP or PDF · Up to 10 MB each</span>
        {!!selectedFiles.length && <><p>{selectedFiles.map(file=>file.name).join(", ")}</p><button type="button" className="button ghost" disabled={pending || uploading} onClick={()=>void receiptFirst(selectedFiles)}>Upload and open draft</button></>}
      </div>}
      {progress && <p role="status">{progress}</p>}
      {createdId && error && <div className="notice"><p>Your draft is saved. Correct any missing details there or retry sending from this page.</p>{!!selectedFiles.length && <button type="button" className="button secondary" disabled={uploading} onClick={() => void receiptFirst(retryFiles.current)}>Retry receipt upload</button>} <Link href={`${base}/expenses/${createdId}`}>Open saved draft</Link></div>}
      {!createdId && creationStarted && error && <div className="notice"><p>Your draft request is saved in this browser. Retry to recover the same draft.</p><button type="button" className="button secondary" disabled={uploading} onClick={() => void receiptFirst(retryFiles.current)}>Recover draft and retry</button></div>}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {draft.notice}
      <form onSubmit={(e) => { e.preventDefault(); if (!enableSubmit) void save(false); else if (reviewing) void save(true); else setReviewing(true); }}>
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
            <label>What are you requesting?<select value={fields.requestKind} onChange={e=>change("requestKind",e.target.value)}><option value="expense">I paid and need reimbursement</option><option value="payment">The company needs to make a payment</option></select></label>{fields.requestKind==="payment"&&<label>Who should be paid?<select value={fields.payeePartyId} onChange={e=>change("payeePartyId",e.target.value)}><option value="">Me (employee payment)</option>{options.payees?.filter(p=>p.currency===fields.currency).map(p=><option key={String(p.id)} value={String(p.id)}>{String(p.name)}</option>)}</select></label>}
              <label>
              {fields.requestKind === "payment" ? "Document date" : "Expense date"}
              <input
                type="date"
                required={enableSubmit}
                value={fields.expenseDate}
                onChange={(e) => change("expenseDate", e.target.value)}
              />
              {aiSource("expenseDate")}
            </label>
            <label>
              {fields.requestKind === "payment" ? "Supplier or merchant" : "Merchant"}
              <input
                aria-label="Merchant"
                maxLength={200}
                required={enableSubmit}
                value={fields.merchant}
                onChange={(e) => change("merchant", e.target.value)}
              />
              {aiSource("merchant")}
            </label>
            <label>
              {fields.requestKind === "payment" ? "Invoice or supporting document number" : "Receipt number"}
                <input
                aria-label="Receipt number"
                maxLength={100}
                required={enableSubmit}
                value={fields.receiptReference}
                onChange={(e) => change("receiptReference", e.target.value)}
              />
              {aiSource("receiptReference")}
            </label>
            <label>
              What type of expense is this?
              <select
                value={fields.categoryAccountId}
                required={enableSubmit}
                onChange={(e) => change("categoryAccountId", e.target.value)}
              >
                <option value="">Choose an expense type</option>
                {options.accounts.filter(a => String(a.currency) === fields.currency).map((a) => (
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
                required={enableSubmit}
                value={fields.currency}
                onChange={(e) =>
                  change("currency", e.target.value.toUpperCase())
                }
              />
              {aiSource("currency")}
            </label>
            <label>
              {fields.requestKind === "payment" ? "Amount requested" : "Amount paid"}
              <input
                type="number"
                step="0.01"
                min="0.01"
                required={enableSubmit}
                value={fields.amount}
                onChange={(e) => change("amount", e.target.value)}
              />
              {aiSource("amount")}
            </label>
          </div>
          <label>
            Business purpose
            <textarea
              maxLength={2000}
              required={enableSubmit}
              minLength={enableSubmit ? 5 : undefined}
              value={fields.businessPurpose}
              onChange={(e) => change("businessPurpose", e.target.value)}
            />
              {aiSource("businessPurpose")}
          </label>
          <details className={styles.advanced}>
            <summary>Tax details (optional)</summary>
            <p>Most people can leave this blank. Finance can classify recoverable tax during review.</p>
            <label>Recoverable tax<input type="number" step="0.01" min="0" value={fields.tax} onChange={(e)=>change("tax",e.target.value)}/>{aiSource("tax")}</label>
            <p>If you enter recoverable tax, enter the amount before that tax above. The request total is both amounts added together.</p>
          </details>
        </fieldset>
        {enableSubmit && reviewing && <section className={styles.review} aria-label="Review claim" role="region">
          <h3>Review before you send</h3>
          <p><strong>{fields.requestKind === "payment" ? "Company payment" : "Reimbursement"}</strong> · {fields.merchant || "Merchant needed"} · {fields.expenseDate || "Date needed"}</p>
          <p>{fields.businessPurpose}</p>
          <p><strong>{fields.currency} {fields.amount || "0"}</strong>{fields.tax ? ` + ${fields.tax} recoverable tax` : ""} · {options.accounts.find(account=>String(account.id)===fields.categoryAccountId)?.name ?? "Expense type needed"}</p>
          <p>{hasReceipt ? "Supporting evidence attached" : `${selectedFiles.length} supporting ${selectedFiles.length === 1 ? "file" : "files"} selected`}</p>
          <p className="muted">Submitting sends this request to the company approval flow. Approval, accounting posting, and payment have separate statuses.</p>
        </section>}
        <div className="form-actions">
          {enableSubmit ? reviewing ? <button className="button" disabled={!hydrated || pending || uploading || draft.conflict}>{pending ? "Sending…" : "Confirm & submit"}</button> : <button className="button" disabled={!hydrated || pending || uploading || draft.conflict}>Review &amp; submit</button> : null}
          <button type="button" className="button secondary" disabled={!hydrated || pending || uploading || !!createdId || draft.conflict} onClick={()=>void save(false)}>{pending ? "Saving…" : "Save draft"}</button>
          {enableSubmit && reviewing && <button type="button" className="button ghost" onClick={()=>setReviewing(false)}>Edit details</button>}
          {!enableSubmit && <button className="button" disabled={!hydrated || pending || draft.conflict}>{pending ? "Saving…" : "Save changes"}</button>}
        </div>
      </form>
    </details>
  );
}
