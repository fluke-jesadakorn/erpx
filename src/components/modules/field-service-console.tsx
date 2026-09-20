"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type {
  fieldServiceDetail,
  fieldServiceWorkspace,
} from "@/server/field-service";
function useCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { key: crypto.randomUUID(), body };
    try {
      const response = await workFetch(`/api/v1${base}/field-service`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body,
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(
          result.error?.message ?? "Unable to record field service action.",
        );
      }
      last.current = null;
      router.push(
        `${base}/field-service${input && typeof input === "object" && "entity" in input && input.entity === "agreement" ? "" : `/${result.data.id}`}`,
      );
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  return { send, busy, error };
}
export function FieldServiceConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof fieldServiceWorkspace>>;
  detail?: Awaited<ReturnType<typeof fieldServiceDetail>>;
}) {
  const source = data ?? detail!,
    { send, busy, error } = useCommand(base),
    order = detail?.order;
  function action(name: string, title: string, extra?: React.ReactNode) {
    return (
      <form
        className="card business-form"
        onSubmit={(e) => {
          e.preventDefault();
          const f = Object.fromEntries(new FormData(e.currentTarget));
          void send({
            ...f,
            id: order!.id,
            action: name,
            expectedVersion: Number(order!.version),
            scheduledAt: f.scheduledAt
              ? new Date(String(f.scheduledAt)).getTime()
              : undefined,
            expiresAt: f.expiresAt
              ? new Date(String(f.expiresAt)).getTime()
              : undefined,
            quantityMilli: f.quantityMilli
              ? Number(f.quantityMilli)
              : undefined,
            laborMinutes: f.laborMinutes ? Number(f.laborMinutes) : undefined,
          });
        }}
      >
        <h2>{title}</h2>
        {extra}
        <label>
          Reason and evidence
          <textarea name="reason" required minLength={3} />
        </label>
        <button className="button" disabled={busy}>
          {title}
        </button>
      </form>
    );
  }
  return (
    <>
      {error && <p role="alert">{error}</p>}
      {data?.canWrite && (
        <>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              void send({
                ...f,
                entity: "agreement",
                departmentId: f.departmentId || null,
                assetId: f.assetId || null,
                visitRateMinor: Number(f.visitRateMinor),
              });
            }}
          >
            <h2>Draft service agreement</h2>
            <label>
              Branch
              <select name="branchId" required>
                {source.nodes
                  .filter((n) =>
                    ["branch", "headquarters"].includes(String(n.kind)),
                  )
                  .map((n) => (
                    <option key={String(n.id)} value={String(n.id)}>
                      {String(n.display_name)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Department
              <select name="departmentId">
                <option value="">No department</option>
                {source.nodes
                  .filter((n) => n.kind === "department")
                  .map((n) => (
                    <option key={String(n.id)} value={String(n.id)}>
                      {String(n.display_name)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Customer
              <select name="customerId" required>
                {source.customers.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Asset
              <select name="assetId">
                <option value="">Any covered asset</option>
                {source.assets.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.code)} · {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Title
              <input name="title" required />
            </label>
            <label>
              Start
              <input name="startDate" type="date" required />
            </label>
            <label>
              End
              <input name="endDate" type="date" required />
            </label>
            <label>
              Currency
              <input
                name="currency"
                defaultValue="THB"
                maxLength={3}
                required
              />
            </label>
            <label>
              Visit rate (minor units)
              <input name="visitRateMinor" type="number" min="0" required />
            </label>
            <button className="button" disabled={busy}>
              Draft agreement
            </button>
          </form>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget)),
                ticket = source.tickets.find((x) => x.id === f.ticketId),
                agreement = source.agreements.find(
                  (x) => x.id === f.agreementId,
                );
              void send({
                ...f,
                ticketVersion: Number(ticket?.version),
                agreementVersion: Number(agreement?.version),
                assetId: f.assetId || null,
                scheduledAt: f.scheduledAt
                  ? new Date(String(f.scheduledAt)).getTime()
                  : null,
              });
            }}
          >
            <h2>Create service order</h2>
            <label>
              Support case
              <select name="ticketId" required>
                {source.tickets.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.title)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Active agreement
              <select name="agreementId" required>
                {source.agreements
                  .filter((x) => x.status === "active")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.title)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Asset
              <select name="assetId">
                <option value="">No linked asset</option>
                {source.assets.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.code)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Issue
              <textarea name="issue" required minLength={3} />
            </label>
            <label>
              Site
              <input name="site" required />
            </label>
            <label>
              Technician
              <select name="technicianId" required>
                {source.people.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Schedule
              <input name="scheduledAt" type="datetime-local" />
            </label>
            <button className="button" disabled={busy}>
              Create service order
            </button>
          </form>
        </>
      )}
      {data?.agreements
        .filter((x) => x.status === "draft")
        .map((x) => (
          <form
            className="card business-form"
            key={String(x.id)}
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              void send({
                ...f,
                entity: "agreement",
                id: x.id,
                expectedVersion: Number(x.version),
              });
            }}
          >
            <h2>Agreement review · {String(x.title)}</h2>
            <label>
              Decision
              <select name="action">
                <option value="activate">Activate</option>
                <option value="reject">Reject</option>
              </select>
            </label>
            <label>
              Reason
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy || !Number(x.can_review)}>
              Record decision
            </button>
          </form>
        ))}
      {order &&
        ["open", "scheduled"].includes(String(order.status)) &&
        action(
          "schedule",
          "Schedule or reassign",
          <>
            <label>
              Technician
              <select
                name="technicianId"
                defaultValue={String(order.technician_id)}
              >
                {source.people.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Schedule
              <input name="scheduledAt" type="datetime-local" required />
            </label>
          </>,
        )}
      {order &&
        ["open", "scheduled"].includes(String(order.status)) &&
        action(
          "reserve-part",
          "Reserve part",
          <>
            <label>
              Warehouse id
              <input name="warehouseId" required />
            </label>
            <label>
              Product id
              <input name="productId" required />
            </label>
            <label>
              Quantity milli
              <input name="quantityMilli" type="number" min="1" required />
            </label>
            <label>
              Expires
              <input name="expiresAt" type="datetime-local" required />
            </label>
          </>,
        )}
      {order &&
        order.status === "scheduled" &&
        action("dispatch", "Dispatch technician")}
      {order && order.status === "dispatched" && action("start", "Start visit")}
      {order &&
        ["dispatched", "in_visit"].includes(String(order.status)) &&
        action("exception", "Record exception")}
      {order &&
        order.status === "exception" &&
        action("resume", "Resume visit")}
      {order &&
        order.status === "in_visit" &&
        detail?.parts.some(
          (x) =>
            Number(x.quantity_milli) >
            Number(x.consumed_milli) + Number(x.released_milli),
        ) &&
        action(
          "issue-part",
          "Issue reserved part",
          <>
            <label>
              Reservation
              <select name="reservationId">
                {detail.parts
                  .filter(
                    (x) =>
                      Number(x.quantity_milli) >
                      Number(x.consumed_milli) + Number(x.released_milli),
                  )
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.sku)} · {String(x.quantity_milli)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Quantity milli
              <input name="quantityMilli" type="number" min="1" required />
            </label>
          </>,
        )}
      {order &&
        order.status === "in_visit" &&
        action(
          "complete",
          "Complete visit",
          <>
            <label>
              Labor minutes
              <input name="laborMinutes" type="number" min="0" required />
            </label>
            <label>
              Completion evidence
              <textarea name="evidence" required minLength={3} />
            </label>
            <label>
              Customer confirmation
              <textarea name="customerConfirmation" required minLength={3} />
            </label>
          </>,
        )}
      {order &&
        order.status === "completed" &&
        action("release-billing", "Release billing")}
      {order &&
        !["completed", "billed", "cancelled"].includes(String(order.status)) &&
        action("cancel", "Cancel service order")}
    </>
  );
}
