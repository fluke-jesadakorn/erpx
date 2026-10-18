"use client";
import {useExpensePurchase} from "./expense-purchase-context";
import {expenseActionAllowed,expenseCommandPurpose} from "@/lib/people_payroll/expense-purchases";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { ReviewDecisionForm } from "@/components/workplace/review-decision";
import { workFetch } from "@/lib/work_projection/work-fetch";
import Image from "next/image";
import { useLayoutEffect, useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import type { ExpenseRow as Row } from "@/server/modules/people_payroll/expense";
import { createReceiptUploader, validateReceipt } from "@/lib/people_payroll/expense-upload";
import styles from "@/components/people_payroll/expense.module.css";
import { Card, FormGrid } from "@/components/ui/foundation";
export { ExpenseEditor } from "@/components/people_payroll/expense-editor";
export function useExpenseCommand(base: string) {
  const purchase=useExpensePurchase(),currentPurchase=useRef(purchase);useLayoutEffect(()=>{currentPurchase.current=purchase;},[purchase]);
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null);
  const [pending, setPending] = useState(false),
    [error, setError] = useState("");
  async function send(path: string, input: unknown, create = false) {
    const body = JSON.stringify(input),
      fingerprint = path + body;
    const retry=last.current?.body===fingerprint,contextKey=purchase?.contextKey;
    if(!expenseActionAllowed(purchase,retry?"history":expenseCommandPurpose(path))){setError("The current Expense purchase does not permit this action.");return false;}
    if (last.current?.body !== fingerprint)
      last.current = { body: fingerprint, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/expenses${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": last.current.key,
        },
        body,
      });
      const v = (await r.json()) as {
        data: { id: string; uploadUrl: string; checksum: string };
        error?: { message: string };
      };
      if (!r.ok) throw new Error(v.error?.message ?? "Unable to save claim.");
      if(currentPurchase.current?.contextKey!==contextKey||!expenseActionAllowed(currentPurchase.current,"history"))throw new Error("The Expense context changed. Keep the original request for an authorized retry.");
      last.current = null;
      if (create) router.push(canonicalUiHref(`${base}/expenses/${v.data.id}`));
      router.refresh();
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save claim.");
      return false;
    } finally {
      setPending(false);
    }
  }
  return { send, pending, error, canNewWork:expenseActionAllowed(purchase,"new-work"),canSettle:expenseActionAllowed(purchase,"settle-existing") };
}
export function ExpenseActions({
  base,
  expense,
  editable,
  reviewable,
  canPost,
  periods,
  canCancel = false,
}: {
  base: string;
  expense: Row;
  editable: boolean;
  reviewable: boolean;
  canPost: boolean;
  periods: Row[];
  canCancel?: boolean;
}) {
  const { send, pending, error } = useExpenseCommand(base),
    [reason, setReason] = useState("");
  const today = new Date().toISOString().slice(0, 10),
    currentPeriod = String(
      periods.find((p) => String(p.start_date) <= today && String(p.end_date) >= today)?.id ??
        periods[0]?.id ??
        "",
    );
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Workflow</h2>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {(editable || canCancel) && (
        <>
          <Field appearance="standard">
            Decision reason
            <TextArea appearance="standard"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              maxLength={2000}
              placeholder="Required for approval, rejection or cancellation"
            />
          </Field>
          <div className="ui-form-actions">
            {editable && (
              <>
                <Button appearance="standard"
                  type="submit"
                  variant="primary"

                  disabled={pending || !!expense.approval_blocked_reason}
                  onClick={() =>
                    void send(`/${expense.id}/submit`, {
                      expectedVersion: expense.version,
                    })
                  }
                >
                  Submit for approval
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${expense.id}/cancel`, {
                      expectedVersion: expense.version,
                      reason,
                    })
                  }
                >
                  Cancel claim
                </Button>
              </>
            )}
            {canCancel && !editable && (
              <Button appearance="standard"
                type="submit"
                variant="secondary"

                disabled={pending || reason.trim().length < 3}
                onClick={() =>
                  void send(`/${expense.id}/cancel`, {
                    expectedVersion: expense.version,
                    reason,
                  })
                }
              >
                Cancel claim
              </Button>
            )}
          </div>
        </>
      )}
            {reviewable && <ReviewDecisionForm actions={["approve","reject"]} busy={pending} note={reason} onNote={setReason} minimum={()=>3} onConfirm={action=>void send(`/${expense.id}/${action}`,{expectedVersion:expense.version,reason})}/>}
      {expense.status === "approved" && canPost && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`/${expense.id}/post`, {
              expectedVersion: expense.version,
              periodId: f.get("periodId"),
              entryDate: f.get("entryDate"),
            });
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Accounting period
              <Select appearance="standard" name="periodId" required defaultValue={currentPeriod}>
                {periods.map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Posting date
              <TextInput appearance="standard"
                name="entryDate"
                type="date"
                required
                min={String(expense.expense_date)}
                defaultValue={new Date().toISOString().slice(0, 10)}
              />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending}>
              Post expense payable
            </Button>
          </FormGrid>
        </form>
      )}
      {expense.status === "posted" &&
        canPost &&
        Number(expense.outstanding_minor) ===
          Number(expense.amount_minor) + Number(expense.tax_minor) && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send(`/${expense.id}/reverse`, {
                expectedVersion: expense.version,
                periodId: f.get("periodId"),
                entryDate: f.get("entryDate"),
                reason: f.get("reason"),
              });
            }}
          >
            <h3>Reverse unpaid claim</h3>
            <FormGrid as="div" className="ui-form-grid">
              <Field appearance="standard">
                Reversal period
                <Select appearance="standard" name="periodId" required defaultValue={currentPeriod}>
                  {periods.map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {String(p.name)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Reversal date
                <TextInput appearance="standard"
                  name="entryDate"
                  type="date"
                  required
                  defaultValue={new Date().toISOString().slice(0, 10)}
                />
              </Field>
              <Field appearance="standard">
                Reversal reason
                <TextInput appearance="standard" name="reason" required minLength={3} maxLength={2000} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending}>
                Reverse posting and cancel claim
              </Button>
            </FormGrid>
          </form>
        )}
      {!editable && !reviewable && !(expense.status === "approved" && canPost) && (
        <p className="muted">
          {expense.status === "in_review"
            ? "Awaiting the named department approver."
            : expense.status === "approved"
              ? "Approved. Awaiting an independent finance approver to post the obligation."
              : expense.status === "posted"
                ? "The employee payable is posted. Reimbursements reduce its outstanding balance."
                : expense.status === "reimbursed"
                  ? "The recorded reimbursements cover this claim in full."
                  : "This claim is closed."}
        </p>
      )}
    </Card>
  );
}
export function ExpenseReceipts({
  base,
  expense,
  attachments,
  editable,
  payment,
}: {
  base: string;
  expense: Row;
  attachments: Row[];
  editable: boolean;
  payment?: Row;
}) {
  const router = useRouter(),
    { send, pending, error } = useExpenseCommand(base),
    [uploading, setUploading] = useState(false),
    [uploadError, setUploadError] = useState("");
  const upload = useRef(createReceiptUploader());
  const busy = useRef(false),
    retryFiles = useRef<File[]>([]),
    receiptInput = useRef<HTMLInputElement>(null);
  const [progress, setProgress] = useState("");
  async function attach(files: File[]) {
    if (busy.current || !files.length) return;
    busy.current = true;
    retryFiles.current = files;
    setUploading(true);
    setUploadError("");
    try {
      files.forEach((file) => validateReceipt(file, !payment));
      for (const file of files)
        await upload.current(base, String(payment?.id ?? expense.id), file, !!payment, setProgress);
      retryFiles.current = [];
      if (receiptInput.current) receiptInput.current.value = "";
    } catch (e) {
      setUploadError(e instanceof Error ? e.message : "Upload failed.");
    } finally {
      busy.current = false;
      setUploading(false);
      setProgress("");
      router.refresh();
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{payment ? "Bank payment slip" : "Receipts"}</h2>
      {(error || uploadError) && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error || uploadError}
        </InlineNotice>
      )}
      {attachments.length === 0 && (
        <p className="muted">
          {payment
            ? "Attach the real bank slip for this payment before confirming."
            : "Attach the merchant receipt before submitting this claim."}
        </p>
      )}
      {progress && <InlineNotice tone="info" role="status">{progress}</InlineNotice>}
      {editable && uploadError && (
        <Button appearance="standard"
          variant="secondary"
          type="button"
          disabled={uploading || pending}
          onClick={() => void attach(retryFiles.current)}
        >
          Retry receipt upload
        </Button>
      )}
      {attachments.map((a) => (
        <div className={`ui-record-item ${styles.receiptItem}`} key={String(a.id)}>
          {["image/jpeg", "image/png", "image/webp", "application/pdf"].includes(
            String(a.media_type),
          ) && (
            <a
              className={styles.receiptPreview}
              href={`/api/v1${base}/attachments/${a.id}/content`}
              target="_blank"
              rel="noreferrer"
            >
              <Image
                unoptimized
                width={800}
                height={1000}
                src={`/api/v1${base}/attachments/${a.id}/content?page=1`}
                alt={`Preview of ${a.original_name}`}
                loading="lazy"
              />
            </a>
          )}
          <a className="row-main" href={`/api/v1${base}/attachments/${a.id}/content`}>
            {String(a.original_name)} · {Math.ceil(Number(a.bytes) / 1024)} KB
          </a>
          {editable && (
            <Button appearance="standard"
              type="submit"
              variant="secondary"

              disabled={pending || uploading}
              onClick={() =>
                void send(
                  `/${expense.id}${payment ? `/payments/${payment.id}` : ""}/attachments/${a.id}/revoke`,
                  {
                    expectedVersion: payment?.version ?? expense.version,
                  },
                )
              }
            >
              Remove {String(a.original_name)}
            </Button>
          )}
        </div>
      ))}
      {editable && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            const file = new FormData(e.currentTarget).get("receipt");
            if (file instanceof File && file.size)
              void attach(
                new FormData(e.currentTarget)
                  .getAll("receipt")
                  .filter((f): f is File => f instanceof File && f.size > 0),
              );
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              {payment ? "Payment slip file" : "Receipt file"}
              <TextInput appearance="standard"
                ref={receiptInput}
                name="receipt"
                type="file"
                multiple
                accept={
                  payment
                    ? "image/jpeg,image/png,image/webp,application/pdf"
                    : "image/jpeg,image/png,image/webp,application/pdf,text/csv"
                }
                required
                disabled={uploading}
              />
            </Field>
            <Button appearance="standard"
              clientManaged type="submit"
              variant="secondary"

              disabled={uploading || pending}
            >
              {uploading ? "Uploading…" : payment ? "Attach slip" : "Attach receipt"}
            </Button>
          </FormGrid>
          <p className="muted">
            {payment ? "PDF or image" : "PDF, image or CSV"}, up to 10 MB. Evidence is private to
            the claimant and authorized reviewers.
          </p>
        </form>
      )}
    </Card>
  );
}
