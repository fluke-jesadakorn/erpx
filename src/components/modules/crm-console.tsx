"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { crmDetail, crmWorkspace } from "@/server/crm";
import type { crmIdentities } from "@/server/crm-identities";
import { Card, FormLayout } from "@/components/ui/foundation";
export function CRMConsole({
  base,
  data,
  identityData,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof crmWorkspace>>;
  identityData?: Awaited<ReturnType<typeof crmIdentities>>;
  detail?: Awaited<ReturnType<typeof crmDetail>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [selectedAccount, setSelectedAccount] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null),
    r = detail?.row;
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/crm`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: JSON.stringify(input),
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save CRM action.");
      }
      last.current = null;
      if (!(input && typeof input === "object" && "domain" in input && input.domain === "identity"))
        router.push(`${base}/crm/${result.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the same command.");
    } finally {
      setBusy(false);
    }
  }
  const reason = (
      <Field appearance="standard">
        Evidence and reason
        <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
      </Field>
    ),
    button = (label: string) => (
      <Button appearance="standard" type="submit" variant="primary" disabled={busy}>
        {label}
      </Button>
    ),
    lead = (f: FormData, action: string) => ({
      id: r!.id,
      action,
      expectedVersion: Number(r!.version),
      reason: f.get("reason"),
    }),
    deal = (f: FormData, action: string) => ({
      ...lead(f, action),
      domain: "opportunity",
      expectedVersion: Number(r!.opportunity_version),
      leadVersion: Number(r!.version),
    });
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canCreate && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Capture lead</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                branchId: f.get("branch"),
                accountId: f.get("accountId") || undefined,
                contactId: f.get("contactId") || undefined,
                title: f.get("title"),
                contactName: f.get("contact"),
                contactDetail: f.get("contactDetail"),
                source: f.get("source"),
                sourceEvidence: f.get("sourceEvidence"),
                campaignId: f.get("campaignId") || undefined,
                brief: f.get("brief"),
                dueDate: f.get("due"),
              });
            }}
          >
            <Field appearance="standard">
              Branch
              <Select appearance="standard" name="branch" required>
                <option value="">Choose branch</option>
                {data.branches.map((b) => (
                  <option key={String(b.id)} value={String(b.id)}>
                    {String(b.display_name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Existing Account (optional)
              <Select appearance="standard"
                name="accountId"
                value={selectedAccount}
                onChange={(e) => setSelectedAccount(e.target.value)}
              >
                <option value="">New individual prospect</option>
                {identityData?.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
            {selectedAccount && (
              <Field appearance="standard">
                Existing Contact (optional)
                <Select appearance="standard" name="contactId">
                  <option value="">Create contact in this Account</option>
                  {identityData?.contacts
                    .filter((c) => c.account_id === selectedAccount)
                    .map((c) => (
                      <option key={String(c.id)} value={String(c.id)}>
                        {String(c.name)}
                      </option>
                    ))}
                </Select>
              </Field>
            )}
            <Field appearance="standard">
              Inquiry title
              <TextInput appearance="standard" name="title" minLength={3} maxLength={160} required />
            </Field>
            <Field appearance="standard">
              Contact name
              <TextInput appearance="standard" name="contact" minLength={2} maxLength={160} required />
            </Field>
            <Field appearance="standard">
              Contact details and requested channel
              <TextArea appearance="standard" name="contactDetail" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Inquiry source
              <TextInput appearance="standard" name="source" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Source evidence
              <TextArea appearance="standard" name="sourceEvidence" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Campaign ID
              <Select appearance="standard" name="campaignId">
                <option value="">No linked campaign</option>
                {data.campaigns.map((c) => (
                  <option key={String(c.id)} value={String(c.id)}>
                    {String(c.code)} · {String(c.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Customer needs and conditions
              <TextArea appearance="standard" name="brief" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Follow-up due date
              <TextInput appearance="standard" name="due" type="date" required />
            </Field>
            {button("Create lead assigned to me")}
          </FormLayout>
        </Card>
      )}
      {r && detail?.canAct && r.status === "open" && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Qualify customer needs</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                ...lead(f, "qualify"),
                customerId: f.get("customer"),
                expectedValue: f.get("value"),
                closeDate: f.get("close"),
              });
            }}
          >
            <p>
              {r.account_customer_id
                ? "Finance customer link has been reviewed."
                : "A separate reviewer must link this Account to a Finance customer first."}
            </p>
            <Field appearance="standard">
              Reviewed customer identity
              <Select appearance="standard" name="customer" required>
                <option value="">Select the existing customer</option>
                {detail.customers
                  .filter((c) => c.id === r.account_customer_id)
                  .map((c) => (
                    <option key={String(c.id)} value={String(c.id)}>
                      {String(c.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Expected net value ({detail.companyCurrency})
              <TextInput appearance="standard" name="value" type="number" min="0" step="0.01" required />
            </Field>
            <Field appearance="standard">
              Expected close date
              <TextInput appearance="standard" name="close" type="date" required />
            </Field>
            {reason}
            {button("Qualify and create opportunity")}
          </FormLayout>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              void send(lead(new FormData(e.currentTarget), "disqualify"));
            }}
          >
            {reason}
            {button("Disqualify lead")}
          </FormLayout>
        </Card>
      )}
      {r && detail?.canManage && r.stage !== "won" && r.status !== "disqualified" && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Change responsibility</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({ ...lead(f, "reassign"), ownerId: f.get("owner"), dueDate: f.get("due") });
            }}
          >
            <Field appearance="standard">
              Eligible salesperson
              <Select appearance="standard" name="owner" required defaultValue={String(r.owner_id)}>
                <option value="">Choose colleague</option>
                {detail.recipients.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Follow-up due date
              <TextInput appearance="standard" name="due" type="date" defaultValue={String(r.due_date)} required />
            </Field>
            {reason}
            {button("Record reassignment")}
          </FormLayout>
        </Card>
      )}
      {r && detail?.canManage && r.status === "disqualified" && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send(lead(new FormData(e.currentTarget), "reopen"));
          }}
        >
          <h2>Reopen lead</h2>
          {reason}
          {button("Reopen with reason")}
        </FormLayout>
      )}
      {r &&
        detail?.canAct &&
        r.opportunity_id &&
        ["discovery", "proposal", "negotiation"].includes(String(r.stage)) && (
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Commercial handoff</h2>
            <p>
              Prepare the quotation in Sales for this customer and branch. Another manager must
              approve it before linking it here.
            </p>
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  quote = detail.quotes.find((q) => q.id === f.get("quote"));
                if (quote)
                  void send({
                    ...deal(f, "link-quote"),
                    quoteId: quote.id,
                    quoteVersion: Number(quote.version),
                  });
              }}
            >
              <Field appearance="standard">
                Approved quotation
                <Select appearance="standard" name="quote" required>
                  <option value="">Choose approved quotation</option>
                  {detail.quotes.map((q) => (
                    <option key={String(q.id)} value={String(q.id)}>
                      {String(q.number)}
                    </option>
                  ))}
                </Select>
              </Field>
              {reason}
              {button("Link approved quotation")}
            </FormLayout>
            {r.quote_id && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  void send(deal(new FormData(e.currentTarget), "negotiate"));
                }}
              >
                {reason}
                {button("Record negotiation")}
              </FormLayout>
            )}
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  sale = detail.sales.find((s) => s.id === f.get("sale"));
                if (sale)
                  void send({
                    ...deal(f, "win"),
                    salesId: sale.id,
                    salesVersion: Number(sale.version),
                  });
              }}
            >
              <Field appearance="standard">
                Confirmed sales order converted from quotation
                <Select appearance="standard" name="sale" required>
                  <option value="">Choose confirmed sales order</option>
                  {detail.sales.map((s) => (
                    <option key={String(s.id)} value={String(s.id)}>
                      {String(s.number)}
                    </option>
                  ))}
                </Select>
              </Field>
              {reason}
              {button("Record won opportunity")}
            </FormLayout>
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({ ...deal(f, "lose"), lossReason: f.get("lossReason") });
              }}
            >
              <Field appearance="standard">
                Loss reason
                <Select appearance="standard" name="lossReason">
                  <option value="price">Price</option>
                  <option value="availability">Availability</option>
                  <option value="competitor">Competitor</option>
                  <option value="timing">Timing</option>
                  <option value="no_response">No response</option>
                  <option value="other">Other</option>
                </Select>
              </Field>
              {reason}
              {button("Record lost opportunity")}
            </FormLayout>
          </Card>
        )}
      {r && detail?.canManage && r.contact_id && detail.guestCandidates.length > 0 && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              guest = detail.guestCandidates.find((g) => g.id === f.get("guestId"));
            if (guest)
              void send({
                domain: "identity",
                action: "link-guest",
                contactId: r.contact_id,
                guestId: guest.id,
                propertyId: guest.property_id,
                evidence: f.get("evidence"),
              });
          }}
        >
          <h2>Review PMS guest link</h2>
          <Field appearance="standard">
            PMS guest
            <Select appearance="standard" name="guestId" required>
              <option value="">Select guest</option>
              {detail.guestCandidates.map((g) => (
                <option key={String(g.id)} value={String(g.id)}>
                  {String(g.name)} · {String(g.property_name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Identity review evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} />
          </Field>
          {button("Confirm guest link")}
        </FormLayout>
      )}
      {r &&
        detail?.canAct &&
        r.opportunity_id &&
        ["discovery", "proposal", "negotiation"].includes(String(r.stage)) &&
        detail.reservations.some((x) => x.state === "confirmed") && (
          <FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                reservation = detail.reservations.find((x) => x.id === f.get("reservationId"));
              if (reservation)
                void send({
                  ...deal(f, "win-pms"),
                  reservationId: reservation.id,
                  propertyId: reservation.property_id,
                });
            }}
          >
            <h2>Confirm hotel outcome</h2>
            <Field appearance="standard">
              Confirmed PMS reservation
              <Select appearance="standard" name="reservationId" required>
                <option value="">Select reservation</option>
                {detail.reservations
                  .filter((x) => x.state === "confirmed")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.number)}
                    </option>
                  ))}
              </Select>
            </Field>
            {reason}
            {button("Record won from confirmed reservation")}
          </FormLayout>
        )}
      {r && detail?.canManage && r.stage === "lost" && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send(deal(new FormData(e.currentTarget), "reopen"));
          }}
        >
          <h2>Reopen opportunity</h2>
          <p>
            The previous quotation remains in history; approve and link a current quotation again.
          </p>
          {reason}
          {button("Reopen opportunity")}
        </FormLayout>
      )}
    </>
  );
}
