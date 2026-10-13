"use client";
import { RequestValidation } from "@/components/workplace/request-validation";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { RequestSteps, RoutePreview } from "@/components/workplace/request-experience";
import type { RequestPreview } from "@/lib/work_projection/request-preview";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { RecordLink as Link } from "@/components/records/record-link";
import { UploadCloud, Sparkles } from "lucide-react";
import { createReceiptUploader, validateReceipt } from "@/lib/people_payroll/expense-upload";
import { ExpenseAiPanel } from "@/components/people_payroll/expense-ai-panel";
import { createExpenseDraftOnce } from "@/lib/people_payroll/expense-draft-create";
import styles from "@/components/people_payroll/expense.module.css";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useHydrated } from "@/components/ui/use-hydrated";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { ExpenseRow as Row } from "@/server/modules/people_payroll/expense";
import { useExpenseDraft } from "@/components/people_payroll/use-expense-draft";
import type { ExpenseAiSuggestionEvent } from "@/lib/people_payroll/expense-ai";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@/lib/shared/ai/guided-intake";
import { FormGrid } from "@/components/ui/foundation";

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
  returnTo = "",
}: {
  base: string;
  options: {
    people: Row[];
    payees?: Row[];
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
  returnTo?: string;
}) {
  const router = useRouter(),
    hydrated = useHydrated(),
    attempt = useRef<{ body: string; key: string } | null>(null);
  const initial: Fields = {
    personId: String(
      expense?.employee_id ??
        options.people.find((p) => p.user_id === options.selfUserId)?.id ??
        "",
    ),
    payeePartyId: String(expense?.payee_party_id ?? ""),
    requestKind: String(expense?.request_kind ?? "expense"),
    expenseDate: String(expense?.expense_date ?? ""),
    merchant: String(expense?.merchant ?? ""),
    receiptReference: String(expense?.receipt_reference ?? ""),
    businessPurpose: String(expense?.business_purpose ?? ""),
    categoryAccountId: String(expense?.category_account_id ?? ""),
    currency: String(expense?.currency ?? options.accounts[0]?.currency ?? ""),
    amount: expense?.amount_minor ? minorToDecimal(Number(expense.amount_minor)) : "",
    tax: expense?.tax_minor ? minorToDecimal(Number(expense.tax_minor)) : "",
  };
  const [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const [routePreview,setRoutePreview] = useState<RequestPreview|null>(null);
  const [previewing,setPreviewing] = useState(false);
  const [reviewing, setReviewing] = useState(false),
    [selectedFiles, setSelectedFiles] = useState<File[]>([]);
  const [uploading, setUploading] = useState(false),
    [progress, setProgress] = useState(""),
    [createdId, setCreatedId] = useState("");
  const created = useRef<string | null>(null),
    creating = useRef(false),
    upload = useRef(createReceiptUploader()),
    retryFiles = useRef<File[]>([]);
  const createDraft = useRef(createExpenseDraftOnce());
  const submitAttempt = useRef<{ version: number; key: string } | null>(null);
  const savedEdit = useRef<{ fields: string; version: number } | null>(null);
  const [creationStarted, setCreationStarted] = useState(false);
  const [guided, setGuided] = useState<Record<string, string> | null>(null);
  const storageKey = `erpx:expense:${base}:${options.selfUserId}:${expense?.id ?? "new"}`;
  const draft = useExpenseDraft(storageKey, initial),
    { fields, update: setFields } = draft;
  useEffect(() => {
    if (expense || !options.companyId || !options.selfUserId || !options.assignmentId) return;
    const restore = () =>
      setGuided(
        readGuidedIntakeHandoff(
          options.companyId!,
          options.selfUserId!,
          options.assignmentId!,
          base,
          "expense",
        ),
      );
    // Show suggestions first; never overwrite a recovered form without a choice.
    restore();
    window.addEventListener("erpx:guided-intake-handoff", restore);
    return () => window.removeEventListener("erpx:guided-intake-handoff", restore);
  }, [expense, options.companyId, options.selfUserId, options.assignmentId, base]);
  function applyGuided() {
    if (!guided || !options.companyId || !options.selfUserId || !options.assignmentId) return;
    setFields((previous) => ({
      ...previous,
      requestKind:
        guided.paymentIntent === "company_payment"
          ? "payment"
          : guided.paymentIntent === "reimbursement"
            ? "expense"
            : previous.requestKind,
      merchant: guided.merchant || previous.merchant,
      businessPurpose: guided.businessPurpose || previous.businessPurpose,
      expenseDate: guided.expenseDate || previous.expenseDate,
      amount: /^\d+(?:\.\d{1,2})?$/.test(guided.amount ?? "") ? guided.amount : previous.amount,
      currency: options.accounts.some((account) => String(account.currency) === guided.currency)
        ? guided.currency
        : previous.currency,
    }));
    clearGuidedIntakeHandoff(
      options.companyId,
      options.selfUserId,
      options.assignmentId,
      base,
      "expense",
    );
    setGuided(null);
  }
  useEffect(() => {
    if (expense) {
      const newKey = `erpx:expense:${base}:${options.selfUserId}:new:created`;
      try {
        if (sessionStorage.getItem(newKey) === String(expense.id)) {
          sessionStorage.removeItem(newKey);
          sessionStorage.removeItem(newKey.slice(0, -8));
        }
      } catch {}
      return;
    }
    try {
      const id = sessionStorage.getItem(`${storageKey}:created`);
      if (id) router.replace(`${base}/expenses/${encodeURIComponent(id)}${returnTo ? `?return=${encodeURIComponent(returnTo)}` : ""}`);
      else if (sessionStorage.getItem(`${storageKey}:creation`)) {
        creating.current = true;
        // Restore the pending command lock before allowing another draft action.
        // eslint-disable-next-line react-hooks/set-state-in-effect
        setCreationStarted(true);
        void createDraft
          .current(base, storageKey, {})
          .then((id) => {
            created.current = id;
            setCreatedId(id);
            router.replace(`${base}/expenses/${encodeURIComponent(id)}${returnTo ? `?return=${encodeURIComponent(returnTo)}` : ""}`);
          })
          .catch((e) => {
            try {
              setCreationStarted(!!sessionStorage.getItem(`${storageKey}:creation`));
            } catch {}
            setError(
              e instanceof Error
                ? e.message
                : "Unable to recover your draft. Retry when connected.",
            );
          })
          .finally(() => {
            creating.current = false;
          });
      }
    } catch {}
  }, [expense, storageKey, router, base, options.selfUserId, returnTo]);
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
    setFields(
      (previous) => {
        const next = { ...previous, [name]: value };
        if (name === "categoryAccountId") {
          const account = options.accounts.find((a) => a.id === value);
          if (account) next.currency = String(account.currency);
        }
        if (name === "currency") {
          const account = options.accounts.find((a) => String(a.id) === previous.categoryAccountId);
          if (account && String(account.currency) !== value) next.categoryAccountId = "";
          const payee = options.payees?.find((p) => String(p.id) === previous.payeePartyId);
          if (payee && String(payee.currency) !== value) next.payeePartyId = "";
        }
        return next;
      },
      name === "categoryAccountId" ? [name, "currency"] : [name],
    );
  }
  function aiSource(name: keyof Fields) {
    const field = draft.ai[name];
    if (!field) return null;
    return (
      <span className={styles.aiSource}>
        <Sparkles size={12} /> AI ·{" "}
        {field.citations?.map((c) => (
          <a
            key={`${c.attachmentId}:${c.page}`}
            href={`/api/v1${base}/attachments/${c.attachmentId}/content?page=${c.page}`}
            target="_blank"
            rel="noreferrer"
          >
            Receipt p.{c.page}
          </a>
        ))}
      </span>
    );
  }
  async function receiptFirst(files: File[]) {
    if (
      creating.current ||
      pending ||
      !hydrated ||
      (!files.length && !creationStarted) ||
      draft.conflict
    )
      return;
    creating.current = true;
    setUploading(true);
    setError("");
    retryFiles.current = files;
    try {
      files.forEach((file) => validateReceipt(file));
      setCreationStarted(true);
      if (!created.current) {
        setProgress("Creating your draft…");
        created.current = await createDraft.current(base, storageKey, fields, draft.touched);
        setCreatedId(created.current);
      }
      for (const file of files)
        await upload.current(base, created.current, file, false, setProgress);
      draft.clear();
      router.push(`${base}/expenses/${created.current}${returnTo ? `?return=${encodeURIComponent(returnTo)}` : ""}`);
      router.refresh();
    } catch (e) {
      if (!created.current) {
        try {
          setCreationStarted(!!sessionStorage.getItem(`${storageKey}:creation`));
        } catch {}
      }
      setError(e instanceof Error ? e.message : "Unable to upload receipt. Your draft is safe.");
    } finally {
      creating.current = false;
      setUploading(false);
      setProgress("");
    }
  }
  async function reviewClaim() {
    setPreviewing(true); setError("");
    try {
      selectedFiles.forEach(file => validateReceipt(file));
      if (!hasReceipt && !selectedFiles.length) throw Error("Attach supporting evidence before sending.");
      const response=await workFetch(`/api/v1${base}/expenses/preview`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({...fields,id:expense?.id})});
      const body=await response.json(); if(!response.ok)throw Error(body.error?.message || "Unable to preview routing.");
      setRoutePreview(body.data);setReviewing(true);
    }catch(cause){setError(cause instanceof Error?cause.message:"Unable to preview routing.");}finally{setPreviewing(false);}
  }
  async function save(submit: boolean) {
    if (draft.conflict || creating.current || creationStarted) return;
    if (submit && !hasReceipt && !selectedFiles.length) {
      setError(
        fields.requestKind === "payment"
          ? "Attach a supporting document before submitting."
          : "Attach a receipt before submitting.",
      );
      return;
    }
    if (created.current && !submit) {
      router.push(`${base}/expenses/${created.current}${returnTo ? `?return=${encodeURIComponent(returnTo)}` : ""}`);
      return;
    }
    const fieldSignature = JSON.stringify(fields);
    const expectedVersion = savedEdit.current?.version ?? Number(expense?.version ?? 1);
    const body = JSON.stringify({
      ...fields,
      ...(expense ? { expectedVersion } : {}),
    });
    setPending(true);
    setError("");
    try {
      selectedFiles.forEach((file) => validateReceipt(file));
      let id = String(expense?.id ?? created.current ?? "");
      if (!expense && !created.current) {
        id = await createDraft.current(base, storageKey, fields, draft.touched);
        created.current = id;
        setCreatedId(id);
      } else if (expense && savedEdit.current?.fields !== fieldSignature) {
        if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
        const response = await workFetch(`/api/v1${base}/expenses/${expense.id}`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        });
        const result = (await response.json()) as {
          data?: { version?: number };
          error?: { message: string };
        };
        if (!response.ok) throw Error(result.error?.message ?? "Unable to save this draft.");
        attempt.current = null;
        savedEdit.current = {
          fields: fieldSignature,
          version: Number(result.data?.version ?? expectedVersion + 1),
        };
      }
      for (const file of selectedFiles) {
        validateReceipt(file);
        await upload.current(base, id, file, false, setProgress);
      }
      if (submit) {
        const response = await workFetch(`/api/v1${base}/expenses/${id}${returnTo ? `?return=${encodeURIComponent(returnTo)}` : ""}`, { cache: "no-store" });
        const result = (await response.json()) as {
          data?: { expense?: { version: number; status: string } };
          error?: { message: string };
        };
        if (!response.ok || !result.data?.expense)
          throw Error(result.error?.message ?? "Unable to review the saved claim.");
        if (result.data.expense.status !== "draft" && result.data.expense.status !== "rejected") {
          router.push(`${base}/expenses/${id}${returnTo ? `?return=${encodeURIComponent(returnTo)}` : ""}`);
          return;
        }
        const version = Number(result.data.expense.version);
        if (submitAttempt.current?.version !== version)
          submitAttempt.current = { version, key: crypto.randomUUID() };
        const sent = await workFetch(`/api/v1${base}/expenses/${id}/submit`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": submitAttempt.current.key,
          },
          body: JSON.stringify({ expectedVersion: version }),
        });
        const outcome = (await sent.json()) as { error?: { message: string } };
        if (!sent.ok)
          throw Error(outcome.error?.message ?? "Unable to submit. Your draft is saved.");
        submitAttempt.current = null;
      }
      draft.clear();
      setSelectedFiles([]);
      if (!expense) router.push(`${base}/expenses/${id}${returnTo ? `?return=${encodeURIComponent(returnTo)}` : ""}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save this draft.");
    } finally {
      setPending(false);
    }
  }
  return (
    <FormDisclosure appearance="standard" inline={Boolean(!!expense || initialOpen)} className={`card ${styles.editor}`} open={!!expense || initialOpen}>
      <summary hidden={initialOpen && !expense}>{expense ? "Edit claim" : "Expense claim"}</summary>
      <RequestSteps reviewing={reviewing} />
      <p>
        Tell us what happened, attach evidence, and review the claim before sending it. You can save
        a draft at any time.
      </p>
      {guided && (
        <InlineNotice tone="info" className="notice" role="status">
          <p>AI prepared details from your message. Review them before using them in this claim.</p>
          <Button appearance="standard" variant="secondary" type="button" onClick={applyGuided}>
            Use prepared details
          </Button>{" "}
          <Button appearance="standard"
            variant="ghost"
            type="button"
            onClick={() => {
              if (options.companyId && options.selfUserId && options.assignmentId)
                clearGuidedIntakeHandoff(
                  options.companyId,
                  options.selfUserId,
                  options.assignmentId,
                  base,
                  "expense",
                );
              setGuided(null);
            }}
          >
            Discard suggestion
          </Button>
        </InlineNotice>
      )}
      {expense && receiptAi && (
        <details><summary>Fill from receipt · AI assistance</summary><ExpenseAiPanel
          compact
          base={base}
          expenseId={String(expense.id)}
          version={Number(expense.version)}
          {...receiptAi}
        /></details>
      )}
      {!expense && !reviewing && (
        <div
          className={styles.dropzone}
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            if (hydrated && !pending && !uploading && !createdId)
              setSelectedFiles(Array.from(e.dataTransfer.files));
          }}
        >
          <UploadCloud size={28} />
          <strong>Attach a receipt or supporting document</strong>
          <p>
            Drop files here or choose them. Review your details before sending; AI reading is
            optional.
          </p>
          <Field appearance="standard" className="button secondary">
            Choose files
            <TextInput appearance="standard"
              aria-label="Choose receipts or supporting documents"
              type="file"
              multiple
              accept="image/jpeg,image/png,image/webp,application/pdf"
              disabled={!hydrated || pending || uploading || !!createdId || draft.conflict}
              onChange={(e) => {
                setSelectedFiles(Array.from(e.target.files ?? []));
                e.target.value = "";
              }}
            />
          </Field>
          <span>JPG, PNG, WebP or PDF · Up to 10 MB each</span>
          {!!selectedFiles.length && (
            <>
              <p>{selectedFiles.map((file) => file.name).join(", ")}</p>
              <Button appearance="standard"
                variant="ghost"
                type="button"
                disabled={pending || uploading}
                onClick={() => void receiptFirst(selectedFiles)}
              >
                Upload and open draft
              </Button>
            </>
          )}
        </div>
      )}
      {progress && <InlineNotice tone="info" role="status">{progress}</InlineNotice>}
      {createdId && error && (
        <div className="notice">
          <p>
            Your draft is saved. Correct any missing details there or retry sending from this page.
          </p>
          {!!selectedFiles.length && (
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={uploading}
              onClick={() => void receiptFirst(retryFiles.current)}
            >
              Retry receipt upload
            </Button>
          )}{" "}
          <Link href={`${base}/expenses/${createdId}`}>Open saved draft</Link>
        </div>
      )}
      {!createdId && creationStarted && error && (
        <div className="notice">
          <p>Your draft request is saved in this browser. Retry to recover the same draft.</p>
          <Button appearance="standard"
            variant="secondary"
            type="button"
            disabled={uploading}
            onClick={() => void receiptFirst(retryFiles.current)}
          >
            Recover draft and retry
          </Button>
        </div>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {draft.notice}
      {!expense && !options.people.some(p=>p.user_id===options.selfUserId) && !fields.personId && <InlineNotice tone="info" role="status"><p>Your draft can be saved now. Company HR must link your active engagement and department before you can submit a personal claim.</p><Link href={`${base}/profiles/${encodeURIComponent(options.selfUserId??"")}`}>Open your profile →</Link></InlineNotice>}
      <RequestValidation><form
        onSubmit={(e) => {
          e.preventDefault();
          if (!enableSubmit) void save(false);
          else if (reviewing) void save(true);
          else void reviewClaim();
        }}
      >
        <fieldset
          disabled={reviewing || !hydrated || pending || uploading || creationStarted || !!createdId}
          className={styles.fields}
          hidden={reviewing}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard" hidden={options.people.length<=1 && options.people.every(p=>p.user_id===options.selfUserId)}>
              Claimant
              <Select appearance="standard"
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
              </Select>
            </Field>
            <Field appearance="standard">
              What are you requesting?
              <Select appearance="standard"
                value={fields.requestKind}
                onChange={(e) => change("requestKind", e.target.value)}
              >
                <option value="expense">I paid and need reimbursement</option>
                <option value="payment">The company needs to make a payment</option>
              </Select>
            </Field>
            {fields.requestKind === "payment" && (
              <Field appearance="standard">
                Who should be paid?
                <Select appearance="standard"
                  value={fields.payeePartyId}
                  onChange={(e) => change("payeePartyId", e.target.value)}
                >
                  <option value="">Me (employee payment)</option>
                  {options.payees
                    ?.filter((p) => p.currency === fields.currency)
                    .map((p) => (
                      <option key={String(p.id)} value={String(p.id)}>
                        {String(p.name)}
                      </option>
                    ))}
                </Select>
              </Field>
            )}
            <Field appearance="standard">
              {fields.requestKind === "payment" ? "Document date" : "Expense date"}
              <TextInput appearance="standard"
                type="date"
                required={enableSubmit}
                value={fields.expenseDate}
                onChange={(e) => change("expenseDate", e.target.value)}
              />
              {aiSource("expenseDate")}
            </Field>
            <Field appearance="standard">
              {fields.requestKind === "payment" ? "Supplier or merchant" : "Merchant"}
              <TextInput appearance="standard"
                aria-label="Merchant"
                maxLength={200}
                required={enableSubmit}
                value={fields.merchant}
                onChange={(e) => change("merchant", e.target.value)}
              />
              {aiSource("merchant")}
            </Field>
            <Field appearance="standard">
              {fields.requestKind === "payment"
                ? "Invoice or supporting document number"
                : "Receipt number"}
              <TextInput appearance="standard"
                aria-label="Receipt number"
                maxLength={100}
                required={enableSubmit}
                value={fields.receiptReference}
                onChange={(e) => change("receiptReference", e.target.value)}
              />
              {aiSource("receiptReference")}
            </Field>
            <Field appearance="standard">
              What type of expense is this?
              <Select appearance="standard"
                value={fields.categoryAccountId}
                required={enableSubmit}
                onChange={(e) => change("categoryAccountId", e.target.value)}
              >
                <option value="">Choose an expense type</option>
                {options.accounts
                  .filter((a) => String(a.currency) === fields.currency)
                  .map((a) => (
                    <option key={String(a.id)} value={String(a.id)}>
                      {String(a.name)} · {String(a.currency)}
                    </option>
                  ))}
              </Select>
              {aiSource("categoryAccountId")}
            </Field>
            <Field appearance="standard">
              Currency
              <TextInput appearance="standard"
                maxLength={3}
                required={enableSubmit}
                value={fields.currency}
                onChange={(e) => change("currency", e.target.value.toUpperCase())}
              />
              {aiSource("currency")}
            </Field>
            <Field appearance="standard">
              {fields.requestKind === "payment" ? "Amount requested" : "Amount paid"}
              <TextInput appearance="standard"
                type="number"
                step="0.01"
                min="0.01"
                required={enableSubmit}
                value={fields.amount}
                onChange={(e) => change("amount", e.target.value)}
              />
              {aiSource("amount")}
            </Field>
          </FormGrid>
          <Field appearance="standard">
            Business purpose
            <TextArea appearance="standard"
              maxLength={2000}
              required={enableSubmit}
              minLength={enableSubmit ? 5 : undefined}
              value={fields.businessPurpose}
              onChange={(e) => change("businessPurpose", e.target.value)}
            />
            {aiSource("businessPurpose")}
          </Field>
          <details className={styles.advanced}>
            <summary>Tax details (optional)</summary>
            <p>
              Most people can leave this blank. Finance can classify recoverable tax during review.
            </p>
            <Field appearance="standard">
              Recoverable tax
              <TextInput appearance="standard"
                type="number"
                step="0.01"
                min="0"
                value={fields.tax}
                onChange={(e) => change("tax", e.target.value)}
              />
              {aiSource("tax")}
            </Field>
            <p>
              If you enter recoverable tax, enter the amount before that tax above. The request
              total is both amounts added together.
            </p>
          </details>
        </fieldset>
        {enableSubmit && reviewing && (
          <section className={styles.review} aria-label="Review claim" role="region">
            <h3>Review before you send</h3>
            <p>
              <strong>
                {fields.requestKind === "payment" ? "Company payment" : "Reimbursement"}
              </strong>{" "}
              · {fields.merchant || "Merchant needed"} · {fields.expenseDate || "Date needed"}
            </p>
            <p>{fields.businessPurpose}</p>
            <p>
              <strong>
                {fields.currency} {fields.amount || "0"}
              </strong>
              {fields.tax ? ` + ${fields.tax} recoverable tax` : ""} ·{" "}
              {options.accounts.find((account) => String(account.id) === fields.categoryAccountId)
                ?.name ?? "Expense type needed"}
            </p>
            <p>
              {hasReceipt
                ? "Supporting evidence attached"
                : `${selectedFiles.length} supporting ${selectedFiles.length === 1 ? "file" : "files"} selected`}
            </p>
            <p className="muted">
              Submitting sends this request to the company approval flow. Approval, accounting
              posting, and payment have separate statuses.
            </p>
          </section>
        )}
        {reviewing && <RoutePreview data={routePreview} base={base} />}
        <div className="ui-form-actions request-footer">
          {enableSubmit ? (
            reviewing ? (
              <Button appearance="standard"
                clientManaged type="submit"
                variant="primary"
                disabled={!hydrated || pending || uploading || previewing || draft.conflict || (reviewing && !!routePreview?.blockers.length)}
              >
                {pending ? "Sending…" : "Confirm & submit"}
              </Button>
            ) : (
              <Button appearance="standard"
                clientManaged type="submit"
                variant="primary"
                disabled={!hydrated || pending || uploading || previewing || draft.conflict || (reviewing && !!routePreview?.blockers.length)}
              >
                Review &amp; submit
              </Button>
            )
          ) : null}
          <Button appearance="standard"
            variant="secondary"
            type="button"
            disabled={!hydrated || pending || uploading || !!createdId || draft.conflict}
            onClick={() => void save(false)}
          >
            {pending ? "Saving…" : "Save draft"}
          </Button>
          {enableSubmit && reviewing && (
            <Button appearance="standard" variant="ghost" type="button" onClick={() => setReviewing(false)}>
              Edit details
            </Button>
          )}
          {!enableSubmit && (
            <Button appearance="standard"
              clientManaged type="submit"
              variant="primary"
              disabled={!hydrated || pending || draft.conflict}
            >
              {pending ? "Saving…" : "Save changes"}
            </Button>
          )}
        </div>
      </form></RequestValidation>
    </FormDisclosure>
  );
}
