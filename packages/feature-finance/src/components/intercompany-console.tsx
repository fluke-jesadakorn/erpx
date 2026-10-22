"use client";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import { useFinanceDraftScope, captureFinanceForm, acknowledgeFinanceForm } from "@erpx/feature-finance/components/finance-draft-boundary";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink } from "@erpx/feature-records/components/record-link";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";

import { financeCommandFetch as workFetch } from "@erpx/feature-finance/lib/finance-command-fetch";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { Card, FormLayout } from "@erpx/ui/components/foundation";

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
  const draftScope=useFinanceDraftScope();
  const [hasAttempt,setHasAttempt]=useState(false);
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    attempt = useRef<{ signature: string; key: string } | null>(null);
  const apiBase = base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/");
  const canPrepare=useFinanceActionAllowed("new-work","finance.advanced"),canSettle=useFinanceActionAllowed("settle-existing","finance.advanced");
  async function post(resource: string, path: string, input: Record<string, unknown>) {
    if (!(path ? canSettle : canPrepare) && !workFetch.hasAttempt(`${apiBase}/finance/${resource}${path}`,JSON.stringify(input))) {setError("This intercompany action is unavailable for the current package.");return;}
    const completion=captureFinanceForm();
    const signature = `${resource}${path}:${JSON.stringify(input)}`;
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    setHasAttempt(true);
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`${apiBase}/finance/${resource}${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body: JSON.stringify(input),
      });
      const body = (await response.json()) as { data?: unknown; error?: { message?: string } };
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save Finance setup.");
      if (body.data === undefined) throw new Error("Finance acknowledgement is missing. Retry the same action.");
      acknowledgeFinanceForm(completion);setHasAttempt(false);
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
          <FormLayout data-finance-draft={draftScope}
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
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || (!canPrepare && !hasAttempt)}>
              {busy ? "Registering…" : "Register Group entity"}
            </Button>
          </FormLayout>
        </Card>
      )}
      {data.canWrite && data.entities.length > 0 && (
        <FormDisclosure appearance="standard" drawerTitle="Create intercompany item" error={error}><summary>Create intercompany item</summary>
        <FormLayout data-finance-draft={draftScope}
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
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || (!canPrepare && !hasAttempt) || !targetEntities.length}>
            {busy ? "Saving…" : "Create intercompany item"}
          </Button>
        </FormLayout></FormDisclosure>
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
                      <RecordLink href={canonicalUiHref(`${base}/record-details/intercompany/${encodeURIComponent(row.id)}`)}>{row.reference}</RecordLink>
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
                        <FormLayout data-finance-draft={draftScope}
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
                          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy || (!canSettle && !hasAttempt)}>
                            {action}
                          </Button>
                        </FormLayout>
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
