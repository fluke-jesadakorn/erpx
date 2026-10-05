"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { crmIdentities } from "@/server/crm-identities";
import { HubForm } from "@/components/sales-customers/hub-form";
import { HubPagination, HubSegments } from "@/components/sales-customers/hub-shell";
import { HubFilters, HubTable, hubPage } from "@/components/sales-customers/hub-list";
import { RecordLink as Link } from "@/components/records/record-link";
import { queryValue, type HubQuery } from "@/lib/sales-customers";
import styles from "@/components/sales-customers/hub.module.css";
import { FormLayout } from "@/components/ui/foundation";

export function CRMIdentityConsole({
  base,
  data,
  query = {},
}: {
  query?: HubQuery;
  base: string;
  data: Awaited<ReturnType<typeof crmIdentities>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ body: string; key: string } | null>(null);
  async function send(body: Record<string, unknown>) {
    if (busy) return;
    const encoded = JSON.stringify({ ...body, domain: "identity" });
    if (last.current?.body !== encoded) last.current = { body: encoded, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/crm`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: encoded,
        }),
        j = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(j.error?.message ?? "Unable to save CRM identity.");
      }
      last.current = null;
      window.dispatchEvent(new Event("erpx:hub-saved"));
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the same action.");
    } finally {
      setBusy(false);
    }
  }
  const items = (query.list === "contacts" ? data.contacts : data.accounts).filter(row => !query.q || `${row.name} ${row.detail ?? ""}`.toLowerCase().includes(queryValue(query, "q").toLowerCase())), rows = hubPage(items, query);
  return (
    <section className={styles.section}>
      <p className={styles.description}>Prospects stay separate until their customer link is reviewed.</p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <div className={styles.actions}>
      {data.canCreate && (
        <>
          <HubForm title="New prospect account" label="New prospect"><FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({ action: "create-account", kind: f.get("kind"), name: f.get("name") });
            }}
          >
            <h3>New account</h3>
            <Field appearance="standard">
              Type
              <Select appearance="standard" name="kind">
                <option value="business">Business</option>
                <option value="individual">Individual</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required minLength={2} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create account
            </Button>
          </FormLayout></HubForm>
          <HubForm title="New contact" label="New contact" primary={false}><FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                action: "create-contact",
                accountId: f.get("accountId"),
                name: f.get("name"),
                detail: f.get("detail"),
              });
            }}
          >
            <h3>New contact</h3>
            <Field appearance="standard">
              Account
              <Select appearance="standard" name="accountId" required>
                <option value="">Select account</option>
                {data.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required minLength={2} />
            </Field>
            <Field appearance="standard">
              Contact detail
              <TextArea appearance="standard" name="detail" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create contact
            </Button>
          </FormLayout></HubForm>
        </>
      )}
      {data.canReview && (
        <HubForm title="Review Finance customer link" label="Review customer link" primary={false}><FormLayout
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              account = data.accounts.find((a) => a.id === f.get("accountId"));
            if (account)
              void send({
                action: "link-finance",
                accountId: account.id,
                customerId: f.get("customerId"),
                expectedVersion: Number(account.version),
                evidence: f.get("evidence"),
              });
          }}
        >
          <h3>Review Finance customer link</h3>
          <Field appearance="standard">
            Prospect account
            <Select appearance="standard" name="accountId" required>
              <option value="">Select account</option>
              {data.accounts
                .filter((a) => !a.finance_party_id)
                .map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Verified Finance customer
            <Select appearance="standard" name="customerId" required>
              <option value="">Select customer</option>
              {data.customers.map((c) => (
                <option key={String(c.id)} value={String(c.id)}>
                  {String(c.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Review evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Confirm link
          </Button>
        </FormLayout></HubForm>
      )}
      </div>
      <div className={styles.section}>
        <HubSegments base={base} hub="customers" query={{ ...query, view: "contacts" }} name="list" selected={queryValue(query, "list", "accounts")} label="Prospect record type" values={[["accounts", "Prospects"], ["contacts", "Contacts"]]} />
        <HubFilters query={{ ...query, view: "contacts" }} placeholder="Search prospects and contacts" />
        <HubTable label="Prospects and contacts" columns={query.list === "contacts" ? ["Contact", "Prospect account", "Contact detail"] : ["Prospect", "Type", "Customer link"]}>
          {rows.map(row => <tr key={String(row.id)}>
            <td><span className={styles.recordName}>{String(row.name)}</span></td>
            <td data-label={query.list === "contacts" ? "Prospect account" : "Type"}>{query.list === "contacts" ? String(data.accounts.find(account => account.id === row.account_id)?.name ?? "Account unavailable") : String(row.kind)}</td>
            <td data-label={query.list === "contacts" ? "Contact detail" : "Customer link"}>{query.list === "contacts" ? String(row.detail) : row.finance_party_id ? <Link href={`${base}/customers/${row.finance_party_id}`}>{String(row.finance_customer_name)}</Link> : "Prospect"}</td>
          </tr>)}
        </HubTable>
        {!items.length && <p>No matching records in this scope.</p>}
        <HubPagination base={base} hub="customers" query={{ ...query, view: "contacts" }} count={rows.length} total={items.length} />
      </div>
    </section>
  );
}
