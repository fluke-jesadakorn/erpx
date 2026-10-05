"use client";
import {RecordLink} from "@/components/records/record-link";
import {recordCanonicalHref} from "@/lib/record-drawer";
import { Button, Table, TableFrame } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { minorToDecimal } from "@/lib/finance-money";
import {BookingRefundRecovery} from "./booking-refund-recovery";

type Refund = {
  id: string;
  amount_minor: number;
  status: string;
  refund_state: string;
  last_error: string | null;
  created_at: number;
  needs_receipt_recovery?: boolean;
};
export function BookingRefunds({
  base,
  rows,
  canApprove,
}: {
  base: string;
  rows: Refund[];
  canApprove: boolean;
}) {
  const router = useRouter(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState("");
  if (!rows.length) return <p>No online booking refunds require review.</p>;
  return (
    <section className="card">
      <h2>Online booking refunds</h2>
      {error && <p role="alert">{error}</p>}
      <TableFrame label="Records" className="business-table-wrap">
        <Table className="business-table">
          <thead>
            <tr>
              <th>Booking payment</th>
              <th>Amount</th>
              <th>Status</th>
              <th>Provider review</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id}>
                <td><RecordLink href={recordCanonicalHref(base,{kind:"pms_booking_refund",id:row.id})}>{row.id.slice(0, 8)}</RecordLink></td>
                <td>THB {minorToDecimal(Number(row.amount_minor))}</td>
                <td>
                  {row.refund_state}
                  {row.last_error && <p>{row.last_error}</p>}
                </td>
                <td>
                  {canApprove && (row.needs_receipt_recovery || row.refund_state === 'failed') ? <BookingRefundRecovery base={base} id={row.id} receipt={row.needs_receipt_recovery} onSaved={()=>router.refresh()}/> : canApprove && row.refund_state === "requested" ? (
                    <Button
                      type="submit"
                      variant="primary"
                      disabled={Boolean(busy)}
                      onClick={async () => {
                        setBusy(row.id);
                        setError("");
                        try {
                          const response = await workFetch(`/api/v1${base}/pms/booking-refunds`, {
                            method: "POST",
                            headers: { "Content-Type": "application/json" },
                            body: JSON.stringify({ id: row.id }),
                          });
                          const body = await response.json();
                          if (!response.ok)
                            throw new Error(body.error?.message ?? "Approval failed");
                          router.refresh();
                        } catch (error) {
                          setError(error instanceof Error ? error.message : "Approval failed");
                        } finally {
                          setBusy("");
                        }
                      }}
                    >
                      Approve Stripe refund
                    </Button>
                  ) : row.refund_state === "requires_action" ? (
                    "Waiting for guest bank details"
                  ) : row.refund_state === "processing" ? (
                    "Waiting for Stripe"
                  ) : row.refund_state === "succeeded" ? (
                    "Confirmed by Stripe"
                  ) : (
                    "Finance review"
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
    </section>
  );
}
