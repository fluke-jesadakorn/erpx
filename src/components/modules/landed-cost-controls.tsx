"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Dialog } from "@/components/ui/dialog";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { Card, Stack, FormLayout } from "@/components/ui/foundation";

type Row = Record<string, string | number | null>;

function useCommand(base: string) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  async function send(path: string, input: unknown) {
    if (busy) return null;
    setBusy(true);
    setError("");
    const fingerprint = path + JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/${path}`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: JSON.stringify(input),
        }),
        body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "The landed cost action failed.");
      last.current = null;
      router.refresh();
      return body.data as Row;
    } catch (e) {
      setError(e instanceof Error ? e.message : "The landed cost action failed.");
      return null;
    } finally {
      setBusy(false);
    }
  }
  return { busy, error, send };
}

export function LandedCostCreateForm({
  base,
  fulfillment,
  accounts,
  currency,
}: {
  base: string;
  fulfillment: Row;
  accounts: Row[];
  currency: string;
}) {
  const command = useCommand(base),
    [open, setOpen] = useState(false);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <div className="ui-section-header">
        <div>
          <strong>Allocate freight, duty or handling</strong>
          <p className="muted">
            Capitalizes the cost across stock from this posted receipt. All received stock and lots
            must still be in the receiving warehouse.
          </p>
        </div>
        <Button appearance="standard" variant="secondary" type="button" onClick={() => setOpen((v) => !v)}>
          {open ? "Close" : "Add landed cost"}
        </Button>
      </div>
      {open && (
        <Dialog appearance="standard" open={open} onClose={() => setOpen(false)} title="Allocate landed cost" presentation="form-drawer" dismissDisabled={command.busy}><Stack as="div" className="ui-stack">{command.error && <InlineNotice tone="danger" role="alert" className="error">{command.error}</InlineNotice>}<FormLayout
          className="ui-form-grid"
          onSubmit={async (e) => {
            e.preventDefault();
            const form = new FormData(e.currentTarget),
              result = await command.send("landed-costs", {
                fulfillmentId: String(fulfillment.id),
                costDate: String(form.get("costDate")),
                description: String(form.get("description")),
                allocationMethod: String(form.get("allocationMethod")),
                amount: String(form.get("amount")),
                creditAccountId: String(form.get("creditAccountId")),
              });
            if (result) setOpen(false);
          }}
        >
          <Field appearance="standard">
            Cost date
            <TextInput appearance="standard"
              name="costDate"
              type="date"
              min={String(fulfillment.fulfillment_date ?? "")}
              defaultValue={String(fulfillment.fulfillment_date ?? "")}
              required
            />
          </Field>
          <Field appearance="standard">
            Description
            <TextInput appearance="standard"
              name="description"
              minLength={5}
              maxLength={500}
              placeholder="Ocean freight, import duty or handling reference"
              required
            />
          </Field>
          <Field appearance="standard">
            Allocation basis
            <Select appearance="standard" name="allocationMethod" defaultValue="receipt_value">
              <option value="receipt_value">Receipt value</option>
              <option value="quantity">Received quantity</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Amount ({currency})
            <TextInput appearance="standard"
              name="amount"
              inputMode="decimal"
              pattern="\d{1,12}(\.\d{1,2})?"
              placeholder="0.00"
              required
            />
          </Field>
          <Field appearance="standard">
            Credit account
            <Select appearance="standard" name="creditAccountId" required defaultValue="">
              <option value="" disabled>
                Select freight accrual, duty payable or expense
              </option>
              {accounts.map((a) => (
                <option key={String(a.id)} value={String(a.id)}>
                  {String(a.code)} · {String(a.name)} · {String(a.category)}
                </option>
              ))}
            </Select>
          </Field>
          <p className="muted">
            The posted journal debits inventory and credits this account after independent Company
            Finance approval.
          </p>
          {command.error && (
            <InlineNotice tone="danger" className="error" role="alert">
              {command.error}
            </InlineNotice>
          )}
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={command.busy}>
            {command.busy ? "Submitting…" : "Submit for review"}
          </Button>
        </FormLayout></Stack></Dialog>
      )}
    </Card>
  );
}

export function LandedCostActions({
  base,
  record,
  canApprove,
  canCancel,
}: {
  base: string;
  record: Row;
  canApprove: boolean;
  canCancel: boolean;
}) {
  const command = useCommand(base),
    [reason, setReason] = useState("");
  const status = String(record.status),
    action = status === "approved" ? "post" : "approve";
  if (!(
    (status === "submitted" && (canApprove || canCancel)) ||
    (status === "approved" && canApprove)
  ))
    return null;
  const run = (next: "approve" | "reject" | "cancel" | "post") =>
    command.send(`landed-costs/${record.id}/${next}`, {
      expectedVersion: Number(record.version),
      reason,
    });
  return (
    <FormLayout className="ui-form-grid" onSubmit={(e) => e.preventDefault()}>
      <Field appearance="standard">
        Decision reason
        <TextArea appearance="standard"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          minLength={3}
          maxLength={1000}
          placeholder="State the evidence checked or correction required"
          required
        />
      </Field>
      {command.error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {command.error}
        </InlineNotice>
      )}
      <div className="ui-form-actions">
        {canApprove && (
          <Button appearance="standard"
            variant="primary"
            disabled={command.busy || reason.trim().length < 3}
            type="button"
            onClick={() => run(action)}
          >
            {action === "post" ? "Post inventory value and GL" : "Approve allocation"}
          </Button>
        )}
        {canApprove && status === "submitted" && (
          <Button appearance="standard"
            variant="secondary"
            disabled={command.busy || reason.trim().length < 3}
            type="button"
            onClick={() => run("reject")}
          >
            Reject for correction
          </Button>
        )}
        {canCancel && status === "submitted" && (
          <Button appearance="standard"
            variant="secondary"
            disabled={command.busy || reason.trim().length < 3}
            type="button"
            onClick={() => run("cancel")}
          >
            Cancel
          </Button>
        )}
      </div>
    </FormLayout>
  );
}
