"use client";
import { InlineNotice } from "@/components/ui/foundation";

import {CountForm,CountDisclosure} from "./count-draft-boundary";
import {useCountCommand} from "./use-count-command";
import {countEditorDraft,acknowledgedCountDraft,currentCountDraft,type CountEditorDraft,type CountItem,type CountAllocation} from "@/lib/inventory/count-editor-draft";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import type { CountRow as Row } from "@/server/modules/inventory/public/inventory-counts";
import { minorToDecimal } from "@/lib/finance/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { FormGrid, Stack, Card } from "@/components/ui/foundation";
type Options = {
  warehouses: Row[];
  products: Row[];
  stock: Row[];
  lots: Row[];
  currency: string;
};
export function CountEditor({
  base,
  options,
  count,
  lines,
  lotLines,
  initialOpen = false,
  drawer = false,
}: {
  base: string;
  options: Options;
  count?: Row;
  lines?: Row[];
  lotLines?: Row[];
  initialOpen?: boolean;
  drawer?: boolean;
}) {
  const {send,retry,error,notice,pending,uncertain,canPrepare}=useCountCommand(base);
  const [seed]=useState(()=>countEditorDraft(options,count,lines,lotLines));
  const baseline=useRef(seed),sourceRef=useRef(seed.source);
  const [source,setSource]=useState(seed.source);
  const [warehouse,setWarehouse]=useState(seed.warehouse),[items,setItems]=useState<CountItem[]>(seed.items),[adding,setAdding]=useState(seed.adding),[countDate,setCountDate]=useState(seed.countDate),[countReason,setCountReason]=useState(seed.reason),[generation,setGeneration]=useState(0),[editorOpen,setEditorOpen]=useState(initialOpen);
  const inline=initialOpen && !drawer && !count;
  function resetDraft(){const next=currentCountDraft(baseline.current,countEditorDraft(options,count,lines,lotLines));baseline.current=next;sourceRef.current=next.source;setSource(next.source);setWarehouse(next.warehouse);setItems(next.items);setAdding(next.adding);setCountDate(next.countDate);setCountReason(next.reason);setGeneration(value=>value+1);}
  if(!canPrepare && !editorOpen && !inline)return <p role="status">New counts and recounts are unavailable for the current package.</p>;
  return (
    <>{notice && <p role="status">{notice}</p>}{uncertain && <Button appearance="standard" type="button" variant="secondary" disabled={pending} onClick={()=>void retry().then(saved=>{if(saved)setEditorOpen(false);})}>Retry submitted count change</Button>}<CountDisclosure appearance="standard" inline={inline} open={editorOpen} onOpenChange={setEditorOpen} onBeforeOpen={resetDraft} drawerTitle={source ? "Edit or recount" : "New physical count"} error={error || undefined} dismissDisabled={pending} onDiscard={()=>{resetDraft();setEditorOpen(false);}}>
      <summary>{source ? "Edit or recount" : "New physical count"}</summary>
      <p className="muted">
        Record the quantities physically counted. Saving captures the current stock balance;
        movement during review requires a recount. Existing stock keeps its average cost.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <CountForm key={generation}
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget),submitted:CountEditorDraft={warehouse,items,adding,countDate,reason:countReason,...(sourceRef.current?{source:sourceRef.current}:{})};
          void send(
            sourceRef.current ? `/${sourceRef.current.id}` : "",
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
              ...(sourceRef.current ? { expectedVersion: sourceRef.current.version } : {}),
            },
            !sourceRef.current,
            receipt=>{baseline.current=acknowledgedCountDraft(submitted,receipt);sourceRef.current=baseline.current.source;setSource(baseline.current.source);},
          ).then(saved=>{if(saved)setEditorOpen(false);});
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Count warehouse
            <Select appearance="standard"
              required
              disabled={!!source}
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
              value={countDate} onChange={event=>setCountDate(event.target.value)}
            />
          </Field>
          <Field appearance="standard">
            Count reason
            <TextInput appearance="standard"
              name="reason"
              required
              minLength={5}
              maxLength={2000}
              value={countReason} onChange={event=>setCountReason(event.target.value)}
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

          disabled={pending || !canPrepare || !items.length || !warehouse}
        >
          {pending ? "Saving…" : source ? "Save recount" : "Save physical count"}
        </Button>
      </CountForm>
    </CountDisclosure></>
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
  const { send,retry,error,notice,pending,uncertain,canPrepare,canSettle } = useCountCommand(base),
    [reason, setReason] = useState("");
  const [attachmentSets, setAttachmentSets] = useState<Record<string, AttachmentPanelChange>>({});
  const [workflowOpen,setWorkflowOpen]=useState(false),[generation,setGeneration]=useState(0);
  const visible=editable && (canPrepare || canSettle) || reviewable && canSettle || workflowOpen;
  function reset(){setReason("");setAttachmentSets({});setGeneration(value=>value+1);}
  const attachmentSetIds = Object.values(attachmentSets)
    .filter((item) => item.setId && item.readyCount > 0 && item.bindable)
    .map((item) => item.setId!);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Count workflow</h2>
      {notice && <p role="status">{notice}</p>}{uncertain && <Button appearance="standard" type="button" variant="secondary" disabled={pending} onClick={()=>void retry()}>Retry submitted count change</Button>}{error && (
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
      {visible && (
        <CountDisclosure appearance="standard" open={workflowOpen} onOpenChange={setWorkflowOpen} onBeforeOpen={reset} drawerTitle="Count workflow" error={error || undefined} dismissDisabled={pending} onDiscard={()=>{reset();setWorkflowOpen(false);}}><summary>{reviewable ? "Review count decision" : "Prepare count submission or cancellation"}</summary><CountForm key={generation} onSubmit={event=>event.preventDefault()}>
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
                compact readOnly={!canPrepare}
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
                  type="button"
                  variant="primary"

                  disabled={pending || !canPrepare || stale}
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
                  type="button"
                  variant="secondary"

                  disabled={pending || !canSettle || reason.trim().length < 3}
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
                  type="button"
                  variant="primary"

                  disabled={pending || !canSettle || stale || reason.trim().length < 3}
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
                  type="button"
                  variant="secondary"

                  disabled={pending || !canSettle || reason.trim().length < 3}
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
        </CountForm></CountDisclosure>
      )}
      {!visible && (
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
