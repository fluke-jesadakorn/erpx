"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { ecommerceDetail } from "@/server/ecommerce";
import { Card } from "@/components/ui/foundation";

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
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
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
    <Card as="section" appearance="standard" className="ui-panel ui-form-grid">
      <h2>Owner actions</h2>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
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
          <Field appearance="standard">
            Approval reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Approve order and reserve stock
          </Button>
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
          <Field appearance="standard">
            Sync evidence
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Sync Sales fulfillment
          </Button>
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
                  lines: [{ lineId: line.id, quantity: String(form.get("quantity")),condition:String(form.get("condition")),conditionNote:String(form.get("conditionNote")),qualityDueDate:String(form.get("qualityDueDate")),
                    ...(data.order.lotAllocations.some(allocation=>allocation.fulfillment_line_id===line.id)?{allocations:data.order.lotAllocations.filter(allocation=>allocation.fulfillment_line_id===line.id&&Number(form.get(`lot-${allocation.id}`))>0).map(allocation=>({lotId:String(allocation.lot_id),quantity:String(form.get(`lot-${allocation.id}`))}))}:{}),
                  }],
                });
              }}
            >
              <h3>Return delivery {String(fulfillment.number)}</h3>
              <Field appearance="standard">
                Date
                <TextInput appearance="standard" name="date" type="date" required />
              </Field>
              <Field appearance="standard">
                Quantity
                <TextInput appearance="standard" name="quantity" defaultValue="1" required />
              </Field>
              {data.order.lotAllocations.filter(allocation=>allocation.fulfillment_line_id===line.id&&Number(allocation.quantity_milli)>Number(allocation.returned_milli)).map(allocation=><Field appearance="standard" key={String(allocation.id)}>
                Return from {String(allocation.lot_code??allocation.serial_number)}
                <TextInput appearance="standard" name={`lot-${allocation.id}`} type="number" min="0" max={(Number(allocation.quantity_milli)-Number(allocation.returned_milli))/1000} step={allocation.serial_number?'1':'0.001'} defaultValue="0"/>
              </Field>)}
              <Field appearance="standard">Returned condition<Select appearance="standard" name="condition" defaultValue="unknown"><option value="unknown">Needs inspection</option><option value="unopened">Unopened</option><option value="damaged">Damaged</option></Select></Field>
              <Field appearance="standard">Condition evidence<TextArea appearance="standard" name="conditionNote" required minLength={3} maxLength={500}/></Field>
              <Field appearance="standard">Quality review due<TextInput appearance="standard" name="qualityDueDate" type="date" min={String(fulfillment.fulfillment_date)} required/></Field>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" required minLength={3} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Request return
              </Button>
            </form>
          )),
      )}
    </Card>
  );
}
