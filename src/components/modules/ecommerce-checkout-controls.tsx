"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { ecommerceDetail } from "@/server/ecommerce";

export function EcommerceCheckoutControls({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof ecommerceDetail>>;
}) {
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/ecommerce`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": last.current.key,
        },
        body,
      });
      const result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to update checkout.");
      }
      last.current = null;
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const checkout = data.checkout;
  return (
    <section className="card business-form">
      <h2>Owner actions</h2>
      {error && <p role="alert">{error}</p>}
      {checkout.status === "submitted" && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void send({
              entity: "checkout",
              id: checkout.id,
              action: "approve-order",
              expectedVersion: Number(checkout.version),
              reason: String(new FormData(event.currentTarget).get("reason")),
            });
          }}
        >
          <label>
            Approval reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Approve order and reserve stock
          </button>
        </form>
      )}
      {checkout.status !== "submitted" && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void send({
              entity: "checkout",
              id: checkout.id,
              action: "sync-fulfillment",
              expectedVersion: Number(checkout.version),
              reason: String(new FormData(event.currentTarget).get("reason")),
            });
          }}
        >
          <label>
            Sync evidence
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Sync Sales fulfillment
          </button>
        </form>
      )}
      {data.order.fulfillments.flatMap((fulfillment) =>
        data.order.fulfillmentLines
          .filter((line) => line.fulfillment_id === fulfillment.id)
          .map((line) => (
            <form
              key={String(line.id)}
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                void send({
                  entity: "checkout",
                  id: checkout.id,
                  action: "request-return",
                  expectedVersion: Number(checkout.version),
                  reason: String(form.get("reason")),
                  fulfillmentId: fulfillment.id,
                  fulfillmentVersion: Number(fulfillment.version),
                  date: String(form.get("date")),
                  lines: [
                    { lineId: line.id, quantity: String(form.get("quantity")) },
                  ],
                });
              }}
            >
              <h3>Return delivery {String(fulfillment.number)}</h3>
              <label>
                Date
                <input name="date" type="date" required />
              </label>
              <label>
                Quantity
                <input name="quantity" defaultValue="1" required />
              </label>
              <label>
                Reason
                <textarea name="reason" required minLength={3} />
              </label>
              <button className="button" disabled={busy}>
                Request return
              </button>
            </form>
          )),
      )}
    </section>
  );
}
