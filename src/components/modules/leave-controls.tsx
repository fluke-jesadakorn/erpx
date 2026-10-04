"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDrawer } from "@/components/ui/dialog";
import { Button, Field, TextArea, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Card, FormLayout, Stack } from "@/components/ui/foundation";
export function LeaveControls({
  base,
  recordId,
  version,
  canReject = false,
  canCancel = false,
  initialOpen = false,
}: {
  base: string;
  recordId?: string;
  version?: number;
  canReject?: boolean;
  canCancel?: boolean;
  initialOpen?: boolean;
}) {
  const router = useRouter(),
    attempt = useRef<{ body: string; key: string } | null>(null);
  const [open, setOpen] = useState(initialOpen),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function submit(form: FormData, action?: string) {
    const body = JSON.stringify(
      recordId
        ? {
            action,
            expectedVersion: version,
            note: String(form.get("note") ?? ""),
          }
        : {
            startDate: String(form.get("startDate")),
            days: Number(form.get("days")),
            handoverNote: String(form.get("handoverNote")),
          },
    );
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(
        `/api/v1${base}/leave-requests${recordId ? `/${recordId}/transition` : ""}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": attempt.current.key,
          },
          body,
        },
      );
      const result = (await r.json()) as {
        data?: { id: string };
        error?: { message: string };
      };
      if (!r.ok) throw new Error(result.error?.message ?? "Could not save leave.");
      if (!recordId) router.push(`${base}/tasks/people_payroll/${result.data?.id}`);
      else router.refresh();
      setOpen(false);
      attempt.current = null;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Connection interrupted. Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  if (recordId)
    return (
      <Card as="section" appearance="standard" className="ui-panel">
        <FormLayout className="ui-form-grid" onSubmit={(e) => e.preventDefault()}>
          <Field appearance="standard">
            Decision note
            <TextArea appearance="standard" name="note" maxLength={2048} />
          </Field>
          <div className="ui-form-actions">
            {canReject && (
              <Button appearance="standard"
                variant="secondary"

                disabled={busy}
                type="button"
                onClick={(e) => submit(new FormData(e.currentTarget.form!), "reject")}
              >
                Reject request
              </Button>
            )}
            {canCancel && (
              <Button appearance="standard"
                variant="secondary"

                disabled={busy}
                type="button"
                onClick={(e) => submit(new FormData(e.currentTarget.form!), "cancel")}
              >
                Cancel request
              </Button>
            )}
          </div>
          {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
        </FormLayout>
      </Card>
    );
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <Button appearance="standard" type="submit" variant="primary" onClick={() => setOpen(!open)}>
        Request leave
      </Button>
      {open && (
        <FormDrawer appearance="standard" inline={initialOpen} open={open} onClose={() => setOpen(false)} title="Request leave" dismissDisabled={busy}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<FormLayout
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            submit(new FormData(e.currentTarget));
          }}
        >
          <Field appearance="standard">
            First day
            <TextInput appearance="standard" type="date" name="startDate" required />
          </Field>
          <Field appearance="standard">
            Requested working days
            <TextInput appearance="standard" type="number" name="days" min={1} max={365} defaultValue={1} required />
          </Field>
          <Field appearance="standard">
            Handover plan
            <TextArea appearance="standard" name="handoverNote" maxLength={2048} />
          </Field>
          <p className="muted">
            Enter working days under your company’s leave policy. Your department approver reviews
            first, then an independent HR administrator.
          </p>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Save leave draft
          </Button>
          {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
        </FormLayout></Stack></FormDrawer>
      )}
    </Card>
  );
}
