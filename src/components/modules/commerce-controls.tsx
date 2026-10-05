"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { useEmbeddedRecord } from "@/components/records/record-context";
import { hubResultHref } from "@/lib/hub-result";
import { recordTarget, recordDrawerHref } from "@/lib/record-drawer";
import { Dialog, FormDrawer } from "@/components/ui/dialog";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/time/time-preference-context";
import { PreferredDateTimeField } from "@/components/time/preferred-date-time-field";

import { workFetch } from "@/lib/work-fetch";
import { InvoiceSlipUpload, type InvoiceSlipSelection } from "./invoice-slip-upload";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import type { Row } from "@/server/commerce";
import { minorToDecimal } from "@/lib/finance-money";
import { businessDate } from "@/lib/pos";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { Card, Stack, FormGrid, FormLayout } from "@/components/ui/foundation";
type Options = {
  products: Row[];
  parties: Row[];
  warehouses: Row[];
  nodes: Row[];
  currency: string;
  priceLists: Row[];
  priceListItems: Row[];
};
function useCommand(base: string) {
  const router = useRouter();
  const embedded = useEmbeddedRecord();
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const last = useRef<{ payload: string; key: string } | null>(null);
  async function send(path: string, input: unknown, target?: string) {
    const form=document.activeElement?.closest('form')??null;
    const body = JSON.stringify(input);
    const fingerprint = path + body;
    if (last.current?.payload !== fingerprint)
      last.current = { payload: fingerprint, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const r = await workFetch(
        `${base.replace("/holdings/", "/api/v1/holdings/")}/commerce/${path}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body,
        },
      );
      const v = (await r.json()) as {
        data: { id: string; salesId?: string; orderId?: string };
        error?: { message: string };
      };
      if (!r.ok) throw new Error(v.error?.message ?? "Unable to save.");
      last.current = null;
      const destination = target ?? `${base}/commerce/orders/${v.data.salesId ?? v.data.orderId ?? v.data.id}`;
      const record = recordTarget(destination, base);
      const hubResult = hubResultHref(window.location.href, base, destination);
      window.dispatchEvent(new Event("erpx:hub-saved"));
      window.dispatchEvent(new CustomEvent("erpx:record-saved",{detail:{form}}));
      if (hubResult) router.replace(hubResult, { scroll: false });
      else if (embedded && record) router.replace(recordDrawerHref(window.location.href, record), { scroll: false });
      else if (!embedded) router.push(destination);
      router.refresh();
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save.");
      return false;
    } finally {
      setPending(false);
    }
  }
  return { send, pending, error };
}
export function OrderEditor({
  base,
  options,
  kind,
  order,
  lines,
  sourceRequest,
  initialPartyId,
  initialOpen = false,
}: {
  base: string;
  options: Options;
  kind: string;
  initialPartyId?: string;
  initialOpen?: boolean;
  order?: Row;
  lines?: Row[];
  sourceRequest?: { id: string; version: number; neededDate: string | null; note: string };
}) {
  const [open, setOpen] = useState(!order || initialOpen);
  const { send, pending, error } = useCommand(base);
  const [items, setItems] = useState(
    (lines ?? []).map((l) => ({
      productId: String(l.product_id),
      quantity: String(Number(l.quantity_milli) / 1000),
      price: minorToDecimal(Number(l.price_minor)),
      taxBps: Number(l.tax_bps),
    })),
  );
  const [partyId, setPartyId] = useState(String(order?.party_id ?? initialPartyId ?? "")),
    [priceListId, setPriceListId] = useState(String(order?.price_list_id ?? ""));
  const eligiblePriceLists = options.priceLists.filter(
    (list) => !list.customer_id || list.customer_id === partyId,
  );
  function choosePriceList(id: string) {
    setPriceListId(id);
    if (!id) return;
    const approved = options.priceListItems.filter((item) => item.price_list_id === id);
    setItems(
      items.map((line) => {
        const price = approved.find((item) => item.product_id === line.productId);
        return price
          ? {
              ...line,
              price: minorToDecimal(Number(price.price_minor)),
              taxBps: Number(price.tax_bps),
            }
          : line;
      }),
    );
  }
  const add = () => {
    const p = options.products.find(
      (p) => p.status === "active" && !items.some((l) => l.productId === p.id),
    );
    if (p) {
      const approved = options.priceListItems.find(
        (item) => item.price_list_id === priceListId && item.product_id === p.id,
      );
      setItems([
        ...items,
        {
          productId: String(p.id),
          quantity: "1",
          price: minorToDecimal(Number(approved?.price_minor ?? p.price_minor)),
          taxBps: Number(approved?.tax_bps ?? p.tax_bps),
        },
      ]);
    }
  };
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      {!initialOpen && <div className="ui-section-header">
        <h2>
          {order
            ? "Edit draft"
            : `New ${kind === "quote" ? "quotation" : kind === "sales" ? "sales order" : "purchase order"}`}
        </h2>
        {order && (
          <Button appearance="standard" type="submit" variant="secondary" onClick={() => setOpen(!open)}>
            {open ? "Close" : "Edit draft"}
          </Button>
        )}
      </div>}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {open && (
        <FormDrawer appearance="standard" inline={!order || initialOpen} open={open} onClose={() => setOpen(false)} title="Order details" dismissDisabled={pending}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<form
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`orders${order ? `/${order.id}` : ""}`, {
              kind,
              partyId: f.get("partyId"),
              warehouseId: f.get("warehouseId"),
              orderDate: f.get("orderDate"),
              dueDate: f.get("dueDate"),
              note: f.get("note"),
              lines: items,
              ...(kind !== "purchase"
                ? {
                    ...(priceListId ? { priceListId } : {}),
                    discountReason: f.get("discountReason"),
                  }
                : {}),
              ...(sourceRequest
                ? { requestId: sourceRequest.id, requestVersion: sourceRequest.version }
                : {}),
              ...(order ? { expectedVersion: order.version } : {}),
            });
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              {kind === "purchase" ? "Supplier" : "Customer"}
              <Select appearance="standard"
                name="partyId"
                required
                value={partyId}
                onChange={(event) => {
                  setPartyId(event.target.value);
                  setPriceListId("");
                }}
              >
                <option value="">Select {kind === "purchase" ? "supplier" : "customer"}</option>
                {options.parties
                  .filter(
                    (p) =>
                      p.kind === (kind === "purchase" ? "supplier" : "customer") &&
                      p.status === "active" &&
                      p.currency === options.currency,
                  )
                  .map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {String(p.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            {kind !== "purchase" && (
              <>
                <Field appearance="standard">
                  Approved price list
                  <Select appearance="standard"
                    value={priceListId}
                    onChange={(event) => choosePriceList(event.target.value)}
                  >
                    <option value="">Automatic active price list / standard product price</option>
                    {eligiblePriceLists.map((list) => (
                      <option key={String(list.id)} value={String(list.id)}>
                        {String(list.name)} · {String(list.valid_from)} to{" "}
                        {String(list.valid_until)}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Discount reason
                  <TextInput appearance="standard"
                    name="discountReason"
                    minLength={5}
                    maxLength={1000}
                    defaultValue={String(order?.discount_reason ?? "")}
                  />
                </Field>
              </>
            )}
            <Field appearance="standard">
              Warehouse
              <Select appearance="standard" name="warehouseId" required defaultValue={String(order?.warehouse_id ?? "")}>
                <option value="">Select warehouse</option>
                {options.warehouses
                  .filter((w) => w.status === "active")
                  .map((w) => (
                    <option key={String(w.id)} value={String(w.id)}>
                      {String(w.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Order date
              <TextInput appearance="standard"
                name="orderDate"
                type="date"
                required
                defaultValue={String(order?.order_date ?? businessDate())}
              />
            </Field>
            <Field appearance="standard">
              Required date
              <TextInput appearance="standard"
                name="dueDate"
                type="date"
                required
                defaultValue={String(
                  order?.due_date ?? sourceRequest?.neededDate ?? businessDate(),
                )}
              />
            </Field>
            <Field appearance="standard">
              Order notes
              <TextInput appearance="standard"
                name="note"
                maxLength={2000}
                defaultValue={String(order?.note ?? sourceRequest?.note ?? "")}
              />
            </Field>
          </FormGrid>
          {items.map((line, index) => (
            <FormGrid as="div" className="ui-form-grid" key={index}>
              <Field appearance="standard">
                Product {index + 1}
                <Select appearance="standard"
                  value={line.productId}
                  onChange={(e) => {
                    const p = options.products.find((p) => p.id === e.target.value)!,
                      approved = options.priceListItems.find(
                        (item) => item.price_list_id === priceListId && item.product_id === p.id,
                      );
                    setItems(
                      items.map((l, i) =>
                        i === index
                          ? {
                              ...l,
                              productId: String(p.id),
                              price: minorToDecimal(Number(approved?.price_minor ?? p.price_minor)),
                              taxBps: Number(approved?.tax_bps ?? p.tax_bps),
                            }
                          : l,
                      ),
                    );
                  }}
                >
                  {options.products
                    .filter((p) => p.status === "active")
                    .map((p) => (
                      <option key={String(p.id)} value={String(p.id)}>
                        {String(p.sku)} · {String(p.name)}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Quantity {index + 1}
                <TextInput appearance="standard"
                  type="number"
                  min="0.001"
                  step="0.001"
                  required
                  value={line.quantity}
                  onChange={(e) =>
                    setItems(
                      items.map((l, i) => (i === index ? { ...l, quantity: e.target.value } : l)),
                    )
                  }
                />
              </Field>
              <Field appearance="standard">
                Unit price {index + 1} ({options.currency})
                <TextInput appearance="standard"
                  type="number"
                  min="0"
                  step="0.01"
                  required
                  value={line.price}
                  onChange={(e) =>
                    setItems(
                      items.map((l, i) => (i === index ? { ...l, price: e.target.value } : l)),
                    )
                  }
                />
              </Field>
              <Field appearance="standard">
                Tax rate {index + 1} (%)
                <TextInput appearance="standard"
                  type="number"
                  min="0"
                  max="100"
                  step="0.01"
                  required
                  value={line.taxBps / 100}
                  onChange={(e) =>
                    setItems(
                      items.map((l, i) =>
                        i === index
                          ? { ...l, taxBps: Math.round(Number(e.target.value) * 100) }
                          : l,
                      ),
                    )
                  }
                />
              </Field>
              <Button appearance="standard"
                variant="secondary"
                type="button"
                onClick={() => setItems(items.filter((_, i) => i !== index))}
              >
                Remove line {index + 1}
              </Button>
            </FormGrid>
          ))}
          <div className="ui-form-actions">
            <Button appearance="standard" variant="secondary" type="button" onClick={add} disabled={items.length >= 20}>
              Add product
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || !items.length}>
              {pending ? "Saving…" : "Save draft"}
            </Button>
            <span className="muted">
              Review the tax treatment and agreed prices before submission.
            </span>
          </div>
        </form></Stack></FormDrawer>
      )}
    </Card>
  );
}
export function OrderActions({
  base,
  order,
  canApprove,
}: {
  base: string;
  order: Row;
  canApprove: boolean;
}) {
  const { send, pending, error } = useCommand(base);
  const states =
    order.state === "draft"
      ? ["submit", "cancel"]
      : order.state === "submitted"
        ? canApprove
          ? ["approve", "return"]
          : ["return"]
        : order.state === "confirmed"
          ? order.kind === "quote"
            ? ["convert", ...(canApprove ? ["cancel"] : [])]
            : canApprove
              ? ["cancel"]
              : []
          : [];
  const [reason, setReason] = useState("");
  return (
    <div>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {order.state === "submitted" &&
        canApprove &&
        Number(order.discount_approval_required) > 0 && (
          <Field appearance="standard">
            Independent discount approval reason
            <TextInput appearance="standard"
              value={reason}
              minLength={5}
              required
              onChange={(event) => setReason(event.target.value)}
            />
          </Field>
        )}
      <div className="ui-form-actions">
        {states.map((action) => (
          <Button appearance="standard"
            type="submit"
            variant="secondary"
            key={action}
            disabled={
              pending ||
              (action === "approve" &&
                Number(order.discount_approval_required) > 0 &&
                reason.trim().length < 5)
            }
            onClick={() =>
              void send(`orders/${order.id}/transition`, {
                action,
                expectedVersion: order.version,
                ...(action === "approve" && reason ? { reason } : {}),
              })
            }
          >
            {
              (
                {
                  submit: "Submit for approval",
                  cancel: order.state === "confirmed" ? "Cancel approved order" : "Cancel draft",
                  approve:
                    Number(order.discount_approval_required) > 0
                      ? "Approve order and discount"
                      : "Approve order",
                  return: "Return to draft",
                  convert: "Create sales order",
                } as Record<string, string>
              )[action]
            }
          </Button>
        ))}
      </div>
    </div>
  );
}
type AllocationDraft = {
  key: string;
  lotId?: string;
  lotCode?: string;
  serialNumber?: string;
  quantity: string;
  manufacturedDate?: string;
  expiryDate?: string;
  disposition: "available" | "quarantine";
  quarantineReason?: string;
  quarantineDueDate?: string;
};
function DeliveryEvidenceUpload({
  base,
  order,
  onUploaded,
}: {
  base: string;
  order: Row;
  onUploaded: (value: { sourceId: string; setId: string } | null) => void;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [name, setName] = useState("");
  async function upload(file: File) {
    setBusy(true);
    setError("");
    onUploaded(null);
    try {
      const reserved = await workFetch(`/api/v1${base}/uploads`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
          body: JSON.stringify({
            ownerType: "commerce_order_delivery",
            ownerId: order.id,
            expectedVersion: Number(order.version),
            purpose: "proof_of_delivery",
            filename: file.name,
            mediaType: file.type,
            bytes: file.size,
          }),
        }),
        intent = await reserved.json();
      if (!reserved.ok)
        throw new Error(intent.error?.message ?? "Unable to reserve delivery evidence.");
      const stored = await workFetch(intent.data.uploadUrl, {
          method: "PUT",
          headers: { "Content-Type": file.type },
          body: file,
        }),
        result = await stored.json();
      if (!stored.ok)
        throw new Error(result.error?.message ?? "Unable to upload delivery evidence.");
      onUploaded({
        sourceId: String(result.data.id),
        setId: String(result.data.attachmentSetId ?? intent.data.attachmentSetId),
      });
      setName(file.name);
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry the delivery evidence upload.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Field appearance="standard">
      Proof of delivery
      <TextInput appearance="standard"
        type="file"
        accept="image/jpeg,image/png,image/webp,application/pdf"
        disabled={busy}
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) void upload(file);
        }}
      />
      {busy
        ? "Uploading…"
        : name
          ? `Attached: ${name}`
          : "Attach a signed note, receiving photo, or carrier document."}
      {error && <span role="alert">{error}</span>}
    </Field>
  );
}
export function FulfillmentForm({
  base,
  order,
  lines,
  lots,
  pendingEvidence = [],
}: {
  base: string;
  order: Row;
  lines: Row[];
  lots: Row[];
  pendingEvidence?: Row[];
}) {
  const { send, pending, error } = useCommand(base),
    incoming = order.kind === "purchase";
  const openLines = lines.filter(
    (line) => Number(line.fulfilled_milli) < Number(line.quantity_milli),
  );
  const [quantities, setQuantities] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      openLines.map((line) => [
        String(line.id),
        String((Number(line.quantity_milli) - Number(line.fulfilled_milli)) / 1000),
      ]),
    ),
  );
  const [allocations, setAllocations] = useState<Record<string, AllocationDraft[]>>({});
  const [evidenceId, setEvidenceId] = useState(() =>
    String(pendingEvidence[0]?.attachment_id ?? ""),
  );
  const [stagedEvidence, setStagedEvidence] = useState<{ sourceId: string; setId: string } | null>(
    null,
  );
  const [attachmentsV2, setAttachmentsV2] = useState<boolean | null>(null);
  const [attachmentSets, setAttachmentSets] = useState<Record<string, AttachmentPanelChange>>({});
  function newAllocation(line: Row): AllocationDraft {
    const serial = line.tracking_mode === "serial";
    return { key: crypto.randomUUID(), quantity: serial ? "1" : "", disposition: "available" };
  }
  function lineAllocations(line: Row) {
    return allocations[String(line.id)] ?? [];
  }
  function addAllocation(line: Row) {
    setAllocations((value) => ({
      ...value,
      [String(line.id)]: [...(value[String(line.id)] ?? []), newAllocation(line)],
    }));
  }
  function changeAllocation(line: Row, key: string, change: Partial<AllocationDraft>) {
    setAllocations((value) => ({
      ...value,
      [String(line.id)]: (value[String(line.id)] ?? []).map((item) =>
        item.key === key ? { ...item, ...change } : item,
      ),
    }));
  }
  function removeAllocation(line: Row, key: string) {
    setAllocations((value) => ({
      ...value,
      [String(line.id)]: (value[String(line.id)] ?? []).filter((item) => item.key !== key),
    }));
  }
  function payloadLines() {
    return openLines
      .map((line) => {
        const tracked = line.tracking_mode !== "none",
          drafts = lineAllocations(line);
        return {
          lineId: line.id,
          quantity: tracked
            ? String(drafts.reduce((sum, item) => sum + Number(item.quantity || 0), 0))
            : quantities[String(line.id)],
          ...(tracked
            ? {
                allocations: drafts.map((draft) =>
                  Object.fromEntries(
                    Object.entries(draft).filter(
                      ([field, value]) => field !== "key" && value !== "" && value !== undefined,
                    ),
                  ),
                ),
              }
            : {}),
        };
      })
      .filter((line) => Number(line.quantity) > 0);
  }
  const selectedSetIds = [
    ...new Set([
      ...Object.values(attachmentSets)
        .filter((item) => item.setId && item.readyCount > 0 && item.bindable)
        .map((item) => item.setId!),
      ...(stagedEvidence ? [stagedEvidence.setId] : []),
    ]),
  ];
  const proof = attachmentSets.proof_of_delivery;
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{incoming ? "Receive goods" : "Deliver goods"}</h2>
      <p className="muted">
        Record actual quantities and their traceability evidence. Stock, lot balances, quality
        holds, and accounting update in one transaction.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          const form = new FormData(event.currentTarget);
          void send(`orders/${order.id}/fulfill`, {
            expectedVersion: order.version,
            date: form.get("date"),
            evidenceIds: incoming || stagedEvidence || attachmentsV2 === true ? [] : [evidenceId],
            attachmentSetIds: selectedSetIds,
            evidenceNote: incoming ? "" : form.get("evidenceNote"),
            lines: payloadLines(),
          });
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Movement date
            <TextInput appearance="standard" name="date" type="date" required defaultValue={businessDate()} />
          </Field>
          {!incoming && attachmentsV2 !== true && (
            <>
              <DeliveryEvidenceUpload
                base={base}
                order={order}
                onUploaded={(value) => {
                  setStagedEvidence(value);
                  setEvidenceId(value?.sourceId ?? "");
                }}
              />
              {pendingEvidence.length > 0 && (
                <Field appearance="standard">
                  Available uploaded evidence
                  <Select appearance="standard"
                    value={evidenceId}
                    onChange={(event) => {
                      setStagedEvidence(null);
                      setEvidenceId(event.target.value);
                    }}
                    required
                  >
                    <option value="">Select proof of delivery</option>
                    {pendingEvidence.map((evidence) => (
                      <option
                        key={String(evidence.attachment_id)}
                        value={String(evidence.attachment_id)}
                      >
                        {String(evidence.original_name)} · {Number(evidence.bytes)} bytes
                      </option>
                    ))}
                  </Select>
                  <span className="muted">
                    Uploads remain available here after a reload until they are linked to a
                    delivery.
                  </span>
                </Field>
              )}
            </>
          )}
          {!incoming && (
            <Field appearance="standard">
              Receipt confirmation note
              <TextInput appearance="standard"
                name="evidenceNote"
                required
                minLength={3}
                maxLength={1000}
                placeholder="Recipient, carrier reference, or receiving confirmation"
              />
            </Field>
          )}
        </FormGrid>
        {incoming ? (
          (["delivery_note", "packing_list", "damage_photo"] as const).map((purpose, index) => (
            <AttachmentPanel appearance="standard"
              key={purpose}
              base={base}
              targetType="purchase_order"
              targetId={String(order.id)}
              expectedTargetVersion={Number(order.version)}
              componentKind="action"
              componentRef="goods-receipt"
              purposeKey={purpose}
              compact
              title={
                (
                  {
                    delivery_note: "Delivery notes",
                    packing_list: "Packing lists",
                    damage_photo: "Damage photos",
                  } as const
                )[purpose]
              }
              description={
                (
                  {
                    delivery_note: "Attach the supplier delivery note for this receipt.",
                    packing_list: "Attach packing lists or package details for this receipt.",
                    damage_photo: "Attach receiving-condition or damage photos when needed.",
                  } as const
                )[purpose]
              }
              visibility="Purchasing, receiving, Finance, and authorized Quality reviewers can access these files."
              onAvailabilityChange={index === 0 ? setAttachmentsV2 : undefined}
              onChange={(change) =>
                setAttachmentSets((current) => ({ ...current, [purpose]: change }))
              }
            />
          ))
        ) : (
          <AttachmentPanel appearance="standard"
            base={base}
            targetType="commerce_order_delivery"
            targetId={String(order.id)}
            expectedTargetVersion={Number(order.version)}
            componentKind="action"
            componentRef="delivery"
            purposeKey="proof_of_delivery"
            title="Proof of delivery"
            description="Attach a signed receiving note, delivery photo, or carrier document. One clean file is required."
            visibility="Sales, warehouse staff, and authorized reviewers for this order can access these files."
            requiredCount={1}
            onAvailabilityChange={setAttachmentsV2}
            onChange={(change) =>
              setAttachmentSets((current) => ({ ...current, proof_of_delivery: change }))
            }
          />
        )}
        {openLines.map((line) => {
          const tracked = line.tracking_mode !== "none",
            availableLots = lots.filter(
              (lot) =>
                lot.product_id === line.product_id &&
                Number(lot.quantity_milli) > 0 &&
                !Number(lot.quality_held),
            );
          return (
            <fieldset className="ui-panel" key={String(line.id)}>
              <legend>
                <strong>
                  {String(line.sku)} · {String(line.description)}
                </strong>
              </legend>
              <p className="muted">
                {(Number(line.quantity_milli) - Number(line.fulfilled_milli)) / 1000}{" "}
                {String(line.unit)} remaining ·{" "}
                {tracked
                  ? `${String(line.tracking_mode)} tracking required`
                  : "No traceability required"}
              </p>
              {!tracked && (
                <Field appearance="standard">
                  Quantity received / delivered
                  <TextInput appearance="standard"
                    aria-label={`${String(line.sku)} quantity`}
                    type="number"
                    min="0"
                    step="0.001"
                    max={(Number(line.quantity_milli) - Number(line.fulfilled_milli)) / 1000}
                    value={quantities[String(line.id)] ?? "0"}
                    onChange={(event) =>
                      setQuantities((value) => ({
                        ...value,
                        [String(line.id)]: event.target.value,
                      }))
                    }
                  />
                </Field>
              )}
              {tracked && (
                <>
                  {lineAllocations(line).map((allocation, index) => (
                    <FormGrid as="div" className="ui-form-grid" key={allocation.key}>
                      {incoming ? (
                        <Field appearance="standard">
                          {line.tracking_mode === "serial" ? "Serial number" : "Lot code"}
                          <TextInput appearance="standard"
                            required
                            value={String(
                              line.tracking_mode === "serial"
                                ? (allocation.serialNumber ?? "")
                                : (allocation.lotCode ?? ""),
                            )}
                            onChange={(event) =>
                              changeAllocation(
                                line,
                                allocation.key,
                                line.tracking_mode === "serial"
                                  ? { serialNumber: event.target.value }
                                  : { lotCode: event.target.value },
                              )
                            }
                          />
                        </Field>
                      ) : (
                        <Field appearance="standard">
                          {line.tracking_mode === "serial" ? "Serial number" : "Available lot"}
                          <Select appearance="standard"
                            required
                            value={allocation.lotId ?? ""}
                            onChange={(event) =>
                              changeAllocation(line, allocation.key, { lotId: event.target.value })
                            }
                          >
                            <option value="">Select {String(line.tracking_mode)}</option>
                            {availableLots.map((lot) => (
                              <option key={String(lot.id)} value={String(lot.id)}>
                                {String(lot.serial_number ?? lot.lot_code)} ·{" "}
                                {Number(lot.quantity_milli) / 1000} {String(line.unit)}
                                {lot.expiry_date ? ` · expires ${lot.expiry_date}` : ""}
                              </option>
                            ))}
                          </Select>
                        </Field>
                      )}
                      <Field appearance="standard">
                        Quantity
                        <TextInput appearance="standard"
                          required
                          type="number"
                          min={line.tracking_mode === "serial" ? "1" : "0.001"}
                          max={line.tracking_mode === "serial" ? "1" : undefined}
                          step={line.tracking_mode === "serial" ? "1" : "0.001"}
                          readOnly={line.tracking_mode === "serial"}
                          value={allocation.quantity}
                          onChange={(event) =>
                            changeAllocation(line, allocation.key, { quantity: event.target.value })
                          }
                        />
                      </Field>
                      {incoming && (
                        <>
                          <Field appearance="standard">
                            Manufactured date
                            <TextInput appearance="standard"
                              type="date"
                              value={allocation.manufacturedDate ?? ""}
                              onChange={(event) =>
                                changeAllocation(line, allocation.key, {
                                  manufacturedDate: event.target.value,
                                })
                              }
                            />
                          </Field>
                          <Field appearance="standard">
                            Expiry date
                            <TextInput appearance="standard"
                              type="date"
                              value={allocation.expiryDate ?? ""}
                              onChange={(event) =>
                                changeAllocation(line, allocation.key, {
                                  expiryDate: event.target.value,
                                })
                              }
                            />
                          </Field>
                          <Field appearance="standard">
                            Receipt disposition
                            <Select appearance="standard"
                              value={allocation.disposition}
                              onChange={(event) =>
                                changeAllocation(line, allocation.key, {
                                  disposition: event.target.value as AllocationDraft["disposition"],
                                })
                              }
                            >
                              <option value="available">Available</option>
                              <option value="quarantine">Quarantine for Quality review</option>
                            </Select>
                          </Field>
                          {allocation.disposition === "quarantine" && (
                            <>
                              <Field appearance="standard">
                                Quarantine reason
                                <TextInput appearance="standard"
                                  required
                                  minLength={3}
                                  value={allocation.quarantineReason ?? ""}
                                  onChange={(event) =>
                                    changeAllocation(line, allocation.key, {
                                      quarantineReason: event.target.value,
                                    })
                                  }
                                />
                              </Field>
                              <Field appearance="standard">
                                Review due date
                                <TextInput appearance="standard"
                                  required
                                  type="date"
                                  value={allocation.quarantineDueDate ?? ""}
                                  onChange={(event) =>
                                    changeAllocation(line, allocation.key, {
                                      quarantineDueDate: event.target.value,
                                    })
                                  }
                                />
                              </Field>
                            </>
                          )}
                        </>
                      )}
                      <Button appearance="standard"
                        variant="secondary"
                        type="button"
                        onClick={() => removeAllocation(line, allocation.key)}
                      >
                        Remove {String(line.tracking_mode)} {index + 1}
                      </Button>
                    </FormGrid>
                  ))}
                  <Button appearance="standard" variant="secondary" type="button" onClick={() => addAllocation(line)}>
                    Add {String(line.tracking_mode)}
                  </Button>
                  {!incoming && !availableLots.length && (
                    <InlineNotice tone="danger" role="alert">
                      No released {String(line.tracking_mode)} stock is available in this warehouse.
                    </InlineNotice>
                  )}
                </>
              )}
            </fieldset>
          );
        })}
        <Button appearance="standard"
          clientManaged type="submit"
          variant="primary"
          disabled={
            pending || (!incoming && (attachmentsV2 === true ? !proof?.bindable : !evidenceId))
          }
        >
          {pending ? "Recording…" : incoming ? "Record receipt" : "Record delivery"}
        </Button>
      </form>
    </Card>
  );
}
export function InvoiceDeliveryForm({ base, fulfillment }: { base: string; fulfillment: Row }) {
  const { send, pending, error } = useCommand(base);
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        void send(`fulfillments/${fulfillment.id}/invoice`, {
          expectedVersion: fulfillment.version,
          date: f.get("date"),
          dueDate: f.get("dueDate"),
        });
      }}
    >
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <FormGrid as="div" className="ui-form-grid">
        <Field appearance="standard">
          Invoice date
          <TextInput appearance="standard" name="date" type="date" required defaultValue={businessDate()} />
        </Field>
        <Field appearance="standard">
          Invoice due date
          <TextInput appearance="standard" name="dueDate" type="date" required defaultValue={businessDate()} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending}>
          Prepare invoice for GL review
        </Button>
      </FormGrid>
    </form>
  );
}
export function ReturnRequestForm({
  base,
  fulfillment,
  lines,
  lotAllocations,
  customerReturn,
}: {
  base: string;
  fulfillment: Row;
  lines: Row[];
  lotAllocations: Row[];
  customerReturn: boolean;
}) {
  const { send, pending, error } = useCommand(base),
    [open, setOpen] = useState(false),
    [attachmentSet, setAttachmentSet] = useState<AttachmentPanelChange | null>(null),
    available = lines.filter(
      (line) =>
        line.fulfillment_id === fulfillment.id &&
        Number(line.returned_milli) < Number(line.quantity_milli),
    );
  if (!available.length) return null;
  const businessToday = businessDate(),
    defaultReturnDate =
      String(fulfillment.fulfillment_date) > businessToday
        ? String(fulfillment.fulfillment_date)
        : businessToday;
  return (
    <div>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <Button appearance="standard" variant="secondary" type="button" onClick={() => setOpen(!open)}>
        {open ? "Close return" : "Request return"}
      </Button>
      {open && (
        <Dialog appearance="standard" open={open} onClose={() => setOpen(false)} title="Request a return" presentation="form-drawer" dismissDisabled={pending}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget),
              returnLines = available.flatMap((line) => {
                const lots = lotAllocations.filter((a) => a.fulfillment_line_id === line.id),
                  allocations = lots.flatMap((a) => {
                    const quantity = String(form.get(`lot:${line.id}:${a.lot_id}`) ?? "0");
                    return Number(quantity) > 0 ? [{ lotId: String(a.lot_id), quantity }] : [];
                  }),
                  quantity = lots.length
                    ? String(allocations.reduce((sum, a) => sum + Number(a.quantity), 0))
                    : String(form.get(String(line.id)));
                return Number(quantity) > 0
                  ? [
                      {
                        lineId: line.id,
                        quantity,
                        allocations,
                        condition: form.get(`condition:${line.id}`),
                        conditionNote: form.get(`condition-note:${line.id}`),
                        qualityDueDate: form.get(`quality-due:${line.id}`),
                      },
                    ]
                  : [];
              });
            void send(`fulfillments/${fulfillment.id}/returns`, {
              expectedVersion: Number(fulfillment.version),
              attachmentSetIds:
                attachmentSet?.setId && attachmentSet.readyCount > 0 && attachmentSet.bindable
                  ? [attachmentSet.setId]
                  : [],
              date: form.get("date"),
              reason: form.get("reason"),
              lines: returnLines,
            });
          }}
        >
          <Field appearance="standard">
            Return date
            <TextInput appearance="standard"
              name="date"
              type="date"
              required
              min={String(fulfillment.fulfillment_date)}
              defaultValue={defaultReturnDate}
            />
          </Field>
          {customerReturn && (
            <p className="muted">
              Returned customer stock is quarantined immediately. Quality must inspect it before it
              becomes available or Finance posts controlled scrap.
            </p>
          )}
          {available.map((line) => {
            const lots = lotAllocations.filter((a) => a.fulfillment_line_id === line.id);
            return (
              <fieldset key={String(line.id)}>
                <legend>
                  {String(line.sku)} · {String(line.description)} (
                  {(Number(line.quantity_milli) - Number(line.returned_milli)) / 1000} available)
                </legend>
                {lots.length ? (
                  lots
                    .filter((a) => Number(a.returned_milli) < Number(a.quantity_milli))
                    .map((a) => (
                      <Field appearance="standard" key={String(a.id)}>
                        {a.serial_number
                          ? `Serial ${String(a.serial_number)}`
                          : `Lot ${String(a.lot_code)}`}{" "}
                        · {(Number(a.quantity_milli) - Number(a.returned_milli)) / 1000} returnable
                        <TextInput appearance="standard"
                          name={`lot:${line.id}:${a.lot_id}`}
                          type="number"
                          min="0"
                          max={(Number(a.quantity_milli) - Number(a.returned_milli)) / 1000}
                          step={a.serial_number ? "1" : "0.001"}
                          defaultValue="0"
                        />
                      </Field>
                    ))
                ) : (
                  <Field appearance="standard">
                    Return quantity
                    <TextInput appearance="standard"
                      name={String(line.id)}
                      type="number"
                      min="0"
                      max={(Number(line.quantity_milli) - Number(line.returned_milli)) / 1000}
                      step="0.001"
                      defaultValue="0"
                    />
                  </Field>
                )}
                {customerReturn && (
                  <>
                    <Field appearance="standard">
                      Observed condition
                      <Select appearance="standard" name={`condition:${line.id}`} defaultValue="unknown">
                        <option value="unopened">Unopened / apparently resalable</option>
                        <option value="damaged">Damaged</option>
                        <option value="unknown">Unknown · inspect before use</option>
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Condition evidence
                      <TextInput appearance="standard"
                        name={`condition-note:${line.id}`}
                        required
                        minLength={3}
                        maxLength={500}
                        placeholder="Packaging, damage, customer statement, or receiving observation"
                      />
                    </Field>
                    <Field appearance="standard">
                      Quality review due
                      <TextInput appearance="standard"
                        name={`quality-due:${line.id}`}
                        type="date"
                        required
                        min={defaultReturnDate}
                        defaultValue={defaultReturnDate}
                      />
                    </Field>
                  </>
                )}
              </fieldset>
            );
          })}
          <AttachmentPanel appearance="standard"
            base={base}
            targetType="commerce_fulfillment"
            targetId={String(fulfillment.id)}
            expectedTargetVersion={Number(fulfillment.version)}
            componentKind="action"
            componentRef="return-request"
            purposeKey="return_condition"
            title="Return condition files"
            description="Attach package, damage, or receiving photos and supporting documents for this return."
            visibility="Sales, warehouse, Finance, and authorized Quality reviewers can access these files."
            onChange={setAttachmentSet}
          />
          <Field appearance="standard">
            Return reason
            <TextInput appearance="standard" name="reason" required minLength={3} maxLength={500} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending}>
            {pending ? "Submitting…" : "Submit return for review"}
          </Button>
        </FormLayout></Stack></Dialog>
      )}
    </div>
  );
}
export function ReturnActions({
  base,
  record,
  canApprove,
  canComplete,
  fxAccounts = [],
}: {
  base: string;
  record: Row;
  canApprove: boolean;
  canComplete: boolean;
  fxAccounts?: Row[];
}) {
  const { send, pending, error } = useCommand(base);
  if (record.status === "requested" && canApprove)
    return (
      <div>
        {error && (
          <InlineNotice tone="danger" role="alert" className="error">
            {error}
          </InlineNotice>
        )}
        <FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget),
              action = (event.nativeEvent as SubmitEvent).submitter?.getAttribute("value");
            if (action !== "approve" && action !== "reject") return;
            void send(`returns/${record.id}/${action}`, {
              expectedVersion: Number(record.version),
              reason: form.get("reason"),
            });
          }}
        >
          <Field appearance="standard">
            Review evidence
            <TextInput appearance="standard" name="reason" required minLength={3} maxLength={500} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" name="action" value="approve" disabled={pending}>
            Approve return
          </Button>
          <Button appearance="standard" clientManaged type="submit" variant="secondary" name="action" value="reject" disabled={pending}>
            Reject return
          </Button>
        </FormLayout>
      </div>
    );
  if (record.status === "approved" && canComplete)
    return (
      <FormLayout className="ui-form-grid" onSubmit={(event)=>{
        event.preventDefault();const form=new FormData(event.currentTarget);
        void send(`returns/${record.id}/complete`,{expectedVersion:Number(record.version),
          ...(form.get('fxGainAccountId')?{fxGainAccountId:form.get('fxGainAccountId')}:{}),
          ...(form.get('fxLossAccountId')?{fxLossAccountId:form.get('fxLossAccountId')}:{})});
      }}>
        {error && (
          <InlineNotice tone="danger" role="alert" className="error">
            {error}
          </InlineNotice>
        )}
        {record.invoice_currency!==record.stock_currency&&fxAccounts.length>0&&<>
          <p className="muted">Review the realized FX accounts when this credit applies to a revalued invoice. The original credit and stock values stay retained.</p>
          <Field appearance="standard">Realized FX gain account<Select appearance="standard" name="fxGainAccountId" defaultValue=""><option value="">Choose reviewed gain account</option>{fxAccounts.filter(account=>account.category==='other_income').map(account=><option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</Select></Field>
          <Field appearance="standard">Realized FX loss account<Select appearance="standard" name="fxLossAccountId" defaultValue=""><option value="">Choose reviewed loss account</option>{fxAccounts.filter(account=>account.category==='other_expense').map(account=><option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</Select></Field>
        </>}
        <Button appearance="standard"
          clientManaged type="submit"
          variant="primary"
          disabled={pending}
        >
          {pending ? "Completing…" : "Complete credit / debit note"}
        </Button>
      </FormLayout>
    );
  return null;
}
export function SettlementForm({ base, invoice, periods, bankAccounts }: { base: string; invoice: Row; periods: Row[]; bankAccounts: Row[] }) {
  const { send, pending, error } = useCommand(base);
  const [receiptOpen, setReceiptOpen] = useState(false);
  const [paymentSlip, setPaymentSlip] = useState<InvoiceSlipSelection | null>(null);
  const paymentLabel = invoice.kind === "payable" ? "Record made payment" : "Record received payment";
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>
        {invoice.source_type === "expense_claim"
          ? "Record employee reimbursement"
          : invoice.kind === "payable"
            ? "Record supplier payment"
            : "Record customer receipt"}
      </h2>
      <p className="muted">
        Record a payment already received or made. This records accounting evidence; it does not
        send money through a bank.
      </p>
      <FormDisclosure appearance="standard" open={receiptOpen} onToggle={event=>setReceiptOpen(event.currentTarget.open)} drawerTitle={paymentLabel} error={error} dismissDisabled={pending}>
      <summary>{paymentLabel}</summary>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          const saved = await send(
            `invoices/${invoice.id}/settle`,
            {
              expectedVersion: invoice.version,
              date: f.get("date"),
              periodId: f.get("periodId"),
              bankAccountId: f.get("bankAccountId") || undefined,
              amount: f.get("amount"),
              reference: f.get("reference"),
              paymentSlipId: paymentSlip?.sourceId,
              attachmentSetIds: paymentSlip ? [paymentSlip.setId] : [],
            },
            `${base}/finance/invoices/${invoice.id}`,
          );
          if (saved) { setReceiptOpen(false); setPaymentSlip(null); }
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">Accounting period
            <Select appearance="standard" name="periodId" required defaultValue="">
              <option value="">Choose open period</option>
              {periods.filter(p=>p.state==="open").map(p=><option key={String(p.id)} value={String(p.id)}>{String(p.name)} · {String(p.start_date)}–{String(p.end_date)}</option>)}
            </Select>
          </Field>
          {bankAccounts.length>0 && <Field appearance="standard">{invoice.kind === "payable" ? "Paid from bank account" : "Received into bank account"}
            <Select appearance="standard" name="bankAccountId" required defaultValue={bankAccounts.length===1?String(bankAccounts[0].id):""}>
              <option value="">Choose bank account</option>
              {bankAccounts.map(a=><option key={String(a.id)} value={String(a.id)}>{String(a.code)} · {String(a.name)}</option>)}
            </Select>
          </Field>}
          <Field appearance="standard">
            Payment date
            <TextInput appearance="standard" name="date" type="date" required defaultValue={businessDate()} />
          </Field>
          <Field appearance="standard">
            Amount ({String(invoice.currency)})
            <TextInput appearance="standard"
              name="amount"
              type="number"
              min="0.01"
              step="0.01"
              required
              max={Number(invoice.outstanding_minor) / 100}
              defaultValue={minorToDecimal(Number(invoice.outstanding_minor))}
            />
          </Field>
          <Field appearance="standard">
            Bank reference
            <TextInput appearance="standard" name="reference" required minLength={2} maxLength={100} />
          </Field>
          {invoice.kind === "payable" && (
            <InvoiceSlipUpload
              base={base}
              invoiceId={String(invoice.id)}
              invoiceVersion={Number(invoice.version)}
              onUploaded={setPaymentSlip}
            />
          )}
          <Button appearance="standard"
            clientManaged type="submit"
            variant="primary"
            disabled={pending || (invoice.kind === "payable" && !paymentSlip)}
          >
            Record and allocate payment
          </Button>
        </FormGrid>
      </form>
      </FormDisclosure>
    </Card>
  );
}
export function MasterEditor({
  base,
  kind,
  options,
  row,
  initialOpen = false,
  partyKind,
}: {
  base: string;
  kind: "products" | "parties" | "warehouses";
  options: Options;
  row?: Row;
  initialOpen?: boolean;
  partyKind?: "customer" | "supplier";
}) {
  const { send, pending, error } = useCommand(base);
  const [open, setOpen] = useState(initialOpen);
  const embedded = useEmbeddedRecord();
  return (
    <div>
      <Button hidden={embedded&&open} appearance="standard" type="button" variant="secondary" onClick={() => setOpen(!open)}>
        {open
          ? "Close"
          : row
            ? "Edit"
            : `Add ${kind === "products" ? "product" : kind === "parties" ? (partyKind ?? "customer / supplier") : "warehouse"}`}
      </Button>
      {open && (
        <FormDrawer appearance="standard" inline={initialOpen} open={open} onClose={() => setOpen(false)} title="Record details" dismissDisabled={pending}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<form
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            const value = Object.fromEntries(f);
            const input =
              kind === "products"
                ? { ...value, taxBps: Math.round(Number(f.get("tax")) * 100) }
                : kind === "parties"
                  ? { ...value, paymentTermsDays: Number(f.get("paymentTermsDays")) }
                  : value;
            if (
              await send(
                `${kind}${row ? `/${row.id}` : ""}`,
                { ...input, ...(row ? { expectedVersion: row.version } : {}) },
                `${base}/commerce?view=${kind}${kind === "parties" && partyKind ? `&kind=${partyKind}` : ""}`,
              )
            )
              setOpen(false);
          }}
        >
          {error && (
            <InlineNotice tone="danger" role="alert" className="error">
              {error}
            </InlineNotice>
          )}
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              {kind === "products" ? "SKU" : "Code"}
              <TextInput appearance="standard"
                name={kind === "products" ? "sku" : "code"}
                required
                minLength={2}
                defaultValue={String(row?.sku ?? row?.code ?? "")}
              />
            </Field>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard"
                name="name"
                required
                minLength={2}
                defaultValue={String(row?.name ?? "")}
              />
            </Field>
            {kind === "products" && (
              <>
                <Field appearance="standard">
                  Unit
                  <TextInput appearance="standard" name="unit" required defaultValue={String(row?.unit ?? "each")} />
                </Field>
                <Field appearance="standard">
                  Traceability
                  <Select appearance="standard" name="trackingMode" defaultValue={String(row?.tracking_mode ?? "none")}>
                    <option value="none">No lot or serial tracking</option>
                    <option value="lot">Lot tracked</option>
                    <option value="serial">Serial tracked</option>
                  </Select>
                </Field>
                <Field appearance="standard">
                  List price
                  <TextInput appearance="standard"
                    name="price"
                    type="number"
                    min="0"
                    step="0.01"
                    required
                    defaultValue={minorToDecimal(Number(row?.price_minor ?? 0))}
                  />
                </Field>
                <Field appearance="standard">
                  Default tax (%)
                  <TextInput appearance="standard"
                    name="tax"
                    type="number"
                    min="0"
                    max="100"
                    step="0.01"
                    required
                    defaultValue={Number(row?.tax_bps ?? 0) / 100}
                  />
                </Field>
              </>
            )}
            {kind === "parties" && (
              <>
                {partyKind ? (
                  <TextInput appearance="standard" type="hidden" name="kind" value={partyKind} />
                ) : (
                  <Field appearance="standard">
                    Party type
                    <Select appearance="standard" name="kind" defaultValue={String(row?.kind ?? "customer")}>
                      <option value="customer">Customer</option>
                      <option value="supplier">Supplier</option>
                    </Select>
                  </Field>
                )}
                <Field appearance="standard">
                  Payment terms (days)
                  <TextInput appearance="standard"
                    name="paymentTermsDays"
                    type="number"
                    min="0"
                    max="365"
                    required
                    defaultValue={Number(row?.payment_terms_days ?? 30)}
                  />
                </Field>
                {partyKind === "supplier" ? (
                  <>
                    <TextInput appearance="standard"
                      type="hidden"
                      name="creditLimit"
                      value={minorToDecimal(Number(row?.credit_limit_minor ?? 0))}
                    />
                    <TextInput appearance="standard"
                      type="hidden"
                      name="notificationEmail"
                      value={String(row?.notification_email ?? "")}
                    />
                    <TextInput appearance="standard"
                      type="hidden"
                      name="notificationConsent"
                      value={String(row?.notification_consent ?? "unknown")}
                    />
                  </>
                ) : (
                  <>
                    <Field appearance="standard">
                      Customer credit limit ({options.currency})
                      <TextInput appearance="standard"
                        name="creditLimit"
                        type="number"
                        min="0"
                        step="0.01"
                        required
                        defaultValue={minorToDecimal(Number(row?.credit_limit_minor ?? 0))}
                      />
                    </Field>
                    <Field appearance="standard">
                      Customer delivery email
                      <TextInput appearance="standard"
                        name="notificationEmail"
                        type="email"
                        maxLength={320}
                        defaultValue={String(row?.notification_email ?? "")}
                        placeholder="Used only for customer notifications"
                      />
                    </Field>
                    <Field appearance="standard">
                      Notification consent
                      <Select appearance="standard"
                        name="notificationConsent"
                        defaultValue={String(row?.notification_consent ?? "unknown")}
                      >
                        <option value="unknown">Not confirmed</option>
                        <option value="granted">Granted and verified</option>
                        <option value="revoked">Revoked / opted out</option>
                      </Select>
                    </Field>
                  </>
                )}
              </>
            )}
            {kind === "warehouses" && (
              <Field appearance="standard">
                Branch
                <Select appearance="standard" name="branchId" defaultValue={String(row?.branch_id ?? "")} required>
                  <option value="">Select branch</option>
                  {options.nodes.map((n) => (
                    <option key={String(n.id)} value={String(n.id)}>
                      {String(n.display_name)}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            <Field appearance="standard">
              Status
              <Select appearance="standard" name="status" defaultValue={String(row?.status ?? "active")}>
                <option value="active">Active</option>
                <option value={kind === "parties" ? "blocked" : "archived"}>
                  {kind === "parties" ? "Blocked" : "Archived"}
                </option>
              </Select>
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending}>
              Save {kind === "products" ? "product" : kind === "parties" ? "party" : "warehouse"}
            </Button>
          </FormGrid>
        </form></Stack></FormDrawer>
      )}
    </div>
  );
}

export function StockReservationForm({
  base,
  order,
  lines,
}: {
  base: string;
  order: Row;
  lines: Row[];
}) {
  const { send, pending, error } = useCommand(base);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Reserve stock for this order</h2>
      <p className="muted">
        Reserved quantities are unavailable to other orders and POS. Delivery consumes this order’s
        reservation; expiry releases availability automatically.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          const action = String(f.get("action"));
          void send(`orders/${order.id}/reservations`, {
            expectedVersion: order.version,
            action,
            lineId: f.get("lineId"),
            quantity: f.get("quantity"),
            reason: f.get("reason"),
            ...(action === "reserve"
              ? { expiresAt: new Date(String(f.get("expiresAt"))).getTime() }
              : {}),
          });
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Order line
            <Select appearance="standard" required name="lineId">
              {lines
                .filter((l) => Number(l.quantity_milli) > Number(l.fulfilled_milli))
                .map((l) => (
                  <option key={String(l.id)} value={String(l.id)}>
                    {String(l.sku)} · Reserved {Number(l.reserved_milli) / 1000} · Remaining{" "}
                    {(Number(l.quantity_milli) - Number(l.fulfilled_milli)) / 1000}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Action
            <Select appearance="standard" name="action">
              <option value="reserve">Reserve</option>
              <option value="release">Release</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Quantity
            <TextInput appearance="standard" required name="quantity" type="number" min="0.001" step="0.001" />
          </Field>
          <PreferredDateTimeField appearance="standard" name="expiresAt" label="Expiry (reserve only)" />
          <Field appearance="standard">
            Reason
            <TextInput appearance="standard" required name="reason" minLength={3} maxLength={500} />
          </Field>
        </FormGrid>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending}>
          {pending ? "Saving…" : "Save reservation"}
        </Button>
      </form>
    </Card>
  );
}

export function CustomerNotificationSettingsForm({
  base,
  settings,
}: {
  base: string;
  settings: Row;
}) {
  const { send, pending, error } = useCommand(base);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Customer delivery notifications</h2>
      <p className="muted">
        Disabled sends nothing. Sandbox records a simulated delivery for training. Resend uses the
        configured live email provider.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <FormLayout
        className="ui-form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          const form = new FormData(event.currentTarget);
          void send(
            "customer-notifications/settings",
            { expectedVersion: Number(settings.version), adapter: form.get("adapter") },
            `${base}/commerce?view=parties&kind=customer`,
          );
        }}
      >
        <Field appearance="standard">
          Delivery adapter
          <Select appearance="standard" name="adapter" defaultValue={String(settings.adapter)}>
            <option value="disabled">Disabled</option>
            <option value="sandbox">Sandbox · no external email</option>
            <option value="resend">Resend · live external email</option>
          </Select>
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending}>
          {pending ? "Saving…" : "Save notification adapter"}
        </Button>
      </FormLayout>
    </Card>
  );
}

export function CustomerDeliveryNotification({
  base,
  fulfillment,
  notification,
  customerEmail,
  customerConsent,
  adapter,
}: {
  base: string;
  fulfillment: Row;
  notification?: Row;
  customerEmail: string;
  customerConsent: string;
  adapter: string;
}) {
  const { send, pending, error } = useCommand(base);
  if (notification)
    return (
      <Card as="section" appearance="standard" className="ui-panel">
        <strong>Customer notification · {String(notification.status)}</strong>
        <p>
          {String(notification.recipient_email)} · Attempt {String(notification.attempts)}
        </p>
        {notification.sent_at && (
          <p className="muted">
            Sent <TimeText appearance="standard" value={Number(notification.sent_at)} /> · {String(notification.adapter)}
          </p>
        )}
        {notification.last_error && <InlineNotice tone="danger" role="alert">{String(notification.last_error)}</InlineNotice>}
        {notification.status === "failed" && (
          <Button appearance="standard"
            type="submit"
            variant="secondary"
            disabled={pending}
            onClick={() =>
              void send(`customer-notifications/${notification.id}/retry`, {
                expectedVersion: Number(notification.version),
              })
            }
          >
            {pending ? "Queueing…" : "Retry notification"}
          </Button>
        )}
        {error && (
          <InlineNotice tone="danger" role="alert" className="error">
            {error}
          </InlineNotice>
        )}
      </Card>
    );
  const blocker =
    customerConsent !== "granted" || !customerEmail
      ? "Add a verified customer email and granted consent in Customer directory."
      : adapter === "disabled"
        ? "Configure a sandbox or live notification adapter in Customer directory."
        : "";
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h3>Notify customer</h3>
      {blocker ? (
        <p className="muted">{blocker}</p>
      ) : (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            void send(`fulfillments/${fulfillment.id}/notify`, {
              expectedVersion: Number(fulfillment.version),
              note: form.get("note"),
            });
          }}
        >
          <p>
            Recipient: <strong>{customerEmail}</strong> ·{" "}
            {adapter === "sandbox" ? "Sandbox delivery" : "Live provider"}
          </p>
          <Field appearance="standard">
            Delivery update
            <TextInput appearance="standard"
              name="note"
              required
              minLength={3}
              maxLength={500}
              placeholder="Carrier reference, delivery window, or receiving update"
            />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending}>
            {pending ? "Queueing…" : "Queue customer notification"}
          </Button>
        </FormLayout>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
    </Card>
  );
}
