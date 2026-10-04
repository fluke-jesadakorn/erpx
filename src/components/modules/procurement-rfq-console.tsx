"use client";
import { InlineNotice } from "@/components/ui/foundation";

import {
  Button,
  Field,
  Select,
  Table,
  TableFrame,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { workFetch } from "@/lib/work-fetch";
import { minorToDecimal } from "@/lib/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import type { rfqDetail, rfqWorkspace } from "@/server/procurement-rfqs";
import { Card, FormGrid, FormLayout } from "@/components/ui/foundation";
type Workspace = Awaited<ReturnType<typeof rfqWorkspace>>;
type RequestSource = {
  id: string;
  version: number;
  title: string;
  description: string;
  neededDate: string | null;
  branchId: string | null;
  items: Array<{
    description: string;
    quantity: string;
    unit: string;
    requestedMilli: number;
    allocatedMilli: number;
    remainingMilli: number;
  }>;
};
type Detail = Awaited<ReturnType<typeof rfqDetail>>;
function useCommand(base: string) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  async function send(path: string, input: unknown, order = false) {
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
        throw new Error(body.error?.message ?? "Unable to save quotation request.");
      }
      last.current = null;
      router.push(`${base}/${order ? "commerce/orders" : "procurement-rfqs"}/${body.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the same command.");
    } finally {
      setBusy(false);
    }
  }
  return { busy, error, send };
}
export function RfqCreate({
  base,
  data,
  source,
}: {
  base: string;
  data: Workspace;
  source?: RequestSource;
}) {
  const { busy, error, send } = useCommand(base),
    [lines, setLines] = useState<
      Array<{
        productId: string;
        quantity: string;
        requestQuantity?: string;
        requestUnit?: string;
        requestUnitsPerProductUnit?: string;
        requestItemOrdinal?: number;
      }>
    >(
      () =>
        source?.items.flatMap((item, i) =>
          item.remainingMilli > 0
            ? [
                {
                  productId: "",
                  quantity: String(item.remainingMilli / 1000),
                  requestQuantity: String(item.remainingMilli / 1000),
                  requestUnit: item.unit,
                  requestUnitsPerProductUnit: "1",
                  requestItemOrdinal: i,
                },
              ]
            : [],
        ) ?? [],
    );
  if (!data.canPrepare) return null;
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Request supplier quotations</h2>
      <p>
        Record all or part of the remaining approved demand and compare complete offers in{" "}
        {data.currency}. This does not reserve stock or approve a purchase order.
      </p>
      {source && (
        <TableFrame appearance="standard" label="Records" className="business-table-wrap">
          <Table className="business-table">
            <thead>
              <tr>
                <th>Approved item</th>
                <th>Requested</th>
                <th>Already allocated</th>
                <th>Remaining</th>
              </tr>
            </thead>
            <tbody>
              {source.items.map((item, i) => (
                <tr key={i}>
                  <td>{item.description}</td>
                  <td>
                    {item.requestedMilli / 1000} {item.unit}
                  </td>
                  <td>
                    {item.allocatedMilli / 1000} {item.unit}
                  </td>
                  <td>
                    {item.remainingMilli / 1000} {item.unit}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
      )}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          void send("procurement-rfqs", {
            ...(source ? { requestId: source.id, requestVersion: source.version } : {}),
            title: f.get("title"),
            warehouseId: f.get("warehouse"),
            neededDate: f.get("needed"),
            demandEvidence: f.get("evidence"),
            lines,
          });
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Demand title
            <TextInput appearance="standard"
              name="title"
              defaultValue={source?.title}
              required
              minLength={3}
              maxLength={2000}
            />
          </Field>
          <Field appearance="standard">
            Receiving warehouse
            <Select appearance="standard" name="warehouse" required>
              <option value="">Choose warehouse</option>
              {data.warehouses
                .filter((w) => !source || w.branch_id === source.branchId)
                .map((w) => (
                  <option key={String(w.id)} value={String(w.id)}>
                    {String(w.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Required date
            <TextInput appearance="standard"
              name="needed"
              type="date"
              required
              readOnly={!!source?.neededDate}
              defaultValue={source?.neededDate ?? undefined}
            />
          </Field>
        </FormGrid>
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Demand source and justification
            <TextArea appearance="standard"
              name="evidence"
              defaultValue={source?.description}
              required
              minLength={3}
              maxLength={2000}
            />
          </Field>
        </FormGrid>
        {lines.map((l, i) => {
          const item =
              source && l.requestItemOrdinal !== undefined
                ? source.items[l.requestItemOrdinal]
                : null,
            product = data.products.find((p) => p.id === l.productId);
          return (
            <FormGrid as="div" className="ui-form-grid" key={`${l.requestItemOrdinal ?? "new"}-${i}`}>
              {item && (
                <p>
                  Approved item {Number(l.requestItemOrdinal) + 1}: {item.description} · remaining{" "}
                  {item.remainingMilli / 1000} {item.unit}
                </p>
              )}
              <Field appearance="standard">
                Demand product {i + 1}
                <Select appearance="standard"
                  required
                  value={l.productId}
                  onChange={(e) =>
                    setLines((prev) =>
                      prev.map((x, n) => (n === i ? { ...x, productId: e.target.value } : x)),
                    )
                  }
                >
                  <option value="">Map to product</option>
                  {data.products
                    .filter((p) => p.id === l.productId || !lines.some((x) => x.productId === p.id))
                    .map((p) => (
                      <option key={String(p.id)} value={String(p.id)}>
                        {String(p.sku)} · {String(p.name)} · {String(p.unit)}
                      </option>
                    ))}
                </Select>
              </Field>
              {item && (
                <Field appearance="standard">
                  Allocate request quantity ({item.unit})
                  <TextInput appearance="standard"
                    type="number"
                    min="0.001"
                    max={item.remainingMilli / 1000}
                    step="0.001"
                    required
                    value={l.requestQuantity}
                    onChange={(e) =>
                      setLines((prev) =>
                        prev.map((x, n) =>
                          n === i ? { ...x, requestQuantity: e.target.value } : x,
                        ),
                      )
                    }
                  />
                </Field>
              )}
              <Field appearance="standard">
                Purchase quantity {product ? `(${String(product.unit)})` : ""}
                <TextInput appearance="standard"
                  type="number"
                  min="0.001"
                  step="0.001"
                  required
                  value={l.quantity}
                  onChange={(e) =>
                    setLines((prev) =>
                      prev.map((x, n) => (n === i ? { ...x, quantity: e.target.value } : x)),
                    )
                  }
                />
              </Field>
              {item && (
                <Field appearance="standard">
                  {item.unit} per one {product ? String(product.unit) : "purchasing unit"}
                  <TextInput appearance="standard"
                    type="number"
                    min="0.001"
                    step="0.001"
                    required
                    value={l.requestUnitsPerProductUnit}
                    onChange={(e) =>
                      setLines((prev) =>
                        prev.map((x, n) =>
                          n === i ? { ...x, requestUnitsPerProductUnit: e.target.value } : x,
                        ),
                      )
                    }
                  />
                </Field>
              )}
              <Button appearance="standard"
                variant="secondary"
                type="button"
                onClick={() => setLines((prev) => prev.filter((_, n) => n !== i))}
              >
                Remove demand {i + 1}
              </Button>
            </FormGrid>
          );
        })}
        <div className="ui-form-actions">
          {!source && (
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={lines.length >= 20 || lines.length >= data.products.length}
              onClick={() => {
                const p = data.products.find((p) => !lines.some((l) => l.productId === p.id));
                if (p) setLines([...lines, { productId: String(p.id), quantity: "1" }]);
              }}
            >
              Add demand product
            </Button>
          )}
          <Button appearance="standard"
            clientManaged type="submit"
            variant="primary"
            disabled={busy || !lines.length || lines.some((l) => !l.productId)}
          >
            Create quotation request
          </Button>
        </div>
        {source && !lines.length && (
          <p>
            All approved demand is already allocated, or all remaining lines were removed. Cancel an
            unused RFQ to release its allocation.
          </p>
        )}
      </form>
    </Card>
  );
}
export function RfqConsole({
  base,
  data,
  suppliers,
}: {
  base: string;
  data: Detail;
  suppliers: Workspace["suppliers"];
}) {
  const { busy, error, send } = useCommand(base),
    [offerAttachmentSet, setOfferAttachmentSet] = useState<AttachmentPanelChange | null>(null),
    r = data.rfq,
    path = `procurement-rfqs/${r.id}`,
    selected = data.offers.find((o) => o.id === r.selected_offer_id);
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data.sourceBlocked && (
        <InlineNotice tone="danger" role="alert">
          The source requisition changed or is not ready. Resolve it with its assigned team or
          cancel this RFQ before continuing.
        </InlineNotice>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Demand and comparison</h2>
        <p>{String(r.demand_evidence)}</p>
        <p>
          Required {String(r.needed_date)} · {String(r.currency)} · {String(r.status)}
        </p>
        <TableFrame appearance="standard" label="Records"><Table className="business-table">
          <thead>
            <tr>
              <th>Product demand</th>
              <th>Source allocation</th>
              <th>Conversion evidence</th>
            </tr>
          </thead>
          <tbody>
            {data.lines.map((l) => (
              <tr key={String(l.id)}>
                <td>
                  {String(l.description)} · {Number(l.quantity_milli) / 1000} {String(l.unit)}
                </td>
                <td>
                  {l.request_quantity_milli
                    ? `${Number(l.request_quantity_milli) / 1000} ${String(l.request_unit)}`
                    : "Standalone demand"}
                </td>
                <td>
                  {l.request_units_per_product_unit_milli
                    ? `${Number(l.request_units_per_product_unit_milli) / 1000} ${String(l.request_unit)} per ${String(l.unit)}`
                    : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </Table></TableFrame>
        <h3>Current offer from each supplier</h3>
        <TableFrame appearance="standard" label="Records" className="business-table-wrap">
          <Table className="business-table">
            <thead>
              <tr>
                <th>Supplier / reference</th>
                <th>Total ({String(r.currency)})</th>
                <th>Arrival</th>
                <th>Valid through</th>
                <th>Selection</th>
              </tr>
            </thead>
            <tbody>
              {data.current.map((o) => (
                <tr key={String(o.id)}>
                  <td>
                    {String(o.supplier_name)} · {String(o.reference)}
                  </td>
                  <td>{minorToDecimal(Number(o.subtotal_minor) + Number(o.tax_minor))}</td>
                  <td>
                    {String(o.arrival_date)}
                    {String(o.arrival_date) > String(r.needed_date) ? " · Later than required" : ""}
                  </td>
                  <td>{String(o.valid_until)}</td>
                  <td>{o.id === r.selected_offer_id ? "Selected for award" : "Not selected"}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
        {!data.current.length && <p>No supplier offers recorded.</p>}
        {r.selection_reason && <p>Selection reason: {String(r.selection_reason)}</p>}
      </Card>
      {data.canPrepare && r.status === "collecting" && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Record supplier offer</h2>
          <p>
            Enter every demand line. A new offer replaces this supplier’s current offer in the
            comparison and retains the earlier evidence.
          </p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send(path, {
                action: "offer",
                expectedVersion: Number(r.version),
                supplierId: f.get("supplier"),
                reference: f.get("reference"),
                evidence: f.get("evidence"),
                arrivalDate: f.get("arrival"),
                validUntil: f.get("valid"),
                attachmentSetIds:
                  offerAttachmentSet?.setId &&
                  offerAttachmentSet.readyCount > 0 &&
                  offerAttachmentSet.bindable
                    ? [offerAttachmentSet.setId]
                    : [],
                lines: data.lines.map((l) => ({
                  lineId: String(l.id),
                  price: String(f.get(`price-${l.id}`)),
                  taxBps: Math.round(Number(f.get(`tax-${l.id}`)) * 100),
                })),
              });
            }}
          >
            <FormGrid as="div" className="ui-form-grid">
              <Field appearance="standard">
                Supplier
                <Select appearance="standard" name="supplier" required>
                  <option value="">Select supplier</option>
                  {suppliers.map((s) => (
                    <option key={String(s.id)} value={String(s.id)}>
                      {String(s.name)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Quotation reference
                <TextInput appearance="standard" name="reference" required minLength={3} maxLength={2000} />
              </Field>
              <Field appearance="standard">
                Quoted arrival
                <TextInput appearance="standard" name="arrival" type="date" required />
              </Field>
              <Field appearance="standard">
                Quote valid through
                <TextInput appearance="standard" name="valid" type="date" required />
              </Field>
            </FormGrid>
            <FormGrid as="div" className="ui-form-grid">
              <Field appearance="standard">
                Quotation evidence and terms
                <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={2000} />
              </Field>
            </FormGrid>
            <AttachmentPanel appearance="standard"
              base={base}
              targetType="procurement_rfq"
              targetId={String(r.id)}
              expectedTargetVersion={Number(r.version)}
              componentKind="action"
              componentRef="supplier-offer"
              purposeKey="supplier_quotation"
              title="Supplier quotation files"
              description="Attach the supplier quotation, commercial terms, or supporting images for this offer."
              visibility="Purchasing and authorized independent award reviewers can access these files."
              onChange={setOfferAttachmentSet}
            />
            {data.lines.map((l, i) => (
              <FormGrid as="div" className="ui-form-grid" key={String(l.id)}>
                <p>
                  {String(l.description)} · {Number(l.quantity_milli) / 1000} {String(l.unit)}
                </p>
                <Field appearance="standard">
                  Offer unit price {i + 1} ({String(r.currency)})
                  <TextInput appearance="standard" name={`price-${l.id}`} type="number" min="0" step="0.01" required />
                </Field>
                <Field appearance="standard">
                  Offer tax {i + 1} (%)
                  <TextInput appearance="standard"
                    name={`tax-${l.id}`}
                    type="number"
                    min="0"
                    max="100"
                    step="0.01"
                    defaultValue="0"
                    required
                  />
                </Field>
              </FormGrid>
            ))}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record quotation
            </Button>
          </form>
        </Card>
      )}
      {data.canPrepare && !["ordered", "cancelled"].includes(String(r.status)) && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Award review</h2>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send(path, {
                action: f.get("action"),
                expectedVersion: Number(r.version),
                ...(r.status === "collecting" ? { offerId: f.get("offer") } : {}),
                reason: f.get("reason"),
              });
            }}
          >
            <FormGrid as="div" className="ui-form-grid">
              {r.status === "collecting" && (
                <Field appearance="standard">
                  Proposed offer
                  <Select appearance="standard" name="offer">
                    {data.current.map((o) => (
                      <option key={String(o.id)} value={String(o.id)}>
                        {String(o.supplier_name)} · {String(o.reference)}
                      </option>
                    ))}
                  </Select>
                </Field>
              )}
              <Field appearance="standard">
                Action
                <Select appearance="standard" name="action">
                  {r.status === "collecting" && data.current.length > 0 && (
                    <option value="submit">Submit award for independent review</option>
                  )}
                  {r.status === "review" && data.canApprove && (
                    <option value="approve">Approve supplier award</option>
                  )}
                  {data.canApprove && ["review", "approved"].includes(String(r.status)) && (
                    <option value="return">Return to collect revised offers</option>
                  )}
                  <option value="cancel">Cancel quotation request</option>
                </Select>
              </Field>
              <Field appearance="standard">
                Decision and delivery exception reason
                <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
              </Field>
            </FormGrid>
            {r.status === "review" && !data.canApprove && (
              <p>
                A different purchasing manager must approve the creator, proposer and selected offer
                recorder.
              </p>
            )}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record decision
            </Button>
          </form>
        </Card>
      )}
      {data.canPrepare &&
        r.status === "approved" &&
        r.request_id &&
        !data.currentRequestVersion && (
          <p>
            The assigned requisition handler must create this purchase order after the source
            request is still approved and in progress.
          </p>
        )}
      {data.canPrepare &&
        r.status === "approved" &&
        selected &&
        (!r.request_id || data.currentRequestVersion) && (
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Create purchase order</h2>
            <p>
              The draft uses the approved supplier, warehouse, required date, quantities, prices and
              taxes. The PO requires its own independent approval.
            </p>
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send(
                  "commerce/orders",
                  {
                    kind: "purchase",
                    rfqId: r.id,
                    rfqVersion: Number(r.version),
                    ...(r.request_id
                      ? { requestId: r.request_id, requestVersion: data.currentRequestVersion }
                      : {}),
                    partyId: selected.supplier_id,
                    warehouseId: r.warehouse_id,
                    orderDate: f.get("date"),
                    dueDate: r.needed_date,
                    note: `RFQ ${r.id}: ${r.title}`,
                    lines: data.lines.map((l) => {
                      const offer = data.offerLines.find(
                        (o) => o.offer_id === selected.id && o.rfq_line_id === l.id,
                      )!;
                      return {
                        productId: l.product_id,
                        quantity: String(Number(l.quantity_milli) / 1000),
                        price: minorToDecimal(Number(offer.price_minor)),
                        taxBps: Number(offer.tax_bps),
                      };
                    }),
                  },
                  true,
                );
              }}
            >
              <Field appearance="standard">
                Purchase order date
                <TextInput appearance="standard"
                  name="date"
                  type="date"
                  required
                  defaultValue={new Date().toISOString().slice(0, 10)}
                  max={String(selected.valid_until)}
                />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Create awarded draft PO
              </Button>
            </FormLayout>
          </Card>
        )}
      {r.order_id && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Purchase order handoff</h2>
          <Link href={`${base}/commerce/orders/${r.order_id}`}>
            Open source-linked purchase order
          </Link>
          <p>
            Approval, receiving, supplier promises and accounting continue in the purchase order.
          </p>
        </Card>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Supplier evidence history</h2>
        {data.offers.map((o) => (
          <details key={String(o.id)}>
            <summary>
              {String(o.supplier_name)} · {String(o.reference)} · recorded at RFQ version{" "}
              {String(o.rfq_version)}
            </summary>
            <p>{String(o.evidence)}</p>
            {data.offerLines
              .filter((l) => l.offer_id === o.id)
              .map((l) => (
                <p key={String(l.id)}>
                  {String(data.lines.find((d) => d.id === l.rfq_line_id)?.description)} · unit price{" "}
                  {String(r.currency)} {minorToDecimal(Number(l.price_minor))} · tax{" "}
                  {Number(l.tax_bps) / 100}%
                </p>
              ))}
          </details>
        ))}
        <h3>Decision history</h3>
        {data.events.map((e) => (
          <p key={String(e.id)}>
            Version {String(e.version)} · {String(e.action)} · {String(e.evidence)}
          </p>
        ))}
      </Card>
    </>
  );
}
