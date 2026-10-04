"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { Row } from "@/server/commerce";
import type { customerFollowupDetail } from "@/server/customer-followups";
import { Card, FormLayout } from "@/components/ui/foundation";
export function CustomerFollowupForm({
  base,
  order,
  recipients,
  detail,
}: {
  base: string;
  order?: Row | null;
  recipients: { id: string; name: string }[];
  detail?: Awaited<ReturnType<typeof customerFollowupDetail>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const r = await workFetch(`/api/v1${base}/customer-followups`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: JSON.stringify(input),
        }),
        result = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save follow-up.");
      }
      last.current = null;
      router.push(`${base}/customer-followups/${result.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this command.");
    } finally {
      setBusy(false);
    }
  }
  const owners = (
    <Field appearance="standard">
      Responsible colleague
      <Select appearance="standard" name="owner" required defaultValue={String(detail?.row.owner_id ?? "")}>
        <option value="">Choose an eligible colleague</option>
        {recipients.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </Select>
    </Field>
  );
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {order && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Schedule customer follow-up</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                orderId: order.id,
                expectedVersion: Number(order.version),
                ownerId: f.get("owner"),
                dueDate: f.get("due"),
                nextAction: f.get("next"),
              });
            }}
          >
            {owners}
            <Field appearance="standard">
              Due date
              <TextInput appearance="standard" name="due" type="date" required />
            </Field>
            <Field appearance="standard">
              Next action
              <TextArea appearance="standard" name="next" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !recipients.length}>
              Assign follow-up
            </Button>
          </FormLayout>
          {!recipients.length && (
            <p>
              No eligible sales colleague. Ask the Company administrator to restore role coverage.
            </p>
          )}
        </Card>
      )}
      {detail?.canComplete && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Record contact outcome</h2>
          <p>
            This records what you did; it does not send an email or change the customer’s order.
          </p>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                id: detail.row.id,
                expectedVersion: Number(detail.row.version),
                action: f.get("action"),
                reason: f.get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Result
              <Select appearance="standard" name="action">
                <option value="complete">Contact completed</option>
                <option value="cancel">Cancel follow-up</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Evidence and outcome
              <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record outcome
            </Button>
          </FormLayout>
        </Card>
      )}
      {detail?.canManage && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Manage responsibility</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                id: detail.row.id,
                expectedVersion: Number(detail.row.version),
                action: "reassign",
                ownerId: f.get("owner"),
                dueDate: f.get("due"),
                reason: f.get("reason"),
              });
            }}
          >
            {owners}
            <Field appearance="standard">
              Due date
              <TextInput appearance="standard"
                name="due"
                type="date"
                defaultValue={String(detail.row.due_date)}
                required
              />
            </Field>
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record management decision
            </Button>
          </FormLayout>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              void send({
                id: detail.row.id,
                expectedVersion: Number(detail.row.version),
                action: "cancel",
                reason: new FormData(e.currentTarget).get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Cancellation reason
              <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Cancel this follow-up
            </Button>
          </FormLayout>
        </Card>
      )}
    </>
  );
}
