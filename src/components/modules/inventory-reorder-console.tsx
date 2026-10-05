"use client";
import { RecordLink } from "@/components/records/record-link";
import { InlineNotice } from "@/components/ui/foundation";

import {
  Button,
  Field,
  Select,
  Table,
  TableFrame,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { StatusBadge } from "@/components/ui/primitives";
import { Card, FormLayout } from "@/components/ui/foundation";

type Policy = {
  id: string;
  warehouse_id: string;
  warehouse_name: string;
  branch_id: string;
  product_id: string;
  sku: string;
  product_name: string;
  unit: string;
  current_reorder_point_milli: string | null;
  proposed_reorder_point_milli: string | null;
  requested_by: string;
  approved_by: string | null;
  approved_at: number | null;
  retired_at: number | null;
  last_decision: "approved" | "rejected" | "retired" | null;
  last_review_reason: string | null;
  version: number;
};
type Option = { id: string; name: string; sku?: string; unit?: string; status: string };

function quantity(value: string | null) {
  if (!value || !/^\d+$/.test(value)) return "—";
  const milli = BigInt(value);
  return `${milli / BigInt(1000)}.${String(milli % BigInt(1000)).padStart(3, "0")}`;
}

export function InventoryReorderConsole({
  base,
  readerId,
  canPrepare,
  canApprove,
  policies,
  warehouses,
  products,
}: {
  base: string;
  readerId: string;
  canPrepare: boolean;
  canApprove: boolean;
  policies: Policy[];
  warehouses: Option[];
  products: Option[];
}) {
  const router = useRouter(),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  const [pending, setPending] = useState(false),
    [error, setError] = useState("");
  async function send(path: string, input: unknown) {
    const body = JSON.stringify(input),
      fingerprint = `${path}:${body}`;
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const response = await workFetch(
        `${base.replace("/holdings/", "/api/v1/holdings/")}/commerce/reorder-policies${path ? `/${path}` : ""}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body,
        },
      );
      const result = (await response.json()) as { error?: { message?: string } };
      if (!response.ok)
        throw new Error(result.error?.message ?? "Unable to save the reorder policy.");
      last.current = null;
      router.refresh();
      return true;
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Connection interrupted. Retry the same action.",
      );
      return false;
    } finally {
      setPending(false);
    }
  }
  const activeWarehouses = warehouses.filter((item) => item.status === "active"),
    activeProducts = products.filter((item) => item.status === "active");
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Approved reorder points</h2>
      <p className="muted">
        Shortage reporting compares available stock with an independently approved point for each
        product and warehouse. Proposed points do not affect the metric.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {canPrepare && (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget),
              warehouseId = String(form.get("warehouseId")),
              productId = String(form.get("productId"));
            const existing = policies.find(
              (policy) => policy.warehouse_id === warehouseId && policy.product_id === productId,
            );
            void send("", {
              warehouseId,
              productId,
              reorderPoint: form.get("reorderPoint"),
              reason: form.get("reason"),
              ...(existing ? { expectedVersion: existing.version } : {}),
            });
          }}
        >
          <Field appearance="standard">
            Warehouse
            <Select appearance="standard" name="warehouseId" required defaultValue="">
              <option value="" disabled>
                Select a warehouse
              </option>
              {activeWarehouses.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Product
            <Select appearance="standard" name="productId" required defaultValue="">
              <option value="" disabled>
                Select a product
              </option>
              {activeProducts.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.sku} · {item.name} ({item.unit})
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Reorder point
            <TextInput appearance="standard" name="reorderPoint" type="number" min="0.001" step="0.001" required />
          </Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard"
              name="reason"
              minLength={5}
              maxLength={2000}
              required
              placeholder="Describe the approved operational basis for this target."
            />
          </Field>
          <Button appearance="standard"
            clientManaged type="submit"
            variant="primary"
            disabled={pending || !activeWarehouses.length || !activeProducts.length}
          >
            Propose reorder point
          </Button>
        </FormLayout>
      )}
      <TableFrame appearance="standard" label="Records" className="business-table-wrap">
        <Table className="business-table">
          <thead>
            <tr>
              <th>Product</th>
              <th>Warehouse</th>
              <th>Approved point</th>
              <th>Proposed point</th>
              <th>State</th>
              <th>Review</th>
            </tr>
          </thead>
          <tbody>
            {policies.map((policy) => {
              const state =
                policy.proposed_reorder_point_milli !== null
                  ? "pending"
                  : policy.retired_at
                    ? "retired"
                    : policy.current_reorder_point_milli
                      ? "approved"
                      : "unconfigured";
              const canReviewProposal =
                canApprove &&
                policy.proposed_reorder_point_milli !== null &&
                policy.requested_by !== readerId;
              const canWithdraw =
                canPrepare &&
                policy.proposed_reorder_point_milli !== null &&
                policy.requested_by === readerId;
              const canRetire =
                canApprove &&
                policy.current_reorder_point_milli !== null &&
                !policy.retired_at &&
                policy.proposed_reorder_point_milli === null &&
                policy.approved_by !== readerId;
              return (
                <tr key={policy.id}>
                  <td>
                    <RecordLink href={`${base}/record-details/reorder_policy/${encodeURIComponent(policy.id)}`}>{policy.sku} · {policy.product_name}</RecordLink>
                  </td>
                  <td>{policy.warehouse_name}</td>
                  <td>
                    {quantity(policy.current_reorder_point_milli)}{" "}
                    {policy.current_reorder_point_milli ? policy.unit : ""}
                  </td>
                  <td>
                    {quantity(policy.proposed_reorder_point_milli)}{" "}
                    {policy.proposed_reorder_point_milli ? policy.unit : ""}
                  </td>
                  <td>
                    <StatusBadge appearance="standard" status={state} />
                    {policy.last_decision && <small>Last decision: {policy.last_decision}</small>}
                  </td>
                  <td>
                    {canReviewProposal && (
                      <FormLayout
                        className="ui-form-grid"
                        onSubmit={(event) => {
                          event.preventDefault();
                          const form = new FormData(event.currentTarget);
                          const button = (event.nativeEvent as SubmitEvent)
                            .submitter as HTMLButtonElement | null;
                          const action = button?.value === "reject" ? "reject" : "approve";
                          void send(`${policy.id}/${action}`, {
                            action,
                            expectedVersion: policy.version,
                            reason: form.get("reason"),
                          });
                        }}
                      >
                        <Field appearance="standard">
                          Review reason
                          <TextArea appearance="standard" name="reason" minLength={5} maxLength={2000} required />
                        </Field>
                        <div className="ui-form-actions">
                          <Button appearance="standard"
                            clientManaged variant="primary"
                            type="submit"
                            name="action"
                            value="approve"
                            disabled={pending}
                          >
                            Approve
                          </Button>
                          <Button appearance="standard"
                            clientManaged variant="secondary"
                            type="submit"
                            name="action"
                            value="reject"
                            disabled={pending}
                          >
                            Reject
                          </Button>
                        </div>
                      </FormLayout>
                    )}
                    {canWithdraw && (
                      <Button appearance="standard"
                        type="submit"
                        variant="secondary"
                        disabled={pending}
                        onClick={() =>
                          void send(`${policy.id}/withdraw`, {
                            action: "withdraw",
                            expectedVersion: policy.version,
                            reason: "Proposer withdrew the pending reorder point.",
                          })
                        }
                      >
                        Withdraw proposal
                      </Button>
                    )}
                    {canRetire && (
                      <FormLayout
                        className="ui-form-grid"
                        onSubmit={(event) => {
                          event.preventDefault();
                          const form = new FormData(event.currentTarget);
                          void send(`${policy.id}/retire`, {
                            action: "retire",
                            expectedVersion: policy.version,
                            reason: form.get("reason"),
                          });
                        }}
                      >
                        <Field appearance="standard">
                          Retirement reason
                          <TextArea appearance="standard" name="reason" minLength={5} maxLength={2000} required />
                        </Field>
                        <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending}>
                          Retire approved point
                        </Button>
                      </FormLayout>
                    )}
                    {!canReviewProposal &&
                      !canWithdraw &&
                      !canRetire &&
                      policy.last_review_reason && <small>{policy.last_review_reason}</small>}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </Table>
      </TableFrame>
      {!policies.length && (
        <p className="muted">No reorder points have been proposed for this scope.</p>
      )}
    </Card>
  );
}
