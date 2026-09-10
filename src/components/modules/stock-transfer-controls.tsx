"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import type { TransferRow as Row } from "@/server/inventory-transfers";
import { minorToDecimal } from "@/lib/finance-money";
type Options = { warehouses: Row[]; sources: Row[]; stock: Row[] };
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
      const r = await fetch(`/api/v1${base}/stock-transfers${path}`, {
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
      if (!r.ok)
        throw new Error(data.error?.message ?? "Unable to save transfer.");
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
  initialOpen = false,
}: {
  base: string;
  options: Options;
  transfer?: Row;
  lines?: Row[];
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
    })),
  );
  const available = options.stock.filter(
    (s) =>
      s.warehouse_id === source &&
      !items.some((l) => l.productId === s.product_id),
  );
  return (
    <details className="card" open={!!transfer || initialOpen}>
      <summary>
        {transfer ? "Edit transfer quantities" : "New warehouse transfer"}
      </summary>
      <p className="muted">
        Record a completed direct movement between company warehouses. The
        receiving reviewer confirms the quantities before stock changes. Source
        cost is preserved.
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
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
              reason: f.get("reason"),
              lines: items.map((l) => ({
                productId: l.productId,
                quantity: l.quantity,
              })),
              ...(transfer ? { expectedVersion: transfer.version } : {}),
            },
            !transfer,
          );
        }}
      >
        <div className="business-form">
          <label>
            Source warehouse
            <select
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
            </select>
          </label>
          <label>
            Receiving warehouse
            <select
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
            </select>
          </label>
          <label>
            Transfer receipt date
            <input
              type="date"
              name="transferDate"
              required
              defaultValue={String(
                transfer?.transfer_date ??
                  new Date().toISOString().slice(0, 10),
              )}
            />
          </label>
          <label>
            Transfer purpose
            <input
              name="reason"
              required
              minLength={5}
              maxLength={2000}
              defaultValue={String(transfer?.reason ?? "")}
              placeholder="Replenishment or installation staging"
            />
          </label>
        </div>
        <div className="business-form">
          <label>
            Product to transfer
            <select
              value={product}
              onChange={(e) => setProduct(e.target.value)}
            >
              <option value="">Select available stock</option>
              {available.map((s) => (
                <option key={String(s.product_id)} value={String(s.product_id)}>
                  {String(s.sku)} · {String(s.name)} ·{" "}
                  {Number(s.quantity_milli) / 1000} {String(s.unit)} available
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="button secondary"
            disabled={
              !available.some((s) => s.product_id === product) ||
              items.length >= 20
            }
            onClick={() => {
              setItems([
                ...items,
                {
                  productId: product,
                  quantity: "",
                  description: String(
                    available.find((s) => s.product_id === product)?.name ?? "",
                  ),
                },
              ]);
              setProduct("");
            }}
          >
            Add transfer product
          </button>
        </div>
        {items.map((l, index) => {
          const s = options.stock.find(
            (s) => s.warehouse_id === source && s.product_id === l.productId,
          );
          return (
            <div className="business-form" key={l.productId}>
              <div>
                <strong>{l.description}</strong>
                <p className="muted">
                  Available: {Number(s?.quantity_milli ?? 0) / 1000}{" "}
                  {String(s?.unit ?? "")} · recorded value{" "}
                  {minorToDecimal(Number(s?.value_minor ?? 0))}
                </p>
              </div>
              <label>
                Transfer quantity {index + 1}
                <input
                  type="number"
                  min="0.001"
                  step="0.001"
                  required
                  value={l.quantity}
                  onChange={(e) =>
                    setItems(
                      items.map((x, i) =>
                        i === index ? { ...x, quantity: e.target.value } : x,
                      ),
                    )
                  }
                />
              </label>
              <button
                type="button"
                className="button secondary"
                onClick={() => setItems(items.filter((_, i) => i !== index))}
              >
                Remove transfer product {index + 1}
              </button>
            </div>
          );
        })}
        <button
          className="button"
          disabled={pending || !items.length || !source || !target}
        >
          {pending
            ? "Saving…"
            : transfer
              ? "Save transfer changes"
              : "Save transfer draft"}
        </button>
      </form>
    </details>
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
    [reason, setReason] = useState("");
  return (
    <section className="card">
      <h2>Transfer workflow</h2>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {stale && !["posted", "cancelled"].includes(String(transfer.status)) && (
        <p role="note" className="error">
          Source stock changed. Return a submitted transfer and refresh its
          quantities before posting.
        </p>
      )}
      {(editable || reviewable) && (
        <>
          <label>
            Receipt or decision reason
            <textarea
              maxLength={2000}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Confirm the receiving count or explain the correction"
            />
          </label>
          <div className="form-actions">
            {editable && (
              <>
                <button
                  className="button"
                  disabled={pending || stale}
                  onClick={() =>
                    void send(`/${transfer.id}/submit`, {
                      expectedVersion: transfer.version,
                    })
                  }
                >
                  Submit transfer for receipt review
                </button>
                <button
                  className="button secondary"
                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${transfer.id}/cancel`, {
                      expectedVersion: transfer.version,
                      reason,
                    })
                  }
                >
                  Cancel transfer
                </button>
              </>
            )}
            {reviewable && (
              <>
                <button
                  className="button"
                  disabled={pending || stale || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${transfer.id}/post`, {
                      expectedVersion: transfer.version,
                      reason,
                    })
                  }
                >
                  Confirm receipt and post transfer
                </button>
                <button
                  className="button secondary"
                  disabled={pending || reason.trim().length < 3}
                  onClick={() =>
                    void send(`/${transfer.id}/reject`, {
                      expectedVersion: transfer.version,
                      reason,
                    })
                  }
                >
                  Return transfer for correction
                </button>
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
              ? "The source and receiving stock movements are posted. Use a new transfer to move stock back."
              : transfer.status === "cancelled"
                ? "This transfer was cancelled; its history is retained."
                : "Only the source preparer can edit this transfer."}
        </p>
      )}
    </section>
  );
}
