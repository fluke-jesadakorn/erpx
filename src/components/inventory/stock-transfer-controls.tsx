"use client";
import { InlineNotice } from "@/components/ui/foundation";

import {TransferForm,TransferDisclosure} from "./transfer-draft-boundary";
import {useTransferCommand} from "./use-transfer-command";
import {transferEditorDraft,acknowledgedTransferDraft,currentTransferDraft,type TransferItem,type TransferEditorDraft} from "@/lib/inventory/transfer-editor-draft";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import type { TransferRow as Row } from "@/server/modules/inventory/public/inventory-transfers";
import { minorToDecimal } from "@/lib/finance/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { FormGrid, Stack, Card } from "@/components/ui/foundation";
type Options = { warehouses: Row[]; sources: Row[]; stock: Row[]; lots: Row[] };
export function TransferEditor({
  base,
  options,
  transfer,
  lines,
  allocations,
  initialOpen = false,
  drawer = false,
}: {
  base: string;
  options: Options;
  transfer?: Row;
  lines?: Row[];
  allocations?: Row[];
  initialOpen?: boolean;
  drawer?: boolean;
}) {
  const {send,retry,error,notice,pending,uncertain,canPrepare}=useTransferCommand(base);
  const [seed]=useState(()=>transferEditorDraft(options,transfer,lines,allocations)),baseline=useRef(seed),recordRef=useRef(seed.record);
  const [record,setRecord]=useState(seed.record),[source,setSource]=useState(seed.source),[target,setTarget]=useState(seed.target),[product,setProduct]=useState(seed.product),[items,setItems]=useState<TransferItem[]>(seed.items),[date,setDate]=useState(seed.date),[due,setDue]=useState(seed.due),[mode,setMode]=useState(seed.mode),[reason,setReason]=useState(seed.reason),[generation,setGeneration]=useState(0),[editorOpen,setEditorOpen]=useState(initialOpen);
  const inline=initialOpen&&!drawer&&!transfer;
  function resetDraft(){const next=currentTransferDraft(baseline.current,transferEditorDraft(options,transfer,lines,allocations));baseline.current=next;recordRef.current=next.record;setRecord(next.record);setSource(next.source);setTarget(next.target);setProduct(next.product);setItems(next.items);setDate(next.date);setDue(next.due);setMode(next.mode);setReason(next.reason);setGeneration(value=>value+1);}
  if(!canPrepare&&!editorOpen&&!inline)return <p role="status">New transfer plans and amendments are unavailable for the current package.</p>;
  const available = options.stock.filter(
    (s) => s.warehouse_id === source && !items.some((l) => l.productId === s.product_id),
  );
  return (
    <>{notice&&<p role="status">{notice}</p>}{uncertain&&<Button appearance="standard" type="button" variant="secondary" disabled={pending} onClick={()=>void retry().then(saved=>{if(saved)setEditorOpen(false);})}>Retry submitted transfer change</Button>}<TransferDisclosure appearance="standard" inline={inline} open={editorOpen} onOpenChange={setEditorOpen} onBeforeOpen={resetDraft} drawerTitle={record?"Edit transfer quantities":"New warehouse transfer"} error={error||undefined} dismissDisabled={pending} onDiscard={()=>{resetDraft();setEditorOpen(false);}}>
      <summary>{record ? "Edit transfer quantities" : "New warehouse transfer"}</summary>
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
      <TransferForm key={generation}
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget),submitted:TransferEditorDraft={source,target,product,items,date,due,mode,reason,...(recordRef.current?{record:recordRef.current}:{})};
          void send(
            recordRef.current ? `/${recordRef.current.id}` : "",
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
              ...(recordRef.current ? { expectedVersion: recordRef.current.version } : {}),
            },
            !recordRef.current,
            receipt=>{baseline.current=acknowledgedTransferDraft(submitted,receipt);recordRef.current=baseline.current.record;setRecord(baseline.current.record);},
          ).then(saved=>{if(saved)setEditorOpen(false);});
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Transport mode
            <Select appearance="standard"
              name="deliveryMode"
              value={mode} onChange={event=>setMode(event.target.value)}
              disabled={!!record}
            >
              <option value="direct">Completed direct movement</option>
              <option value="transit">Dispatch and receive separately</option>
            </Select>
            {record && (
              <TextInput appearance="standard" type="hidden" name="deliveryMode" value={mode} />
            )}
          </Field>
          <Field appearance="standard">
            Expected arrival (staged transport)
            <TextInput appearance="standard"
              type="date"
              name="arrivalDueDate"
              value={due} onChange={event=>setDue(event.target.value)}
            />
          </Field>
          <Field appearance="standard">
            Source warehouse
            <Select appearance="standard"
              required
              disabled={!!record}
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
              disabled={!!record}
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
              value={date} onChange={event=>setDate(event.target.value)}
            />
          </Field>
          <Field appearance="standard">
            Transfer purpose
            <TextInput appearance="standard"
              name="reason"
              required
              minLength={5}
              maxLength={2000}
              value={reason} onChange={event=>setReason(event.target.value)}
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

          disabled={pending || !canPrepare || !items.length || !source || !target || !!(transfer && record && transfer.id===record.id && Number(transfer.version)>record.version)}
        >
          {pending ? "Saving…" : record ? "Save transfer changes" : "Save transfer draft"}
        </Button>
      </TransferForm>
    </TransferDisclosure></>
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
  const { send,retry,error,notice,pending,uncertain,canPrepare,canSettle } = useTransferCommand(base),
    [reason, setReason] = useState(""),
    [directEvidence, setDirectEvidence] = useState<AttachmentPanelChange | null>(null),[generation,setGeneration]=useState(0),[open,setOpen]=useState(false);
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
      {notice&&<p role="status">{notice}</p>}{uncertain&&<Button appearance="standard" type="button" variant="secondary" disabled={pending} onClick={()=>void retry().then(saved=>{if(saved)setOpen(false);})}>Retry submitted transfer decision</Button>}
      {(editable || reviewable) && (canPrepare || canSettle) && (
        <TransferDisclosure drawerTitle="Transfer receipt or decision" open={open} onOpenChange={setOpen} dismissDisabled={pending} onDiscard={()=>{setReason("");setDirectEvidence(null);setGeneration(value=>value+1);}}><summary>Review transfer workflow</summary><TransferForm key={generation} onSubmit={event=>event.preventDefault()}>
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
                  type="button"
                  variant="primary"

                  disabled={pending || !canPrepare || stale}
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
                  type="button"
                  variant="secondary"

                  disabled={pending || !canSettle || reason.trim().length < 3}
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
                  type="button"
                  variant="primary"

                  disabled={pending || !canSettle || stale || reason.trim().length < 3}
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
                  type="button"
                  variant="secondary"

                  disabled={pending || !canSettle || reason.trim().length < 3}
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
        </TransferForm></TransferDisclosure>
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
