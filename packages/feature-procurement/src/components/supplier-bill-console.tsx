"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import {ProcurementEditor} from "@erpx/feature-procurement/components/procurement-editor";
import {useSupplierBillCommand} from "@erpx/feature-procurement/components/use-supplier-bill-command";
import type {SupplierBillPurchase} from "@erpx/feature-procurement/lib/supplier-bill-command";
import { Button, Field, Select, Table, TextArea, TextInput } from "@erpx/ui/components/foundation";

import {useState} from "react";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { supplierBillDetail, supplierInvoicePolicy } from "@erpx/server-procurement/public/supplier-bill-matching";
import { Card, FormGrid, TableFrame } from "@erpx/ui/components/foundation";
function BillCommandState({command}:{command:ReturnType<typeof useSupplierBillCommand>}){return <>{command.error&&<InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}{command.notice&&<InlineNotice>{command.notice}</InlineNotice>}{command.uncertain&&<Button appearance="standard" type="button" disabled={command.busy} onClick={()=>void command.retry()}>Retry submitted supplier invoice change</Button>}{command.savedHref&&command.notice&&<Link href={command.savedHref}>Open saved supplier invoice</Link>}</>;}
export function SupplierInvoicePolicy({
  base,
  policy,
}: {
  base: string;
  policy: Awaited<ReturnType<typeof supplierInvoicePolicy>>;
}) {
  const command=useSupplierBillCommand(base,policy.purchase,policy.companyId),{busy,ready}=command,[revision,setRevision]=useState(0),[draft,setDraft]=useState({version:policy.version,matchingMode:policy.matchingMode});
  const reset=()=>{setDraft({version:policy.version,matchingMode:policy.matchingMode});setRevision(value=>value+1);};
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
      <BillCommandState command={command}/>
      {policy.canManage && (
        <ProcurementEditor title="Change supplier invoice matching policy" busy={busy} error={command.error} disabled={policy.purchase?.newWork===false||!ready} onBegin={reset} onDiscard={reset}><form key={revision}
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            void command.send("supplier-bills/policy",{
              matchingMode: form.get("matchingMode"),
              expectedVersion: draft.version,
              reason: form.get("reason"),
            },false,event.currentTarget);
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Required control
              <Select appearance="standard" name="matchingMode" defaultValue={draft.matchingMode}>
                <option value="required">Require independent matching</option>
                <option value="optional">Allow direct AP preparation</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Policy change reason
              <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
            </Field>
          </FormGrid>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !ready || policy.purchase?.newWork===false}>
            {busy ? "Saving…" : ready ? "Save company policy" : "Loading policy controls…"}
          </Button>
        </form></ProcurementEditor>
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
  fulfillments:initialFulfillments,
  lines:initialLines,
  currency,
  purchase,
}: {
  base: string;
  fulfillments: Row[];
  lines: Row[];
  currency: string;
  purchase?:SupplierBillPurchase;
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
  const [sources,setSources]=useState({fulfillments:initialFulfillments,lines:initialLines}),{fulfillments,lines}=sources,initial=fulfillments.length?fulfillments.filter(receipt=>compatible(fulfillments[0],receipt)):[];
  const command=useSupplierBillCommand(base,purchase),{busy,ready,send}=command,[revision,setRevision]=useState(0),[selected,setSelected]=useState(()=>new Set(initial.map(receipt=>String(receipt.id))));
  const reset=()=>{setSources({fulfillments:initialFulfillments,lines:initialLines});setSelected(new Set(initialFulfillments.filter(receipt=>compatible(initialFulfillments[0]??null,receipt)).map(receipt=>String(receipt.id))));setRevision(value=>value+1);};
  const receipts = fulfillments.filter((receipt) => selected.has(String(receipt.id))),
    anchor = receipts[0],
    receiptIds = new Set(receipts.map((receipt) => String(receipt.id))),
    items = lines.filter((line) => receiptIds.has(String(line.fulfillment_id)));
  const latest = receipts
    .map((receipt) => String(receipt.fulfillment_date))
    .sort()
    .at(-1);
  return (
    <ProcurementEditor title="Match supplier invoice to goods receipts" busy={busy} error={command.error} disabled={!ready||purchase?.settleExisting===false||!initialFulfillments.length} onBegin={reset} onDiscard={reset}>
      <p>
        Select every posted goods receipt covered by the supplier invoice. Receipts can come from
        multiple purchase orders when they share the same supplier, currency, branch and department.
        A different Finance reviewer must accept quantities and price differences before one AP
        invoice is prepared.
      </p>
      <BillCommandState command={command}/>
      <form key={revision}
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
          },false,event.currentTarget);
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
          disabled={busy || !ready || purchase?.settleExisting===false || !receipts.length || !items.length}
        >
          Send invoice for independent matching review
        </Button>
      </form>
    </ProcurementEditor>
  );
}
export function SupplierBillConsole({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof supplierBillDetail>>;
}) {
  const r = data.bill,command=useSupplierBillCommand(base,data.purchase,String(r.order_id)),{busy,ready,send}=command,[revision,setRevision]=useState(0),[reviewVersion,setReviewVersion]=useState(Number(r.version)),
    allowedReview=["review","approved","invoiced"].includes(String(r.status))&&data.purchase?.settleExisting!==false,
    reset=()=>{setReviewVersion(Number(r.version));setRevision(value=>value+1);},
    variance = Number(r.subtotal_minor) - Number(r.expected_subtotal_minor);
  const primary = data.receipts.find((receipt) => receipt.fulfillment_id === r.fulfillment_id);
  return (
    <>
      <BillCommandState command={command}/>
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
        <Link href={canonicalUiHref(`${base}/commerce/orders/${r.order_id}`)}>
          Source purchase order · {String(r.order_number)}
        </Link>
      </Card>
      {data.canReview && (
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
          <ProcurementEditor title="Review supplier invoice" busy={busy} error={command.error} disabled={!ready||!allowedReview} onBegin={reset} onDiscard={reset}><form key={revision}
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send(`supplier-bills/${r.id}`, {
                action: f.get("action"),
                expectedVersion: reviewVersion,
                reason: f.get("reason"),
                ...(f.get("account") ? { varianceAccountId: f.get("account") } : {}),
              },false,e.currentTarget);
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
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!ready||!allowedReview}>
              Record review
            </Button>
          </form></ProcurementEditor>
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
              disabled={busy||!ready||data.purchase?.settleExisting===false}
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
        <Link href={canonicalUiHref(`${base}/finance/invoices/${r.invoice_id}`)}>Open matched AP invoice</Link>
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
