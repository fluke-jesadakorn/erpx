"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import type { CountRow as Row } from "@/server/modules/inventory/public/inventory-counts";
import { minorToDecimal } from "@/lib/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { FormGrid, Stack, Card } from "@/components/ui/foundation";
type Options = {
  warehouses: Row[];
  products: Row[];
  stock: Row[];
  lots: Row[];
  currency: string;
};
type CountAllocation = {
  lotId?: string;
  lotCode: string;
  serialNumber: string;
  manufacturedDate: string;
  expiryDate: string;
  countedQuantity: string;
  unitCost: string;
};
type CountItem = {
  productId: string;
  trackingMode: string;
  countedQuantity: string;
  unitCost: string;
  allocations: CountAllocation[];
};
function useCountCommand(base: string) {
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
      const r = await workFetch(`/api/v1${base}/stock-counts${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": last.current.key,
        },
        body,
      });
      const v = (await r.json()) as {
        data: { id: string };
        error?: { message: string };
      };
      if (!r.ok) throw new Error(v.error?.message ?? "Unable to save count.");
      last.current = null;
      if (create) router.push(`${base}/stock-counts/${v.data.id}`);
      router.refresh();
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save count.");
      return false;
    } finally {
      setPending(false);
    }
  }
  return { send, error, pending };
}
export function CountEditor({
  base,
  options,
  count,
  lines,
  lotLines,
  initialOpen = false,
}: {
  base: string;
  options: Options;
  count?: Row;
  lines?: Row[];
  lotLines?: Row[];
  initialOpen?: boolean;
}) {
  const { send, error, pending } = useCountCommand(base),
    [warehouse, setWarehouse] = useState(
      String(count?.warehouse_id ?? options.warehouses[0]?.id ?? ""),
    );
  const [items, setItems] = useState<CountItem[]>(
    (lines ?? []).map((l) => ({
      productId: String(l.product_id),
      trackingMode: String(l.tracking_mode ?? "none"),
      countedQuantity: String(Number(l.counted_quantity_milli) / 1000),
      unitCost: l.unit_cost_minor == null ? "" : minorToDecimal(Number(l.unit_cost_minor)),
      allocations: (lotLines ?? [])
        .filter((a) => a.count_line_id === l.id)
        .map((a) => ({
          ...(a.source_lot_id ? { lotId: String(a.source_lot_id) } : {}),
          lotCode: String(a.lot_code ?? ""),
          serialNumber: String(a.serial_number ?? ""),
          manufacturedDate: String(a.manufactured_date ?? ""),
          expiryDate: String(a.expiry_date ?? ""),
          countedQuantity: String(Number(a.counted_quantity_milli) / 1000),
          unitCost: a.unit_cost_minor == null ? "" : minorToDecimal(Number(a.unit_cost_minor)),
        })),
    })),
  );
  const [adding, setAdding] = useState(String(options.products[0]?.id ?? ""));
  return (
    <FormDisclosure appearance="standard" inline={Boolean(!!count || initialOpen)} className="ui-panel" open={!!count || initialOpen}>
      <summary>{count ? "Edit or recount" : "New physical count"}</summary>
      <p className="muted">
        Record the quantities physically counted. Saving captures the current stock balance;
        movement during review requires a recount. Existing stock keeps its average cost.
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
          void send(
            count ? `/${count.id}` : "",
            {
              warehouseId: warehouse,
              countDate: f.get("countDate"),
              reason: f.get("reason"),
              lines: items.map((l) => ({
                productId: l.productId,
                countedQuantity: l.countedQuantity,
                ...(l.unitCost ? { unitCost: l.unitCost } : {}),
                allocations: l.allocations.map((a) => ({
                  ...(a.lotId ? { lotId: a.lotId } : {}),
                  ...(a.lotCode ? { lotCode: a.lotCode } : {}),
                  ...(a.serialNumber ? { serialNumber: a.serialNumber } : {}),
                  manufacturedDate: a.manufacturedDate,
                  expiryDate: a.expiryDate,
                  countedQuantity: a.countedQuantity,
                  ...(a.unitCost ? { unitCost: a.unitCost } : {}),
                })),
              })),
              ...(count ? { expectedVersion: count.version } : {}),
            },
            !count,
          );
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Count warehouse
            <Select appearance="standard"
              required
              disabled={!!count}
              value={warehouse}
              onChange={(e) => {
                setWarehouse(e.target.value);
                setItems([]);
              }}
            >
              {options.warehouses.map((w) => (
                <option key={String(w.id)} value={String(w.id)}>
                  {String(w.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Count date
            <TextInput appearance="standard"
              name="countDate"
              type="date"
              required
              defaultValue={String(count?.count_date ?? new Date().toISOString().slice(0, 10))}
            />
          </Field>
          <Field appearance="standard">
            Count reason
            <TextInput appearance="standard"
              name="reason"
              required
              minLength={5}
              maxLength={2000}
              defaultValue={String(count?.reason ?? "")}
              placeholder="Cycle count, damage review, or unrecorded stock"
            />
          </Field>
        </FormGrid>
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Product to count
            <Select appearance="standard" value={adding} onChange={(e) => setAdding(e.target.value)}>
              {options.products.map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {String(p.sku)} · {String(p.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Button appearance="standard"
            variant="secondary"
            type="button"

            disabled={!adding || items.length >= 50 || items.some((l) => l.productId === adding)}
            onClick={() => {
              const product = options.products.find((p) => p.id === adding),
                trackingMode = String(product?.tracking_mode ?? "none");
              const allocations: CountAllocation[] =
                trackingMode === "none"
                  ? []
                  : options.lots
                      .filter((l) => l.product_id === adding && l.warehouse_id === warehouse)
                      .map((l) => ({
                        lotId: String(l.id),
                        lotCode: String(l.lot_code ?? ""),
                        serialNumber: String(l.serial_number ?? ""),
                        manufacturedDate: String(l.manufactured_date ?? ""),
                        expiryDate: String(l.expiry_date ?? ""),
                        countedQuantity: String(Number(l.quantity_milli) / 1000),
                        unitCost: "",
                      }));
              setItems([
                ...items,
                {
                  productId: adding,
                  trackingMode,
                  countedQuantity:
                    trackingMode === "none"
                      ? ""
                      : String(allocations.reduce((sum, a) => sum + Number(a.countedQuantity), 0)),
                  unitCost: "",
                  allocations,
                },
              ]);
            }}
          >
            Add counted product
          </Button>
        </FormGrid>
        {items.map((l, index) => {
          const p = options.products.find((p) => p.id === l.productId),
            stock = options.stock.find(
              (s) => s.product_id === l.productId && s.warehouse_id === warehouse,
            );
          return (
            <FormGrid as="div" className="ui-form-grid" key={l.productId}>
              <div>
                <strong>{String(p?.sku ?? l.productId)}</strong>
                <p className="muted">
                  {String(p?.name ?? "")}
                  <br />
                  Recorded: {Number(stock?.quantity_milli ?? 0) / 1000} {String(p?.unit ?? "")} ·{" "}
                  {options.currency} {minorToDecimal(Number(stock?.value_minor ?? 0))}
                </p>
              </div>
              {l.trackingMode === "none" && (
                <Field appearance="standard">
                  Counted quantity {index + 1}
                  <TextInput appearance="standard"
                    type="number"
                    min="0"
                    step="0.001"
                    required
                    value={l.countedQuantity}
                    onChange={(e) =>
                      setItems(
                        items.map((x, i) =>
                          i === index ? { ...x, countedQuantity: e.target.value } : x,
                        ),
                      )
                    }
                  />
                </Field>
              )}
              {l.trackingMode === "none" && Number(stock?.quantity_milli ?? 0) === 0 ? (
                <Field appearance="standard">
                  Verified unit cost {index + 1} ({options.currency})
                  <TextInput appearance="standard"
                    type="number"
                    min="0.01"
                    step="0.01"
                    required={Number(l.countedQuantity) > 0}
                    value={l.unitCost}
                    onChange={(e) =>
                      setItems(
                        items.map((x, i) => (i === index ? { ...x, unitCost: e.target.value } : x)),
                      )
                    }
                  />
                </Field>
              ) : l.trackingMode === "none" ? (
                <span className="muted">Valuation follows the recorded average cost.</span>
              ) : null}
              {l.trackingMode !== "none" && (
                <Stack as="div" className="ui-stack">
                  <p>
                    <strong>{l.trackingMode === "serial" ? "Serial count" : "Lot count"}</strong> ·
                    total {l.countedQuantity || "0"} {String(p?.unit ?? "")}
                  </p>
                  {l.allocations.map((a, allocationIndex) => (
                    <FormGrid as="div" className="ui-form-grid" key={a.lotId ?? `new-${allocationIndex}`}>
                      <Field appearance="standard">
                        {l.trackingMode === "serial" ? "Serial number" : "Lot code"}
                        <TextInput appearance="standard"
                          required
                          readOnly={!!a.lotId}
                          value={l.trackingMode === "serial" ? a.serialNumber : a.lotCode}
                          onChange={(e) =>
                            setItems(
                              items.map((x, i) =>
                                i !== index
                                  ? x
                                  : {
                                      ...x,
                                      allocations: x.allocations.map((v, j) =>
                                        j !== allocationIndex
                                          ? v
                                          : {
                                              ...v,
                                              ...(l.trackingMode === "serial"
                                                ? { serialNumber: e.target.value }
                                                : { lotCode: e.target.value }),
                                            },
                                      ),
                                    },
                              ),
                            )
                          }
                        />
                      </Field>
                      <Field appearance="standard">
                        Counted quantity
                        <TextInput appearance="standard"
                          type="number"
                          min={a.lotId ? "0" : "0.001"}
                          max={l.trackingMode === "serial" ? "1" : undefined}
                          step={l.trackingMode === "serial" ? "1" : "0.001"}
                          required
                          value={a.countedQuantity}
                          onChange={(e) =>
                            setItems(
                              items.map((x, i) => {
                                if (i !== index) return x;
                                const allocations = x.allocations.map((v, j) =>
                                  j === allocationIndex
                                    ? { ...v, countedQuantity: e.target.value }
                                    : v,
                                );
                                return {
                                  ...x,
                                  allocations,
                                  countedQuantity: String(
                                    allocations.reduce(
                                      (sum, v) => sum + (Number(v.countedQuantity) || 0),
                                      0,
                                    ),
                                  ),
                                };
                              }),
                            )
                          }
                        />
                      </Field>
                      {!a.lotId && (
                        <>
                          <Field appearance="standard">
                            Manufactured date
                            <TextInput appearance="standard"
                              type="date"
                              value={a.manufacturedDate}
                              onChange={(e) =>
                                setItems(
                                  items.map((x, i) =>
                                    i !== index
                                      ? x
                                      : {
                                          ...x,
                                          allocations: x.allocations.map((v, j) =>
                                            j === allocationIndex
                                              ? { ...v, manufacturedDate: e.target.value }
                                              : v,
                                          ),
                                        },
                                  ),
                                )
                              }
                            />
                          </Field>
                          <Field appearance="standard">
                            Expiry date
                            <TextInput appearance="standard"
                              type="date"
                              value={a.expiryDate}
                              onChange={(e) =>
                                setItems(
                                  items.map((x, i) =>
                                    i !== index
                                      ? x
                                      : {
                                          ...x,
                                          allocations: x.allocations.map((v, j) =>
                                            j === allocationIndex
                                              ? { ...v, expiryDate: e.target.value }
                                              : v,
                                          ),
                                        },
                                  ),
                                )
                              }
                            />
                          </Field>
                          <Field appearance="standard">
                            Verified unit cost ({options.currency})
                            <TextInput appearance="standard"
                              type="number"
                              min="0.01"
                              step="0.01"
                              required
                              value={a.unitCost}
                              onChange={(e) =>
                                setItems(
                                  items.map((x, i) =>
                                    i !== index
                                      ? x
                                      : {
                                          ...x,
                                          allocations: x.allocations.map((v, j) =>
                                            j === allocationIndex
                                              ? { ...v, unitCost: e.target.value }
                                              : v,
                                          ),
                                        },
                                  ),
                                )
                              }
                            />
                          </Field>
                        </>
                      )}
                      {!a.lotId && (
                        <Button appearance="standard"
                          variant="secondary"
                          type="button"
                          onClick={() =>
                            setItems(
                              items.map((x, i) => {
                                if (i !== index) return x;
                                const allocations = x.allocations.filter(
                                  (_, j) => j !== allocationIndex,
                                );
                                return {
                                  ...x,
                                  allocations,
                                  countedQuantity: String(
                                    allocations.reduce(
                                      (sum, v) => sum + (Number(v.countedQuantity) || 0),
                                      0,
                                    ),
                                  ),
                                };
                              }),
                            )
                          }
                        >
                          Remove found {l.trackingMode}
                        </Button>
                      )}
                    </FormGrid>
                  ))}
                  <Button appearance="standard"
                    variant="secondary"
                    type="button"
                    onClick={() =>
                      setItems(
                        items.map((x, i) =>
                          i !== index
                            ? x
                            : {
                                ...x,
                                allocations: [
                                  ...x.allocations,
                                  {
                                    lotCode: "",
                                    serialNumber: "",
                                    manufacturedDate: "",
                                    expiryDate: "",
                                    countedQuantity: l.trackingMode === "serial" ? "1" : "",
                                    unitCost: "",
                                  },
                                ],
                                countedQuantity: String(
                                  x.allocations.reduce(
                                    (sum, v) => sum + (Number(v.countedQuantity) || 0),
                                    0,
                                  ) + (l.trackingMode === "serial" ? 1 : 0),
                                ),
                              },
                        ),
                      )
                    }
                  >
                    Add found {l.trackingMode}
                  </Button>
                </Stack>
              )}
              <Button appearance="standard"
                variant="secondary"
                type="button"

                onClick={() => setItems(items.filter((_, i) => i !== index))}
              >
                Remove product {index + 1}
              </Button>
            </FormGrid>
          );
        })}
        <Button appearance="standard"
          clientManaged type="submit"
          variant="primary"

          disabled={pending || !items.length || !warehouse}
        >
          {pending ? "Saving…" : count ? "Save recount" : "Save physical count"}
        </Button>
      </form>
    </FormDisclosure>
  );
}
export function CountActions({
  base,
  count,
  editable,
  reviewable,
  stale,
}: {
  base: string;
  count: Row;
  editable: boolean;
  reviewable: boolean;
  stale: boolean;
}) {
  const { send, error, pending } = useCountCommand(base),
    [reason, setReason] = useState("");
  const [attachmentSets, setAttachmentSets] = useState<Record<string, AttachmentPanelChange>>({});
  const attachmentSetIds = Object.values(attachmentSets)
    .filter((item) => item.setId && item.readyCount > 0 && item.bindable)
    .map((item) => item.setId!);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Count workflow</h2>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {stale && count.status !== "posted" && (
        <p role="note" className="error">
          Stock changed after this count was saved. Return the count if submitted, then recount
          before approval.
        </p>
      )}
      {(editable || reviewable) && (
        <>
          {editable &&
            (
              [
                [
                  "count_sheet",
                  "Count sheets",
                  "Attach signed count sheets or exported count documents.",
                ],
                [
                  "count_photo",
                  "Count photos",
                  "Attach shelf, bin, label, lot, or serial photos from the physical count.",
                ],
                [
                  "count_evidence",
                  "Other count evidence",
                  "Attach other documents supporting the counted quantities.",
                ],
              ] as const
            ).map(([purpose, title, description]) => (
              <AttachmentPanel appearance="standard"
                key={purpose}
                base={base}
                targetType="stock_count"
                targetId={String(count.id)}
                expectedTargetVersion={Number(count.version)}
                componentKind="action"
                componentRef="submit"
                purposeKey={purpose}
                compact
                title={title}
                description={description}
                visibility="Inventory preparers and authorized independent Finance reviewers can access these files."
                onChange={(change) =>
                  setAttachmentSets((current) => ({ ...current, [purpose]: change }))
                }
              />
            ))}
          <Field appearance="standard">
            Decision reason
            <TextArea appearance="standard"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              maxLength={2000}
              placeholder="Explain the approval, correction request, or cancellation"
            />
          </Field>
          <div className="ui-form-actions">
            {editable && (
              <>
                <Button appearance="standard"
                  type="submit"
                  variant="primary"

                  disabled={pending || stale}
                  onClick={() =>
                    void send(`/${count.id}/submit`, {
                      expectedVersion: count.version,
                      attachmentSetIds,
                    })
                  }
                >
                  Submit count for approval
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${count.id}/cancel`, {
                      expectedVersion: count.version,
                      reason,
                    })
                  }
                >
                  Cancel count
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
                    void send(`/${count.id}/${count.status === "approved" ? "post" : "approve"}`, {
                      expectedVersion: count.version,
                      reason,
                    })
                  }
                >
                  {count.status === "approved"
                    ? "Review Debit / Credit and post GL"
                    : "Approve count for GL review"}
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${count.id}/reject`, {
                      expectedVersion: count.version,
                      reason,
                    })
                  }
                >
                  Return count for correction
                </Button>
              </>
            )}
          </div>
        </>
      )}
      {!editable && !reviewable && (
        <p className="muted">
          {count.status === "submitted"
            ? "Awaiting an independent holding finance approver."
            : count.status === "posted"
              ? "The stock adjustment is posted. Record a new count for later corrections."
              : count.status === "cancelled"
                ? "This count was cancelled. Its history is retained."
                : "Only the preparer can edit this count."}
        </p>
      )}
    </Card>
  );
}
