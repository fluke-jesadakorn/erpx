"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, Table, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { RecordLink as Link } from "@/components/records/record-link";
import { workFetch } from "@/lib/work-fetch";
import { minorToDecimal } from "@/lib/finance-money";
import type { Row } from "@/server/commerce";
import type { supplierBillDetail, supplierInvoicePolicy } from "@/server/supplier-bill-matching";
import { Card, FormGrid, TableFrame } from "@/components/ui/foundation";
function useCommand(base: string) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  async function send(path: string, input: unknown, invoice = false) {
    if (busy) return;
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
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(body.error?.message ?? "Unable to save supplier invoice.");
      }
      last.current = null;
      router.push(
        invoice
          ? `${base}/finance/invoices/${body.data.invoiceId}`
          : `${base}/supplier-bills/${body.data.id}`,
      );
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the same command.");
    } finally {
      setBusy(false);
    }
  }
  return { busy, error, send };
}
export function SupplierInvoicePolicy({
  base,
  policy,
}: {
  base: string;
  policy: Awaited<ReturnType<typeof supplierInvoicePolicy>>;
}) {
  const router = useRouter(),
    [ready, setReady] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  useEffect(() => {
    const frame = requestAnimationFrame(() => setReady(true));
    return () => cancelAnimationFrame(frame);
  }, []);
  async function save(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/supplier-bills/policy`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: fingerprint,
        }),
        body = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(body.error?.message ?? "Unable to save supplier invoice policy.");
      }
      last.current = null;
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry the same policy change.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <div className="ui-section-header">
        <div>
          <h2>Supplier invoice matching policy</h2>
          <p className="muted">
            Current policy: <strong>{policy.matchingMode}</strong>
            {policy.updatedByName ? ` · changed by ${policy.updatedByName}` : ""}
          </p>
        </div>
        <span className="ui-badge">Version {policy.version}</span>
      </div>
      <p>
        {policy.matchingMode === "required"
          ? "Every purchase receipt must pass independent quantity and price matching before Finance can prepare AP."
          : "Finance may prepare AP directly from a posted receipt. Supplier matching remains available for invoices that need three-way review."}
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {policy.canManage && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            void save({
              matchingMode: form.get("matchingMode"),
              expectedVersion: policy.version,
              reason: form.get("reason"),
            });
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Required control
              <Select appearance="standard" name="matchingMode" defaultValue={policy.matchingMode}>
                <option value="required">Require independent matching</option>
                <option value="optional">Allow direct AP preparation</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Policy change reason
              <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
            </Field>
          </FormGrid>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !ready}>
            {busy ? "Saving…" : ready ? "Save company policy" : "Loading policy controls…"}
          </Button>
        </form>
      )}
      {policy.canManage && !!policy.events.length && (
        <details>
          <summary>Policy change history</summary>
          {policy.events.map((event) => (
            <p key={String(event.id)}>
              Version {String(event.version)} · {String(event.previous_mode)} →{" "}
              {String(event.matching_mode)} · {String(event.actor_name)} · {String(event.reason)}
            </p>
          ))}
        </details>
      )}
    </Card>
  );
}
export function SupplierBillCapture({
  base,
  fulfillments,
  lines,
  currency,
}: {
  base: string;
  fulfillments: Row[];
  lines: Row[];
  currency: string;
}) {
  const compatible = (left: Row | null, right: Row) => {
    if (!left) return true;
    const same = (key: string) =>
      left[key] === undefined ||
      right[key] === undefined ||
      String(left[key] ?? "") === String(right[key] ?? "");
    return (
      same("supplier_id") && same("order_currency") && same("branch_id") && same("department_id")
    );
  };
  const initial = fulfillments.length
    ? fulfillments.filter((receipt) => compatible(fulfillments[0], receipt))
    : [];
  const { busy, error, send } = useCommand(base),
    [selected, setSelected] = useState(() => new Set(initial.map((receipt) => String(receipt.id))));
  const receipts = fulfillments.filter((receipt) => selected.has(String(receipt.id))),
    anchor = receipts[0],
    receiptIds = new Set(receipts.map((receipt) => String(receipt.id))),
    items = lines.filter((line) => receiptIds.has(String(line.fulfillment_id)));
  const latest = receipts
    .map((receipt) => String(receipt.fulfillment_date))
    .sort()
    .at(-1);
  return (
    <FormDisclosure appearance="standard">
      <summary>
        Match one supplier invoice to{" "}
        {fulfillments.length > 1 ? "one or more receipts" : "this receipt"}
      </summary>
      <p>
        Select every posted goods receipt covered by the supplier invoice. Receipts can come from
        multiple purchase orders when they share the same supplier, currency, branch and department.
        A different Finance reviewer must accept quantities and price differences before one AP
        invoice is prepared.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          const form = new FormData(event.currentTarget);
          void send("supplier-bills", {
            receipts: receipts.map((receipt) => ({
              fulfillmentId: receipt.id,
              expectedVersion: Number(receipt.version),
            })),
            reference: form.get("reference"),
            invoiceDate: form.get("date"),
            dueDate: form.get("due"),
            evidence: form.get("evidence"),
            lines: items.map((line) => ({
              fulfillmentLineId: line.id,
              quantity: form.get(`quantity-${line.id}`),
              price: form.get(`price-${line.id}`),
              taxBps: Math.round(Number(form.get(`tax-${line.id}`)) * 100),
            })),
          });
        }}
      >
        <fieldset>
          <legend>Goods receipts on this invoice</legend>
          {fulfillments.map((receipt) => {
            const allowed = compatible(anchor ?? null, receipt);
            return (
              <Field appearance="standard" orientation="inline" key={String(receipt.id)}>
                <TextInput appearance="standard"
                  type="checkbox"
                  checked={selected.has(String(receipt.id))}
                  disabled={!allowed && !selected.has(String(receipt.id))}
                  onChange={(event) =>
                    setSelected((current) => {
                      const next = new Set(current);
                      if (event.target.checked) next.add(String(receipt.id));
                      else next.delete(String(receipt.id));
                      return next;
                    })
                  }
                />
                {receipt.order_number ? `PO ${String(receipt.order_number)} · ` : ""}
                {String(receipt.number)} · {String(receipt.fulfillment_date)}
                {receipt.supplier_name ? ` · ${String(receipt.supplier_name)}` : ""}
                {!allowed ? " · different supplier or scope" : ""}
              </Field>
            );
          })}
        </fieldset>
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Supplier invoice reference
            <TextInput appearance="standard" name="reference" required minLength={2} maxLength={160} />
          </Field>
          <Field appearance="standard">
            Invoice date
            <TextInput appearance="standard" name="date" type="date" required min={latest} />
          </Field>
          <Field appearance="standard">
            Payment due date
            <TextInput appearance="standard" name="due" type="date" required />
          </Field>
        </FormGrid>
        {items.map((line, index) => (
          <fieldset key={String(line.id)}>
            <legend>
              {String(line.description ?? `Receipt line ${index + 1}`)} · Received{" "}
              {Number(line.quantity_milli) / 1000}
            </legend>
            <FormGrid as="div" className="ui-form-grid">
              <Field appearance="standard">
                Invoiced quantity
                <TextInput appearance="standard"
                  name={`quantity-${line.id}`}
                  type="number"
                  min="0.001"
                  step="0.001"
                  defaultValue={Number(line.quantity_milli) / 1000}
                  required
                />
              </Field>
              <Field appearance="standard">
                Invoiced unit price ({currency})
                <TextInput appearance="standard"
                  name={`price-${line.id}`}
                  type="number"
                  min="0.01"
                  step="0.01"
                  defaultValue={minorToDecimal(
                    Math.round((Number(line.subtotal_minor) * 1000) / Number(line.quantity_milli)),
                  )}
                  required
                />
              </Field>
              <Field appearance="standard">
                Invoiced tax (%)
                <TextInput appearance="standard"
                  name={`tax-${line.id}`}
                  type="number"
                  min="0"
                  max="100"
                  step="0.01"
                  defaultValue={
                    Number(line.subtotal_minor)
                      ? Math.round((Number(line.tax_minor) / Number(line.subtotal_minor)) * 10000) /
                        100
                      : 0
                  }
                  required
                />
              </Field>
            </FormGrid>
          </fieldset>
        ))}
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Supplier invoice evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={2000} />
          </Field>
        </FormGrid>
        <Button appearance="standard"
          clientManaged type="submit"
          variant="primary"
          disabled={busy || !receipts.length || !items.length}
        >
          Send invoice for independent matching review
        </Button>
      </form>
    </FormDisclosure>
  );
}
export function SupplierBillConsole({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof supplierBillDetail>>;
}) {
  const { busy, error, send } = useCommand(base),
    r = data.bill,
    variance = Number(r.subtotal_minor) - Number(r.expected_subtotal_minor);
  const primary = data.receipts.find((receipt) => receipt.fulfillment_id === r.fulfillment_id);
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>
          {String(r.reference)} · {String(r.status)}
        </h2>
        <p>
          {String(r.supplier_name)} · {String(r.currency)} · Invoice {String(r.invoice_date)} · Due{" "}
          {String(r.due_date)}
        </p>
        <p>
          <strong>Matched goods receipts ({data.receipts.length})</strong>:{" "}
          {data.receipts
            .map(
              (receipt) =>
                `${receipt.order_number ? `PO ${String(receipt.order_number)} · ` : ""}${String(receipt.receipt_number)}`,
            )
            .join(", ")}
        </p>
        <p>{String(r.evidence)}</p>
        <TableFrame appearance="standard" label="Records"><Table className="business-table">
          <thead>
            <tr>
              <th>PO / receipt / item</th>
              <th>Received / invoiced</th>
              <th>PO subtotal</th>
              <th>Supplier subtotal</th>
              <th>Supplier tax</th>
            </tr>
          </thead>
          <tbody>
            {data.lines.map((l) => (
              <tr key={String(l.id)}>
                <td>
                  {l.order_number ? `PO ${String(l.order_number)} · ` : ""}
                  {String(l.receipt_number)} · {String(l.description)}
                </td>
                <td>
                  {Number(l.expected_quantity_milli) / 1000} / {Number(l.quantity_milli) / 1000}
                </td>
                <td>{minorToDecimal(Number(l.expected_subtotal_minor))}</td>
                <td>{minorToDecimal(Number(l.subtotal_minor))}</td>
                <td>{minorToDecimal(Number(l.tax_minor))}</td>
              </tr>
            ))}
          </tbody>
        </Table></TableFrame>
        <p>
          Purchase price variance {String(r.currency)} {minorToDecimal(variance)} · Invoice total{" "}
          {minorToDecimal(Number(r.subtotal_minor) + Number(r.tax_minor))}
        </p>
        {!Number(r.quantity_matches) && (
          <InlineNotice tone="danger" role="alert">
            Quantity differs from goods received. Return the invoice for correction before preparing
            AP.
          </InlineNotice>
        )}
        <Link href={`${base}/commerce/orders/${r.order_id}`}>
          Source purchase order · {String(r.order_number)}
        </Link>
      </Card>
      {["review", "approved", "invoiced"].includes(String(r.status)) && data.canReview && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Independent Finance review</h2>
          {r.status === "invoiced" && (
            <p>
              Returning this invoice voids its unposted AP draft and releases all matched receipts
              for a corrected invoice. The original draft and review remain in history; stock and
              posted goods GL stay unchanged.
            </p>
          )}
          <p>
            Receipt stock retains its original cost. Accepting a price difference records that
            difference as expense in the account you select. Use the Company’s approved accounting
            policy.
          </p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send(`supplier-bills/${r.id}`, {
                action: f.get("action"),
                expectedVersion: Number(r.version),
                reason: f.get("reason"),
                ...(f.get("account") ? { varianceAccountId: f.get("account") } : {}),
              });
            }}
          >
            <FormGrid as="div" className="ui-form-grid">
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="action">
                  {r.status === "review" && Number(r.quantity_matches) > 0 && (
                    <option value="approve">Accept matched invoice</option>
                  )}
                  <option value="return">Return for supplier correction</option>
                </Select>
              </Field>
              {data.lines.some(
                (l) => Number(l.subtotal_minor) !== Number(l.expected_subtotal_minor),
              ) && (
                <Field appearance="standard">
                  Price variance expense account
                  <Select appearance="standard" name="account">
                    <option value="">Choose Company-approved account</option>
                    {data.accounts.map((a) => (
                      <option key={String(a.id)} value={String(a.id)}>
                        {String(a.code)} · {String(a.name)}
                      </option>
                    ))}
                  </Select>
                </Field>
              )}
              <Field appearance="standard">
                Review reason
                <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
              </Field>
            </FormGrid>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record review
            </Button>
          </form>
        </Card>
      )}
      {r.status === "approved" && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Prepare AP invoice</h2>
          <p>
            The assigned purchase order invoicing owner prepares one AP invoice from all approved
            receipts, quantities, amounts and dates. Independent invoice posting remains required.
          </p>
          {data.canPrepare && primary && (
            <Button appearance="standard"
              type="submit"
              variant="primary"
              disabled={busy}
              onClick={() =>
                void send(
                  `commerce/fulfillments/${r.fulfillment_id}/invoice`,
                  {
                    expectedVersion: Number(primary.current_version),
                    date: r.invoice_date,
                    dueDate: r.due_date,
                    matchId: r.id,
                    matchVersion: Number(r.version),
                  },
                  true,
                )
              }
            >
              Prepare matched AP invoice
            </Button>
          )}
        </Card>
      )}
      {r.invoice_id && (
        <Link href={`${base}/finance/invoices/${r.invoice_id}`}>Open matched AP invoice</Link>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Review history</h2>
        {data.events.map((e) => (
          <p key={String(e.id)}>
            Version {String(e.version)} · {String(e.action)} · {String(e.reason)}
          </p>
        ))}
        {r.status === "returned" && (
          <p>
            Capture a corrected invoice from the source purchase order. This review remains in
            history.
          </p>
        )}
      </Card>
    </>
  );
}
