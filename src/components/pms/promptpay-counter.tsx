"use client";
import { Dialog } from "@/components/ui/dialog";
import { decimalToMinor } from "@/lib/finance/finance-money";
import { Button, Field, Select, TextInput, TextArea } from "@/components/ui/foundation";

import { useEffect, useRef, useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import QRCode from "qrcode";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import { formatUiDateTime } from "@/lib/shared/ui-format";

type Payment = {
  id: string;
  state: string;
  kind: string;
  amountMinor: number;
  qrData: string | null;
  expiresAt: number;
  entryId: string | null;
};
type History = {
  id: string;
  state: string;
  kind: string;
  amount_minor: number;
  refund_state: string | null;
  payer_email: string | null;
};
export function PmsPromptPay({
  base,
  propertyId,
  guestAccountId,
  initialId,
  dueMinor,
  history,
}: {
  base: string;
  propertyId: string;
  guestAccountId: string;
  initialId?: string;
  dueMinor: number;
  history: History[];
}) {
  const router = useRouter(),
    canvas = useRef<HTMLCanvasElement>(null),
    [payment, setPayment] = useState<Payment | null>(null),
    [id, setId] = useState(initialId ?? ""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [refundRow,setRefundRow]=useState<History|null>(null);
  const language = useFrontLanguage();
  useEffect(() => {
    if (!id) return;
    let active = true;
    const poll = async () => {
      try {
        const response = await workFetch(
          `/api/v1${base}/pms/stripe-payments?id=${encodeURIComponent(id)}`,
          { cache: "no-store" },
        );
        const body = await response.json();
        if (!response.ok) throw new Error(body.error?.message ?? "Payment status unavailable");
        if (active) {
          setPayment(body.data);
          if (body.data.state === "succeeded") router.refresh();
        }
      } catch (error) {
        if (active) setError(error instanceof Error ? error.message : "Payment status unavailable");
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 5000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [base, id, router]);
  useEffect(() => {
    if (payment?.qrData && canvas.current)
      void QRCode.toCanvas(canvas.current, payment.qrData, { width: 260, margin: 4 }).catch(
        () => {},
      );
  }, [payment?.qrData]);
  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/pms/stripe-payments`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({
          propertyId,
          guestAccountId,
          kind: String(data.get("kind")),
          amountMinor: Math.round(Number(data.get("amount")) * 100),
          payerEmail: String(data.get("email") ?? ""),
        }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Could not create PromptPay QR");
      setPayment(body.data);
      setId(body.data.id);
    } catch (error) {
      setError(error instanceof Error ? error.message : "Could not create PromptPay QR");
    } finally {
      setBusy(false);
    }
  }
  return (
    <FrontLocalizedTree>
      <section className="card">
        <h3>PromptPay · Company Stripe</h3>
        {error && <p role="alert">{error}</p>}
        {payment?.state === "pending" ? (
          <>
            <canvas ref={canvas} aria-label="Scan this PromptPay QR with a Thai banking app" />
            <p>
              {frontText("Waiting for Stripe confirmation. QR valid until", language)}{" "}
              {formatUiDateTime(payment.expiresAt, "Asia/Bangkok", {
                hour: "2-digit",
                minute: "2-digit",
              })}
              .
            </p>
          </>
        ) : payment?.state === "succeeded" ? (
          <p>Payment confirmed and posted to the guest account.</p>
        ) : payment?.state === "refund_review" ? (
          <p>Payment arrived after the QR expired. Finance must review and return the money.</p>
        ) : (
          <form onSubmit={create}>
            <Field>
              Purpose
              <Select name="kind">
                <option value="receipt">Room payment</option>
                <option value="deposit">Damage deposit</option>
              </Select>
            </Field>
            <Field>
              Amount (THB)
              <TextInput
                name="amount"
                type="number"
                min="0.01"
                step="0.01"
                defaultValue={(Math.max(dueMinor, 100) / 100).toFixed(2)}
                required
              />
            </Field>
            <Field>
              Payer email for Stripe refunds (optional)
              <TextInput name="email" type="email" />
            </Field>
            <Button clientManaged type="submit" variant="primary" disabled={busy}>
              Create PromptPay QR
            </Button>
          </form>
        )}
        {history
          .filter((row) => row.state === "succeeded" || row.state === "refund_review")
          .map((row) => (
            <div key={row.id}>
              <p>
                {row.kind === "deposit" ? "Deposit" : "Room payment"} · THB{" "}
                {(row.amount_minor / 100).toFixed(2)} ·{" "}
                {row.refund_state
                  ? `Refund ${row.refund_state}`
                  : row.state === "refund_review"
                    ? "Late payment under review"
                    : "Paid"}
              </p>
              {row.state === "succeeded" && (!row.refund_state || row.refund_state === "succeeded") && row.payer_email && (
                <Button
                  type="submit"
                  variant="primary"
                  disabled={busy}
                  onClick={() => setRefundRow(row)}
                >
                  Request Stripe refund
                </Button>
              )}
              {row.state === "succeeded" && !row.payer_email && (
                <p>
                  For a refund, use the independently approved cash or bank payment workflow and
                  attach payout evidence.
                </p>
              )}
            </div>
          ))}
      </section>
      <Dialog presentation="form-drawer" appearance="standard" open={Boolean(refundRow)} onClose={()=>setRefundRow(null)} title={frontText("Request Stripe refund",language)} dismissDisabled={busy}>
        {refundRow&&<form onSubmit={async e=>{e.preventDefault();const form=new FormData(e.currentTarget);setBusy(true);setError("");try{const amountMinor=decimalToMinor(String(form.get("amount")));if(amountMinor<=0||amountMinor>refundRow.amount_minor)throw new Error("Enter an amount within the original payment");const response=await workFetch(`/api/v1${base}/pms/stripe-refunds`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({action:"request",id:refundRow.id,amountMinor,reason:String(form.get("reason"))})});const body=await response.json();if(!response.ok)throw new Error(body.error?.message??"Refund request failed");setRefundRow(null);router.refresh();}catch(error){setError(error instanceof Error?error.message:"Refund request failed");}finally{setBusy(false);}}}>
          <p>Original payment: THB {(refundRow.amount_minor/100).toFixed(2)}</p>
          <Field appearance="standard">Refund amount (THB)<TextInput appearance="standard" name="amount" type="number" required min="0.01" step="0.01" max={refundRow.amount_minor/100} defaultValue={(refundRow.amount_minor/100).toFixed(2)}/></Field>
          <Field appearance="standard">Reason for refund<TextArea appearance="standard" name="reason" required minLength={3} maxLength={1000}/></Field>
          {error&&<p role="alert">{error}</p>}
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Request independent approval</Button>
        </form>}
      </Dialog>
    </FrontLocalizedTree>
  );
}
