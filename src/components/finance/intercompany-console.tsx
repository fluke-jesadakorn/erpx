"use client";
import { RecordLink } from "@/components/records/record-link";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { minorToDecimal } from "@/lib/finance-money";
import { Card, FormLayout } from "@/components/ui/foundation";

type Entity = {
  company_id: string;
  reporting_name: string;
  functional_currency: string;
  status: string;
};
type AvailableCompany = { company_id: string; display_name: string };
type Transaction = {
  id: string;
  source_company_id: string;
  target_company_id: string;
  reference: string;
  currency: string;
  amount_minor: number;
  status: string;
  version: number;
  source_journal_id: string | null;
  target_journal_id: string | null;
};
type Workspace = {
  companyId: string;
  transactions: Transaction[];
  entities: Entity[];
  availableCompanies: AvailableCompany[];
  canWrite: boolean;
  canApprove: boolean;
  canPay: boolean;
};

export function IntercompanyConsole({ base, data }: { base: string; data: Workspace }) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    attempt = useRef<{ signature: string; key: string } | null>(null);
  const apiBase = base.replace("/holdings/", "/api/v1/holdings/");
  async function post(resource: string, path: string, input: Record<string, unknown>) {
    const signature = `${resource}${path}:${JSON.stringify(input)}`;
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`${apiBase}/finance/${resource}${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body: JSON.stringify(input),
      });
      const body = (await response.json()) as { error?: { message?: string } };
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save Finance setup.");
      attempt.current = null;
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry the same action.");
    } finally {
      setBusy(false);
    }
  }
  const send = (path: string, input: Record<string, unknown>) => post("intercompany", path, input);
  const targetEntities = data.entities.filter((entity) => entity.company_id !== data.companyId);
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="intercompany-title">
      <div className="ui-section-header">
        <div>
          <h2 id="intercompany-title">Intercompany transactions</h2>
          <p className="muted">
            Create a source-linked item, then require an independent match, settlement, and
            elimination decision.
          </p>
        </div>
        <span className="ui-badge">{data.transactions.length}</span>
      </div>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {data.canWrite && data.availableCompanies.length > 0 && (
        <Card as="div" appearance="standard" className="ui-panel">
          <h3>Register a Group Finance entity</h3>
          <p className="muted">
            Add an active Company organization node before using intercompany matching. This is an
            audited Holding setup change.
          </p>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void post("group-entities", "", {
                companyId: form.get("companyId"),
                reportingName: form.get("reportingName"),
                functionalCurrency: form.get("functionalCurrency"),
              });
            }}
          >
            <Field appearance="standard">
              Company
              <Select appearance="standard" name="companyId" required>
                <option value="">Choose active company</option>
                {data.availableCompanies.map((company) => (
                  <option key={company.company_id} value={company.company_id}>
                    {company.display_name} · {company.company_id}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Reporting name
              <TextInput appearance="standard"
                name="reportingName"
                required
                minLength={2}
                maxLength={160}
                placeholder="Northstar Thailand"
              />
            </Field>
            <Field appearance="standard">
              Functional currency
              <TextInput appearance="standard"
                name="functionalCurrency"
                required
                minLength={3}
                maxLength={3}
                defaultValue="THB"
              />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              {busy ? "Registering…" : "Register Group entity"}
            </Button>
          </FormLayout>
        </Card>
      )}
      {data.canWrite && data.entities.length > 0 && (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            void send("", {
              targetCompanyId: form.get("targetCompanyId"),
              reference: form.get("reference"),
              currency: form.get("currency"),
              amount: form.get("amount"),
              sourceJournalId: form.get("sourceJournalId") || undefined,
              targetJournalId: form.get("targetJournalId") || undefined,
            });
          }}
        >
          <Field appearance="standard">
            Target company
            <Select appearance="standard" name="targetCompanyId" required>
              <option value="">Choose company</option>
              {targetEntities.map((entity) => (
                <option key={entity.company_id} value={entity.company_id}>
                  {entity.reporting_name} · {entity.company_id}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Reference
            <TextInput appearance="standard"
              name="reference"
              required
              minLength={2}
              maxLength={120}
              placeholder="IC-2026-001"
            />
          </Field>
          <Field appearance="standard">
            Currency
            <TextInput appearance="standard" name="currency" required minLength={3} maxLength={3} defaultValue="THB" />
          </Field>
          <Field appearance="standard">
            Amount
            <TextInput appearance="standard" name="amount" required inputMode="decimal" pattern="[0-9]+(\.[0-9]{1,6})?" />
          </Field>
          <Field appearance="standard">
            Source posted journal
            <TextInput appearance="standard" name="sourceJournalId" placeholder="Optional journal id" />
          </Field>
          <Field appearance="standard">
            Target posted journal
            <TextInput appearance="standard" name="targetJournalId" placeholder="Optional journal id" />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !targetEntities.length}>
            {busy ? "Saving…" : "Create intercompany item"}
          </Button>
        </FormLayout>
      )}
      {!targetEntities.length && data.canWrite && (
        <p className="muted">
          {data.entities.length === 0
            ? "Register this company and at least one target company above before creating an intercompany item."
            : data.availableCompanies.length > 0
              ? "Register another active Company above before creating an intercompany item."
              : "All active Company nodes are already registered; add another Company organization node to create a new target."}
        </p>
      )}
      {data.transactions.length ? (
        <TableFrame appearance="standard"
          label="Records"
          className="business-table-wrap"
          tabIndex={0}
          role="region"
          aria-label="Financial table — scroll for more columns"
        >
          <Table className="business-table">
            <thead>
              <tr>
                <th>Reference</th>
                <th>Companies</th>
                <th>Amount</th>
                <th>Status</th>
                <th>Next action</th>
              </tr>
            </thead>
            <tbody>
              {data.transactions.map((row) => {
                const action =
                  row.status === "unmatched" && data.canApprove
                    ? "match"
                    : row.status === "matched" && data.canPay
                      ? "settle"
                      : row.status === "settled" && data.canApprove
                        ? "eliminate"
                        : null;
                return (
                  <tr key={row.id}>
                    <td>
                      <RecordLink href={`${base}/record-details/intercompany/${encodeURIComponent(row.id)}`}>{row.reference}</RecordLink>
                      <div className="muted">{row.id}</div>
                    </td>
                    <td>
                      {row.source_company_id} → {row.target_company_id}
                    </td>
                    <td>
                      {row.currency} {minorToDecimal(Number(row.amount_minor))}
                    </td>
                    <td>{row.status}</td>
                    <td>
                      {action ? (
                        <form
                          className="ui-form-actions"
                          onSubmit={(event) => {
                            event.preventDefault();
                            const form = new FormData(event.currentTarget);
                            void send(`/${encodeURIComponent(row.id)}/${action}`, {
                              expectedVersion: Number(row.version),
                              reason: form.get("reason"),
                            });
                          }}
                        >
                          <TextInput appearance="standard"
                            name="reason"
                            required
                            minLength={3}
                            maxLength={2000}
                            placeholder="Evidence and reason"
                            aria-label={`Reason to ${action} ${row.reference}`}
                          />
                          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                            {action}
                          </Button>
                        </form>
                      ) : row.status === "eliminated" ? (
                        "Complete"
                      ) : (
                        "Waiting for an authorized independent reviewer"
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </Table>
        </TableFrame>
      ) : (
        <p className="muted">No intercompany transactions yet.</p>
      )}
    </Card>
  );
}
