"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { hubResultHref } from "@erpx/feature-shared/lib/hub-result";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type {
  customerIdentityDetail,
  customerIdentityWorkspace,
} from "@erpx/server-customer_authority/public/customer-identity-changes";
import { Card, FormLayout } from "@erpx/ui/components/foundation";
export function CustomerIdentityConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof customerIdentityWorkspace>>;
  detail?: Awaited<ReturnType<typeof customerIdentityDetail>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ body: string; key: string } | null>(null);
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/customer-identity`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body,
        }),
        result = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save identity change.");
      }
      last.current = null;
      window.dispatchEvent(new Event("erpx:hub-saved"));
      window.dispatchEvent(new Event("erpx:record-saved"));
      router.push(hubResultHref(window.location.href, base, canonicalUiHref(`${base}/customer-identity/${result.data.id}`)) ?? canonicalUiHref(`${base}/customer-identity/${result.data.id}`), { scroll: false });
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this command.");
    } finally {
      setBusy(false);
    }
  }
  const fields = (current?: Record<string, unknown>) => {
      const address =
        current?.address && typeof current.address === "object"
          ? (current.address as Record<string, unknown>)
          : {};
      return (
        <>
          <Field appearance="standard">
            Legal customer name
            <TextInput appearance="standard" name="name" defaultValue={String(current?.name ?? "")} required />
          </Field>
          <Field appearance="standard">
            Tax registration
            <TextInput appearance="standard" name="tax" defaultValue={String(current?.taxRegistration ?? "")} />
          </Field>
          <Field appearance="standard">
            Phone
            <TextInput appearance="standard" name="phone" defaultValue={String(current?.phone ?? "")} />
          </Field>
          <Field appearance="standard">
            Address line 1
            <TextInput appearance="standard" name="addressLine1" defaultValue={String(address.line1 ?? "")} />
          </Field>
          <Field appearance="standard">
            Address line 2
            <TextInput appearance="standard" name="addressLine2" defaultValue={String(address.line2 ?? "")} />
          </Field>
          <Field appearance="standard">
            City
            <TextInput appearance="standard" name="addressCity" defaultValue={String(address.city ?? "")} />
          </Field>
          <Field appearance="standard">
            Region
            <TextInput appearance="standard" name="addressRegion" defaultValue={String(address.region ?? "")} />
          </Field>
          <Field appearance="standard">
            Postal code
            <TextInput appearance="standard" name="addressPostalCode" defaultValue={String(address.postalCode ?? "")} />
          </Field>
          <Field appearance="standard">
            Country code
            <TextInput appearance="standard"
              name="addressCountryCode"
              maxLength={2}
              defaultValue={String(address.countryCode ?? "")}
            />
          </Field>
          <Field appearance="standard">
            Notification email
            <TextInput appearance="standard"
              type="email"
              name="notificationEmail"
              defaultValue={String(current?.notificationEmail ?? "")}
            />
          </Field>
          <Field appearance="standard">
            Consent
            <Select appearance="standard"
              name="notificationConsent"
              defaultValue={String(current?.notificationConsent ?? "unknown")}
            >
              <option value="unknown">Unknown</option>
              <option value="granted">Granted</option>
              <option value="revoked">Revoked</option>
            </Select>
          </Field>
        </>
      );
    },
    proposed = (f: FormData) => {
      const line1 = String(f.get("addressLine1") || "").trim();
      return {
        name: f.get("name"),
        taxRegistration: String(f.get("tax") || "") || null,
        phone: String(f.get("phone") || "") || null,
        address: line1
          ? {
              line1,
              line2: String(f.get("addressLine2") || "") || null,
              city: String(f.get("addressCity") || ""),
              region: String(f.get("addressRegion") || "") || null,
              postalCode: String(f.get("addressPostalCode") || ""),
              countryCode: String(f.get("addressCountryCode") || "").toUpperCase(),
            }
          : null,
        notificationEmail: String(f.get("notificationEmail") || "") || null,
        notificationConsent: f.get("notificationConsent"),
      };
    },
    evidence = (
      <Field appearance="standard">
        Evidence and reason
        <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
      </Field>
    );
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canCreate && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Propose identity change</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                customer = data.customers.find((c) => c.id === f.get("customer"));
              if (customer)
                void send({
                  customerId: customer.id,
                  expectedCustomerVersion: Number(customer.version),
                  proposed: proposed(f),
                  evidence: f.get("reason"),
                });
            }}
          >
            <Field appearance="standard">
              Customer
              <Select appearance="standard" name="customer" required>
                <option value="">Choose customer</option>
                {data.customers.map((c) => (
                  <option key={String(c.id)} value={String(c.id)}>
                    {String(c.name)} · {String(c.code)}
                  </option>
                ))}
              </Select>
            </Field>
            {fields()}
            {evidence}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Send for independent review
            </Button>
          </FormLayout>
        </Card>
      )}
      {detail && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Review identity snapshot</h2>
          <pre>
            {JSON.stringify(
              { before: detail.row.previous_json, proposed: detail.row.proposed_json },
              null,
              2,
            )}
          </pre>
          {Boolean(detail.row.canReview) && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({
                  id: detail.row.id,
                  action: f.get("action"),
                  expectedVersion: Number(detail.row.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="action">
                  <option value="apply">Apply proposed identity</option>
                  <option value="reject">Reject for correction</option>
                </Select>
              </Field>
              {evidence}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Record independent decision
              </Button>
            </FormLayout>
          )}
          {Boolean(detail.row.canResubmit) && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({
                  id: detail.row.id,
                  action: "resubmit",
                  expectedVersion: Number(detail.row.version),
                  expectedCustomerVersion: Number(detail.row.current_customer_version),
                  proposed: proposed(f),
                  reason: f.get("reason"),
                });
              }}
            >
              {fields(
                typeof detail.row.proposed_json === "string"
                  ? JSON.parse(detail.row.proposed_json)
                  : (detail.row.proposed_json as Record<string, unknown>),
              )}
              {evidence}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Resubmit identity change
              </Button>
            </FormLayout>
          )}
        </Card>
      )}
    </>
  );
}
