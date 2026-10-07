"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { PreferredDateTimeField } from "@/components/time/preferred-date-time-field";
import { minorToDecimal } from "@/lib/finance-money";
import { ServiceSpareForm } from "./service-spare-form";
import type { fieldServiceDetail, fieldServiceWorkspace } from "@/server/modules/field_service/public/field-service";
import { FormLayout, Card } from "@/components/ui/foundation";
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
    if (last.current?.body !== body) last.current = { key: crypto.randomUUID(), body };
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
        throw new Error(result.error?.message ?? "Unable to record field service action.");
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
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = Object.fromEntries(new FormData(e.currentTarget));
          void send({
            ...f,
            id: order!.id,
            action: name,
            expectedVersion: Number(order!.version),
            scheduledAt: f.scheduledAt ? new Date(String(f.scheduledAt)).getTime() : undefined,
            expiresAt: f.expiresAt ? new Date(String(f.expiresAt)).getTime() : undefined,
            quantityMilli: f.quantityMilli ? Number(f.quantityMilli) : undefined,
            laborMinutes: f.laborMinutes ? Number(f.laborMinutes) : undefined,
          });
        }}
      >
        <h2>{title}</h2>
        {extra}
        <Field appearance="standard">
          Reason and evidence
          <TextArea appearance="standard" name="reason" required minLength={3} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          {title}
        </Button>
      </FormLayout>
    );
  }
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {detail?.billing && (
        <Card as="section" appearance="standard" className="ui-panel" aria-label="Billing handoff">
          <h2>Billing handoff</h2>
          <p>
            Finance obligation: {String(detail.billing.currency)}{" "}
            {minorToDecimal(Number(detail.billing.amount_minor))} · {String(detail.billing.status)}
          </p>
          {Boolean(
            Number(detail.billing.warranty_applied) === 1 ||
            String(detail.billing.warranty_applied) === "true",
          ) ? (
            <p className="muted">
              Warranty pricing applied: {Number(detail.billing.warranty_discount_bps) / 100}%
              discount; coverage through {String(detail.billing.warranty_end_snapshot)} (calculated{" "}
              {String(detail.billing.pricing_date)}).
            </p>
          ) : (
            <p className="muted">
              Chargeable visit rate used; no active warranty discount was applied.
            </p>
          )}
        </Card>
      )}
      {data?.canWrite && (
        <>
          <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Draft service agreement</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              void send({
                ...f,
                entity: "agreement",
                departmentId: f.departmentId || null,
                assetId: f.assetId || null,
                visitRateMinor: Number(f.visitRateMinor),
                warrantyDiscountBps: Math.round(Number(f.warrantyDiscountPercent) * 100),
              });
            }}
          >
            <h2>Draft service agreement</h2>
            <Field appearance="standard">
              Branch
              <Select appearance="standard" name="branchId" required>
                {source.nodes
                  .filter((n) => ["branch", "headquarters"].includes(String(n.kind)))
                  .map((n) => (
                    <option key={String(n.id)} value={String(n.id)}>
                      {String(n.display_name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Department
              <Select appearance="standard" name="departmentId">
                <option value="">No department</option>
                {source.nodes
                  .filter((n) => n.kind === "department")
                  .map((n) => (
                    <option key={String(n.id)} value={String(n.id)}>
                      {String(n.display_name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Customer
              <Select appearance="standard" name="customerId" required>
                {source.customers.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Asset
              <Select appearance="standard" name="assetId">
                <option value="">Any covered asset</option>
                {source.assets.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.code)} · {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Title
              <TextInput appearance="standard" name="title" required />
            </Field>
            <Field appearance="standard">
              Start
              <TextInput appearance="standard" name="startDate" type="date" required />
            </Field>
            <Field appearance="standard">
              End
              <TextInput appearance="standard" name="endDate" type="date" required />
            </Field>
            <Field appearance="standard">
              Currency
              <TextInput appearance="standard" name="currency" defaultValue="THB" maxLength={3} required />
            </Field>
            <Field appearance="standard">
              Visit rate (minor units)
              <TextInput appearance="standard" name="visitRateMinor" type="number" min="0" required />
            </Field>
            <Field appearance="standard">
              Warranty discount (%)
              <TextInput appearance="standard"
                name="warrantyDiscountPercent"
                type="number"
                min="0"
                max="100"
                step="1"
                defaultValue="0"
                required
              />
              <small className="muted">
                0 = full rate; 100 = free while the linked asset is under warranty.
              </small>
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Draft agreement
            </Button>
          </FormLayout></FormDisclosure>
          <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Create service order</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget)),
                ticket = source.tickets.find((x) => x.id === f.ticketId),
                agreement = source.agreements.find((x) => x.id === f.agreementId);
              void send({
                ...f,
                ticketVersion: Number(ticket?.version),
                agreementVersion: Number(agreement?.version),
                assetId: f.assetId || null,
                scheduledAt: f.scheduledAt ? new Date(String(f.scheduledAt)).getTime() : null,
              });
            }}
          >
            <h2>Create service order</h2>
            <Field appearance="standard">
              Support case
              <Select appearance="standard" name="ticketId" required>
                {source.tickets.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.title)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Active agreement
              <Select appearance="standard" name="agreementId" required>
                {source.agreements
                  .filter((x) => x.status === "active")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.title)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Asset
              <Select appearance="standard" name="assetId">
                <option value="">No linked asset</option>
                {source.assets.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.code)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Issue
              <TextArea appearance="standard" name="issue" required minLength={3} />
            </Field>
            <Field appearance="standard">
              Site
              <TextInput appearance="standard" name="site" required />
            </Field>
            <Field appearance="standard">
              Technician
              <Select appearance="standard" name="technicianId" required>
                {source.people.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule" />
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create service order
            </Button>
          </FormLayout></FormDisclosure>
        </>
      )}
      {data?.agreements
        .filter((x) => x.status === "draft")
        .map((x) => (
          <FormLayout
            className="ui-panel ui-form-grid"
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
            <Field appearance="standard">
              Decision
              <Select appearance="standard" name="action">
                <option value="activate">Activate</option>
                <option value="reject">Reject</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !Number(x.can_review)}>
              Record decision
            </Button>
          </FormLayout>
        ))}
      {order &&
        ["open", "scheduled"].includes(String(order.status)) &&
        action(
          "schedule",
          "Schedule or reassign",
          <>
            <Field appearance="standard">
              Technician
              <Select appearance="standard" name="technicianId" defaultValue={String(order.technician_id)}>
                {source.people.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule" required />
          </>,
        )}
      {order &&
        ["open", "scheduled"].includes(String(order.status)) &&
        action(
          "reserve-part",
          "Reserve part",
          <>
            <Field appearance="standard">
              Warehouse id
              <TextInput appearance="standard" name="warehouseId" required />
            </Field>
            <Field appearance="standard">
              Product id
              <TextInput appearance="standard" name="productId" required />
            </Field>
            <Field appearance="standard">
              Quantity milli
              <TextInput appearance="standard" name="quantityMilli" type="number" min="1" required />
            </Field>
            <PreferredDateTimeField appearance="standard" name="expiresAt" label="Expires" required />
          </>,
        )}
      {order && order.status === "scheduled" && action("dispatch", "Dispatch technician")}
      {order && order.status === "dispatched" && action("start", "Start visit")}
      {order &&
        ["dispatched", "in_visit"].includes(String(order.status)) &&
        action("exception", "Record exception")}
      {order && order.status === "exception" && action("resume", "Resume visit")}
      {order && detail && order.status === "in_visit" && <ServiceSpareForm spares={detail.spares}
        reservations={detail.parts.filter(x => Number(x.quantity_milli)>Number(x.consumed_milli)+Number(x.released_milli)).map(x => ({ id:String(x.id),warehouseId:String(x.warehouse_id),productId:String(x.product_id),remainingMilli:Number(x.quantity_milli)-Number(x.consumed_milli)-Number(x.released_milli) }))}
        busy={busy} onSubmit={input => { void send({ ...input,id:order.id,action:"issue-part",expectedVersion:Number(order.version) }); }} />}
      {order &&
        order.status === "in_visit" &&
        action(
          "complete",
          "Complete visit",
          <>
            <Field appearance="standard">
              Labor minutes
              <TextInput appearance="standard" name="laborMinutes" type="number" min="0" required />
            </Field>
            <Field appearance="standard">
              Completion evidence
              <TextArea appearance="standard" name="evidence" required minLength={3} />
            </Field>
            <Field appearance="standard">
              Customer confirmation
              <TextArea appearance="standard" name="customerConfirmation" required minLength={3} />
            </Field>
          </>,
        )}
      {order && order.status === "completed" && action("release-billing", "Release billing")}
      {order &&
        !["completed", "billed", "cancelled"].includes(String(order.status)) &&
        action("cancel", "Cancel service order")}
    </>
  );
}
