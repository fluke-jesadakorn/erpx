"use client";
import { Button, Table, TableFrame } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";

type Refund = {
  id: string;
  property_id: string;
  folio_id: string;
  amount_minor: number;
  refund_amount_minor: number | null;
  refund_state: string;
  last_error: string | null;
  refund_reason: string | null;
  payer_email: string | null;
};
export function CounterRefunds({
  base,
  rows,
  canApprove,
}: {
  base: string;
  rows: Refund[];
  canApprove: boolean;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(""),
    [error, setError] = useState("");
  return (
    <section className="card">
      <h2>Counter PromptPay refunds</h2>
      {error && <p role="alert">{error}</p>}
      <TableFrame label="Records" className="business-table-wrap">
        <Table className="business-table">
          <thead>
            <tr>
              <th>Payment</th>
              <th>Amount</th>
              <th>Reason</th>
              <th>Status</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id}>
                <td>{row.id.slice(0, 8)}</td>
                <td>THB {((row.refund_amount_minor ?? row.amount_minor) / 100).toFixed(2)}</td>
                <td>{row.refund_reason ?? "Late payment review"}</td>
                <td>
                  {row.refund_state}
                  {row.last_error && <p>{row.last_error}</p>}
                </td>
                <td>
                  {canApprove && row.refund_state === "requested" ? (
                    <Button
                      type="submit"
                      variant="primary"
                      disabled={Boolean(busy)}
                      onClick={async () => {
                        setBusy(row.id);
                        setError("");
                        try {
                          const response = await workFetch(`/api/v1${base}/pms/stripe-refunds`, {
                            method: "POST",
                            headers: { "Content-Type": "application/json" },
                            body: JSON.stringify({ action: "approve", id: row.id }),
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
      {!rows.length && <p>No counter refunds are waiting or recorded.</p>}
    </section>
  );
}
