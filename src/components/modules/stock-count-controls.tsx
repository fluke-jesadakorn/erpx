"use client";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import type { CountRow as Row } from "@/server/inventory-counts";
import { minorToDecimal } from "@/lib/finance-money";
type Options = {
  warehouses: Row[];
  products: Row[];
  stock: Row[];
  lots: Row[];
  currency: string;
};
type CountAllocation = { lotId?: string; lotCode: string; serialNumber: string; manufacturedDate: string; expiryDate: string; countedQuantity: string; unitCost: string };
type CountItem = { productId: string; trackingMode: string; countedQuantity: string; unitCost: string; allocations: CountAllocation[] };
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
      unitCost:
        l.unit_cost_minor == null
          ? ""
          : minorToDecimal(Number(l.unit_cost_minor)),
      allocations: (lotLines ?? []).filter(a => a.count_line_id === l.id).map(a => ({
        ...(a.source_lot_id ? { lotId: String(a.source_lot_id) } : {}),
        lotCode: String(a.lot_code ?? ""), serialNumber: String(a.serial_number ?? ""),
        manufacturedDate: String(a.manufactured_date ?? ""), expiryDate: String(a.expiry_date ?? ""),
        countedQuantity: String(Number(a.counted_quantity_milli) / 1000),
        unitCost: a.unit_cost_minor == null ? "" : minorToDecimal(Number(a.unit_cost_minor)),
      })),
    })),
  );
  const [adding, setAdding] = useState(String(options.products[0]?.id ?? ""));
  return (
    <details className="card" open={!!count || initialOpen}>
      <summary>{count ? "Edit or recount" : "New physical count"}</summary>
      <p className="muted">
        Record the quantities physically counted. Saving captures the current
        stock balance; movement during review requires a recount. Existing stock
        keeps its average cost.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
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
                allocations: l.allocations.map(a => ({
                  ...(a.lotId ? { lotId: a.lotId } : {}), ...(a.lotCode ? { lotCode: a.lotCode } : {}),
                  ...(a.serialNumber ? { serialNumber: a.serialNumber } : {}), manufacturedDate: a.manufacturedDate,
                  expiryDate: a.expiryDate, countedQuantity: a.countedQuantity, ...(a.unitCost ? { unitCost: a.unitCost } : {}),
                })),
              })),
              ...(count ? { expectedVersion: count.version } : {}),
            },
            !count,
          );
        }}
      >
        <div className="business-form">
          <label>
            Count warehouse
            <select
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
            </select>
          </label>
          <label>
            Count date
            <input
              name="countDate"
              type="date"
              required
              defaultValue={String(
                count?.count_date ?? new Date().toISOString().slice(0, 10),
              )}
            />
          </label>
          <label>
            Count reason
            <input
              name="reason"
              required
              minLength={5}
              maxLength={2000}
              defaultValue={String(count?.reason ?? "")}
              placeholder="Cycle count, damage review, or unrecorded stock"
            />
          </label>
        </div>
        <div className="business-form">
          <label>
            Product to count
            <select value={adding} onChange={(e) => setAdding(e.target.value)}>
              {options.products.map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {String(p.sku)} · {String(p.name)}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="button secondary"
            disabled={
              !adding ||
              items.length >= 50 ||
              items.some((l) => l.productId === adding)
            }
            onClick={() => {
              const product=options.products.find(p=>p.id===adding),trackingMode=String(product?.tracking_mode??"none");
              const allocations:CountAllocation[]=trackingMode==="none"?[]:options.lots.filter(l=>l.product_id===adding&&l.warehouse_id===warehouse).map(l=>({lotId:String(l.id),lotCode:String(l.lot_code??""),serialNumber:String(l.serial_number??""),manufacturedDate:String(l.manufactured_date??""),expiryDate:String(l.expiry_date??""),countedQuantity:String(Number(l.quantity_milli)/1000),unitCost:""}));
              setItems([...items,{productId:adding,trackingMode,countedQuantity:trackingMode==="none"?"":String(allocations.reduce((sum,a)=>sum+Number(a.countedQuantity),0)),unitCost:"",allocations}]);
            }}
          >
            Add counted product
          </button>
        </div>
        {items.map((l, index) => {
          const p = options.products.find((p) => p.id === l.productId),
            stock = options.stock.find(
              (s) =>
                s.product_id === l.productId && s.warehouse_id === warehouse,
            );
          return (
            <div className="business-form" key={l.productId}>
              <div>
                <strong>{String(p?.sku ?? l.productId)}</strong>
                <p className="muted">
                  {String(p?.name ?? "")}
                  <br />
                  Recorded: {Number(stock?.quantity_milli ?? 0) / 1000}{" "}
                  {String(p?.unit ?? "")} · {options.currency}{" "}
                  {minorToDecimal(Number(stock?.value_minor ?? 0))}
                </p>
              </div>
              {l.trackingMode==="none"&&<label>
                Counted quantity {index + 1}
                <input type="number" min="0" step="0.001" required value={l.countedQuantity}
                  onChange={(e) => setItems(items.map((x, i) => i === index ? { ...x, countedQuantity: e.target.value } : x))}/>
              </label>}
              {l.trackingMode==="none"&&Number(stock?.quantity_milli ?? 0) === 0 ? (
                <label>
                  Verified unit cost {index + 1} ({options.currency})
                  <input
                    type="number"
                    min="0.01"
                    step="0.01"
                    required={Number(l.countedQuantity) > 0}
                    value={l.unitCost}
                    onChange={(e) =>
                      setItems(
                        items.map((x, i) =>
                          i === index ? { ...x, unitCost: e.target.value } : x,
                        ),
                      )
                    }
                  />
                </label>
              ) : l.trackingMode==="none" ? (
                <span className="muted">
                  Valuation follows the recorded average cost.
                </span>
              ):null}
              {l.trackingMode!=="none"&&<div className="content-stack">
                <p><strong>{l.trackingMode==="serial"?"Serial count":"Lot count"}</strong> · total {l.countedQuantity||"0"} {String(p?.unit??"")}</p>
                {l.allocations.map((a,allocationIndex)=><div className="business-form" key={a.lotId??`new-${allocationIndex}`}>
                  <label>{l.trackingMode==="serial"?"Serial number":"Lot code"}<input required readOnly={!!a.lotId} value={l.trackingMode==="serial"?a.serialNumber:a.lotCode} onChange={e=>setItems(items.map((x,i)=>i!==index?x:{...x,allocations:x.allocations.map((v,j)=>j!==allocationIndex?v:{...v,...(l.trackingMode==="serial"?{serialNumber:e.target.value}:{lotCode:e.target.value})})}))}/></label>
                  <label>Counted quantity<input type="number" min={a.lotId?"0":"0.001"} max={l.trackingMode==="serial"?"1":undefined} step={l.trackingMode==="serial"?"1":"0.001"} required value={a.countedQuantity} onChange={e=>setItems(items.map((x,i)=>{if(i!==index)return x;const allocations=x.allocations.map((v,j)=>j===allocationIndex?{...v,countedQuantity:e.target.value}:v);return {...x,allocations,countedQuantity:String(allocations.reduce((sum,v)=>sum+(Number(v.countedQuantity)||0),0))};}))}/></label>
                  {!a.lotId&&<><label>Manufactured date<input type="date" value={a.manufacturedDate} onChange={e=>setItems(items.map((x,i)=>i!==index?x:{...x,allocations:x.allocations.map((v,j)=>j===allocationIndex?{...v,manufacturedDate:e.target.value}:v)}))}/></label><label>Expiry date<input type="date" value={a.expiryDate} onChange={e=>setItems(items.map((x,i)=>i!==index?x:{...x,allocations:x.allocations.map((v,j)=>j===allocationIndex?{...v,expiryDate:e.target.value}:v)}))}/></label><label>Verified unit cost ({options.currency})<input type="number" min="0.01" step="0.01" required value={a.unitCost} onChange={e=>setItems(items.map((x,i)=>i!==index?x:{...x,allocations:x.allocations.map((v,j)=>j===allocationIndex?{...v,unitCost:e.target.value}:v)}))}/></label></>}
                  {!a.lotId&&<button type="button" className="button secondary" onClick={()=>setItems(items.map((x,i)=>{if(i!==index)return x;const allocations=x.allocations.filter((_,j)=>j!==allocationIndex);return {...x,allocations,countedQuantity:String(allocations.reduce((sum,v)=>sum+(Number(v.countedQuantity)||0),0))};}))}>Remove found {l.trackingMode}</button>}
                </div>)}
                <button type="button" className="button secondary" onClick={()=>setItems(items.map((x,i)=>i!==index?x:{...x,allocations:[...x.allocations,{lotCode:"",serialNumber:"",manufacturedDate:"",expiryDate:"",countedQuantity:l.trackingMode==="serial"?"1":"",unitCost:""}],countedQuantity:String(x.allocations.reduce((sum,v)=>sum+(Number(v.countedQuantity)||0),0)+(l.trackingMode==="serial"?1:0))}))}>Add found {l.trackingMode}</button>
              </div>}
              <button
                type="button"
                className="button secondary"
                onClick={() => setItems(items.filter((_, i) => i !== index))}
              >
                Remove product {index + 1}
              </button>
            </div>
          );
        })}
        <button
          className="button"
          disabled={pending || !items.length || !warehouse}
        >
          {pending ? "Saving…" : count ? "Save recount" : "Save physical count"}
        </button>
      </form>
    </details>
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
  return (
    <section className="card">
      <h2>Count workflow</h2>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {stale && count.status !== "posted" && (
        <p role="note" className="error">
          Stock changed after this count was saved. Return the count if
          submitted, then recount before approval.
        </p>
      )}
      {(editable || reviewable) && (
        <>
          <label>
            Decision reason
            <textarea
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              maxLength={2000}
              placeholder="Explain the approval, correction request, or cancellation"
            />
          </label>
          <div className="form-actions">
            {editable && (
              <>
                <button
                  className="button"
                  disabled={pending || stale}
                  onClick={() =>
                    void send(`/${count.id}/submit`, {
                      expectedVersion: count.version,
                    })
                  }
                >
                  Submit count for approval
                </button>
                <button
                  className="button secondary"
                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${count.id}/cancel`, {
                      expectedVersion: count.version,
                      reason,
                    })
                  }
                >
                  Cancel count
                </button>
              </>
            )}
            {reviewable && (
              <>
                <button
                  className="button"
                  disabled={pending || stale || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${count.id}/${count.status === "approved" ? "post" : "approve"}`, {
                      expectedVersion: count.version,
                      reason,
                    })
                  }
                >
                  {count.status === "approved" ? "Review Debit / Credit and post GL" : "Approve count for GL review"}
                </button>
                <button
                  className="button secondary"
                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${count.id}/reject`, {
                      expectedVersion: count.version,
                      reason,
                    })
                  }
                >
                  Return count for correction
                </button>
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
    </section>
  );
}
