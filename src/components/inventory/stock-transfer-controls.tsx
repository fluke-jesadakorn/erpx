"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import type { TransferRow as Row } from "@/server/modules/inventory/public/inventory-transfers";
import { minorToDecimal } from "@/lib/finance/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { FormGrid, Stack, Card } from "@/components/ui/foundation";
type Options = { warehouses: Row[]; sources: Row[]; stock: Row[]; lots: Row[] };
function useTransferCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ fingerprint: string; key: string } | null>(null),
    [error, setError] = useState(""),
    [pending, setPending] = useState(false);
  async function send(path: string, input: unknown, create = false) {
    const body = JSON.stringify(input),
      fingerprint = path + body;
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/stock-transfers${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": last.current.key,
        },
        body,
      });
      const data = (await r.json()) as {
        data: { id: string };
        error?: { message: string };
      };
      if (!r.ok) throw new Error(data.error?.message ?? "Unable to save transfer.");
      last.current = null;
      if (create) router.push(`${base}/stock-transfers/${data.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save transfer.");
    } finally {
      setPending(false);
    }
  }
  return { send, error, pending };
}
export function TransferEditor({
  base,
  options,
  transfer,
  lines,
  allocations,
  initialOpen = false,
}: {
  base: string;
  options: Options;
  transfer?: Row;
  lines?: Row[];
  allocations?: Row[];
  initialOpen?: boolean;
}) {
  const { send, error, pending } = useTransferCommand(base),
    [source, setSource] = useState(
      String(transfer?.source_warehouse_id ?? options.sources[0]?.id ?? ""),
    ),
    [target, setTarget] = useState(String(transfer?.target_warehouse_id ?? "")),
    [product, setProduct] = useState("");
  const [items, setItems] = useState(
    (lines ?? []).map((l) => ({
      productId: String(l.product_id),
      quantity: String(Number(l.quantity_milli) / 1000),
      description: String(l.description),
      trackingMode: String(l.tracking_mode ?? "none"),
      allocations: (allocations ?? [])
        .filter((a) => a.transfer_line_id === l.id)
        .map((a) => ({
          lotId: String(a.lot_id),
          quantity: String(Number(a.quantity_milli) / 1000),
        })),
    })),
  );
  const available = options.stock.filter(
    (s) => s.warehouse_id === source && !items.some((l) => l.productId === s.product_id),
  );
  return (
    <FormDisclosure appearance="standard" inline={Boolean(!!transfer || initialOpen)} className="ui-panel" open={!!transfer || initialOpen}>
      <summary>{transfer ? "Edit transfer quantities" : "New warehouse transfer"}</summary>
      <p className="muted">
        Choose a completed direct movement or staged dispatch and receiving. Staged transport
        preserves goods in transit until receipt or independently approved loss. Source cost is
        preserved.
      </p>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          void send(
            transfer ? `/${transfer.id}` : "",
            {
              sourceWarehouseId: source,
              targetWarehouseId: target,
              transferDate: f.get("transferDate"),
              deliveryMode: f.get("deliveryMode"),
              arrivalDueDate: f.get("arrivalDueDate") || undefined,
              reason: f.get("reason"),
              lines: items.map((l) => ({
                productId: l.productId,
                quantity:
                  l.trackingMode === "none"
                    ? l.quantity
                    : String(l.allocations.reduce((sum, a) => sum + Number(a.quantity || 0), 0)),
                allocations: l.allocations,
              })),
              ...(transfer ? { expectedVersion: transfer.version } : {}),
            },
            !transfer,
          );
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Transport mode
            <Select appearance="standard"
              name="deliveryMode"
              defaultValue={String(transfer?.delivery_mode ?? "direct")}
              disabled={!!transfer}
            >
              <option value="direct">Completed direct movement</option>
              <option value="transit">Dispatch and receive separately</option>
            </Select>
            {transfer && (
              <TextInput appearance="standard" type="hidden" name="deliveryMode" value={String(transfer.delivery_mode)} />
            )}
          </Field>
          <Field appearance="standard">
            Expected arrival (staged transport)
            <TextInput appearance="standard"
              type="date"
              name="arrivalDueDate"
              defaultValue={String(transfer?.arrival_due_date ?? "")}
            />
          </Field>
          <Field appearance="standard">
            Source warehouse
            <Select appearance="standard"
              required
              disabled={!!transfer}
              value={source}
              onChange={(e) => {
                setSource(e.target.value);
                setItems([]);
                setProduct("");
                if (e.target.value === target) setTarget("");
              }}
            >
              {options.sources.map((w) => (
                <option key={String(w.id)} value={String(w.id)}>
                  {String(w.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Receiving warehouse
            <Select appearance="standard"
              required
              disabled={!!transfer}
              value={target}
              onChange={(e) => setTarget(e.target.value)}
            >
              <option value="">Select receiving warehouse</option>
              {options.warehouses
                .filter((w) => w.id !== source)
                .map((w) => (
                  <option key={String(w.id)} value={String(w.id)}>
                    {String(w.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Transfer planning / receipt date
            <TextInput appearance="standard"
              type="date"
              name="transferDate"
              required
              defaultValue={String(
                transfer?.transfer_date ?? new Date().toISOString().slice(0, 10),
              )}
            />
          </Field>
          <Field appearance="standard">
            Transfer purpose
            <TextInput appearance="standard"
              name="reason"
              required
              minLength={5}
              maxLength={2000}
              defaultValue={String(transfer?.reason ?? "")}
              placeholder="Replenishment or installation staging"
            />
          </Field>
        </FormGrid>
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Product to transfer
            <Select appearance="standard" value={product} onChange={(e) => setProduct(e.target.value)}>
              <option value="">Select available stock</option>
              {available.map((s) => (
                <option key={String(s.product_id)} value={String(s.product_id)}>
                  {String(s.sku)} · {String(s.name)} ·{" "}
                  {Number(s.available_milli ?? s.quantity_milli) / 1000} {String(s.unit)} available
                </option>
              ))}
            </Select>
          </Field>
          <Button appearance="standard"
            variant="secondary"
            type="button"

            disabled={!available.some((s) => s.product_id === product) || items.length >= 20}
            onClick={() => {
              setItems([
                ...items,
                {
                  productId: product,
                  quantity: "",
                  trackingMode: String(
                    available.find((s) => s.product_id === product)?.tracking_mode ?? "none",
                  ),
                  allocations: [],
                  description: String(available.find((s) => s.product_id === product)?.name ?? ""),
                },
              ]);
              setProduct("");
            }}
          >
            Add transfer product
          </Button>
        </FormGrid>
        {items.map((l, index) => {
          const s = options.stock.find(
            (s) => s.warehouse_id === source && s.product_id === l.productId,
          );
          return (
            <FormGrid as="div" className="ui-form-grid" key={l.productId}>
              <div>
                <strong>{l.description}</strong>
                <p className="muted">
                  Available: {Number(s?.available_milli ?? s?.quantity_milli ?? 0) / 1000}{" "}
                  {String(s?.unit ?? "")} · recorded value{" "}
                  {minorToDecimal(Number(s?.value_minor ?? 0))}
                </p>
              </div>
              {l.trackingMode === "none" ? (
                <Field appearance="standard">
                  Transfer quantity {index + 1}
                  <TextInput appearance="standard"
                    type="number"
                    min="0.001"
                    step="0.001"
                    required
                    value={l.quantity}
                    onChange={(e) =>
                      setItems(
                        items.map((x, i) => (i === index ? { ...x, quantity: e.target.value } : x)),
                      )
                    }
                  />
                </Field>
              ) : (
                <div>
                  <strong>{l.trackingMode} allocations</strong>
                  <p className="muted">
                    Allocated {l.allocations.reduce((sum, a) => sum + Number(a.quantity || 0), 0)}{" "}
                    {String(s?.unit ?? "")}
                  </p>
                </div>
              )}
              {l.trackingMode !== "none" && (
                <Stack as="div" className="ui-stack">
                  {l.allocations.map((allocation, allocationIndex) => {
                    const lot = options.lots.find((item) => item.id === allocation.lotId);
                    return (
                      <FormGrid as="div" className="ui-form-grid" key={`${l.productId}-${allocationIndex}`}>
                        <Field appearance="standard">
                          {l.trackingMode === "serial" ? "Serial number" : "Lot"}
                          <Select appearance="standard"
                            required
                            value={allocation.lotId}
                            onChange={(event) =>
                              setItems(
                                items.map((item, itemIndex) =>
                                  itemIndex === index
                                    ? {
                                        ...item,
                                        allocations: item.allocations.map((entry, entryIndex) =>
                                          entryIndex === allocationIndex
                                            ? { ...entry, lotId: event.target.value }
                                            : entry,
                                        ),
                                      }
                                    : item,
                                ),
                              )
                            }
                          >
                            <option value="">Select {l.trackingMode}</option>
                            {options.lots
                              .filter(
                                (item) =>
                                  item.warehouse_id === source &&
                                  item.product_id === l.productId &&
                                  !Number(item.quality_held) &&
                                  !l.allocations.some(
                                    (used, usedIndex) =>
                                      usedIndex !== allocationIndex && used.lotId === item.id,
                                  ),
                              )
                              .map((item) => (
                                <option key={String(item.id)} value={String(item.id)}>
                                  {String(item.serial_number ?? item.lot_code)} ·{" "}
                                  {Number(item.quantity_milli) / 1000} available
                                  {item.expiry_date ? ` · expires ${item.expiry_date}` : ""}
                                </option>
                              ))}
                          </Select>
                        </Field>
                        <Field appearance="standard">
                          Allocation quantity
                          <TextInput appearance="standard"
                            required
                            type="number"
                            min={l.trackingMode === "serial" ? "1" : "0.001"}
                            max={
                              l.trackingMode === "serial"
                                ? "1"
                                : Number(lot?.quantity_milli ?? 0) / 1000 || undefined
                            }
                            step={l.trackingMode === "serial" ? "1" : "0.001"}
                            readOnly={l.trackingMode === "serial"}
                            value={allocation.quantity}
                            onChange={(event) =>
                              setItems(
                                items.map((item, itemIndex) =>
                                  itemIndex === index
                                    ? {
                                        ...item,
                                        allocations: item.allocations.map((entry, entryIndex) =>
                                          entryIndex === allocationIndex
                                            ? { ...entry, quantity: event.target.value }
                                            : entry,
                                        ),
                                      }
                                    : item,
                                ),
                              )
                            }
                          />
                        </Field>
                        <Button appearance="standard"
                          variant="secondary"
                          type="button"
                          onClick={() =>
                            setItems(
                              items.map((item, itemIndex) =>
                                itemIndex === index
                                  ? {
                                      ...item,
                                      allocations: item.allocations.filter(
                                        (_, entryIndex) => entryIndex !== allocationIndex,
                                      ),
                                    }
                                  : item,
                              ),
                            )
                          }
                        >
                          Remove allocation
                        </Button>
                      </FormGrid>
                    );
                  })}
                  <Button appearance="standard"
                    variant="secondary"
                    type="button"
                    onClick={() =>
                      setItems(
                        items.map((item, itemIndex) =>
                          itemIndex === index
                            ? {
                                ...item,
                                allocations: [
                                  ...item.allocations,
                                  {
                                    lotId: "",
                                    quantity: item.trackingMode === "serial" ? "1" : "",
                                  },
                                ],
                              }
                            : item,
                        ),
                      )
                    }
                  >
                    Add {l.trackingMode}
                  </Button>
                </Stack>
              )}
              <Button appearance="standard"
                variant="secondary"
                type="button"

                onClick={() => setItems(items.filter((_, i) => i !== index))}
              >
                Remove transfer product {index + 1}
              </Button>
            </FormGrid>
          );
        })}
        <Button appearance="standard"
          clientManaged type="submit"
          variant="primary"

          disabled={pending || !items.length || !source || !target}
        >
          {pending ? "Saving…" : transfer ? "Save transfer changes" : "Save transfer draft"}
        </Button>
      </form>
    </FormDisclosure>
  );
}
export function TransferActions({
  base,
  transfer,
  editable,
  reviewable,
  stale,
}: {
  base: string;
  transfer: Row;
  editable: boolean;
  reviewable: boolean;
  stale: boolean;
}) {
  const { send, error, pending } = useTransferCommand(base),
    [reason, setReason] = useState(""),
    [directEvidence, setDirectEvidence] = useState<AttachmentPanelChange | null>(null);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Transfer workflow</h2>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {stale && !["posted", "cancelled"].includes(String(transfer.status)) && (
        <p role="note" className="error">
          Source stock changed. Return a submitted transfer and refresh its quantities before
          posting.
        </p>
      )}
      {(editable || reviewable) && (
        <>
          <Field appearance="standard">
            Receipt or decision reason
            <TextArea appearance="standard"
              maxLength={2000}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Confirm the receiving count or explain the correction"
            />
          </Field>
          {reviewable && transfer.delivery_mode === "direct" && (
            <AttachmentPanel appearance="standard"
              base={base}
              targetType="inventory_transfer"
              targetId={String(transfer.id)}
              expectedTargetVersion={Number(transfer.version)}
              componentKind="action"
              componentRef="direct-receipt"
              purposeKey="direct_receipt_evidence"
              title="Direct transfer receipt evidence"
              description="Attach the receiving count sheet, signed handover, or arrival photos reviewed before stock posts at both warehouses."
              visibility="Visible to authorized Inventory and Finance reviewers for the source or receiving Branch."
              onChange={setDirectEvidence}
            />
          )}
          <div className="ui-form-actions">
            {editable && (
              <>
                <Button appearance="standard"
                  type="submit"
                  variant="primary"

                  disabled={pending || stale}
                  onClick={() =>
                    void send(`/${transfer.id}/submit`, {
                      expectedVersion: transfer.version,
                    })
                  }
                >
                  {transfer.delivery_mode === "transit"
                    ? "Submit transport for approval"
                    : "Submit transfer for receipt review"}
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${transfer.id}/cancel`, {
                      expectedVersion: transfer.version,
                      reason,
                    })
                  }
                >
                  Cancel transfer
                </Button>
              </>
            )}
            {reviewable && (
              <>
                <Button appearance="standard"
                  type="submit"
                  variant="primary"

                  disabled={pending || stale || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${transfer.id}/post`, {
                      expectedVersion: transfer.version,
                      reason,
                      attachmentSetIds:
                        directEvidence?.setId &&
                        directEvidence.readyCount > 0 &&
                        directEvidence.bindable
                          ? [directEvidence.setId]
                          : [],
                    })
                  }
                >
                  {transfer.delivery_mode === "transit"
                    ? "Approve transport for dispatch"
                    : "Confirm receipt and post transfer"}
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${transfer.id}/reject`, {
                      expectedVersion: transfer.version,
                      reason,
                    })
                  }
                >
                  Return transfer for correction
                </Button>
              </>
            )}
          </div>
        </>
      )}
      {!editable && !reviewable && (
        <p className="muted">
          {transfer.status === "submitted"
            ? "Awaiting an independent receiving reviewer."
            : transfer.status === "posted"
              ? transfer.delivery_mode === "transit"
                ? "Transport approved. Continue in the dispatch and receiving record below."
                : "The source and receiving stock movements are posted. Use a new transfer to move stock back."
              : transfer.status === "cancelled"
                ? "This transfer was cancelled; its history is retained."
                : "Only the source preparer can edit this transfer."}
        </p>
      )}
    </Card>
  );
}
