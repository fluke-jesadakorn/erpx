"use client";
import {recordCanonicalHref} from "@/lib/records/record-drawer";
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

import { workFetch } from "@/lib/work_projection/work-fetch";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Card, FormLayout } from "@/components/ui/foundation";

type Run = {
  id: string;
  presentation_currency: string;
  as_of_date: string;
  status: "draft" | "review" | "approved" | "published";
  source_hash: string;
  version: number;
  created_by: string;
  reviewed_by: string | null;
  approved_by: string | null;
  published_by: string | null;
  eliminations: Elimination[];
};

type Elimination = {
  id: string;
  source_transaction_id: string | null;
  description: string;
  debit_account_id: string;
  credit_account_id: string;
  amount_minor: number;
  status: "proposed" | "approved" | "posted";
  version: number;
  approved_by: string | null;
  posted_by: string | null;
  consolidation_journal_id: string | null;
};

type Transaction = {
  id: string;
  reference: string;
  currency: string;
  amount_minor: number;
  status: string;
};
type Account = { id: string; company_id: string; code: string; name: string; currency: string };

export function ConsolidationRunConsole({
  base,
  runs,
  transactions,
  accounts,
  canWrite,
  canApprove,
}: {
  base: string;
  runs: Run[];
  transactions: Transaction[];
  accounts: Account[];
  canWrite: boolean;
  canApprove: boolean;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const attempt = useRef<{ signature: string; key: string } | null>(null);
  const apiBase = base.replace("/holdings/", "/api/v1/holdings/"),
    endpoint = `${apiBase}/finance/consolidation-runs`;
  async function send(url: string, input: Record<string, unknown>) {
    const signature = `${url}:${JSON.stringify(input)}`;
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body: JSON.stringify(input),
      });
      const body = (await response.json()) as { error?: { message?: string } };
      if (!response.ok)
        throw new Error(body.error?.message ?? "Unable to save consolidation workflow.");
      attempt.current = null;
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry the same action.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="consolidation-runs-title">
      <div className="ui-section-header">
        <div>
          <h2 id="consolidation-runs-title">Consolidation runs</h2>
          <p className="muted">
            Snapshot closed company periods, require independent review and publish a controlled
            group result.
          </p>
        </div>
        <span className="ui-badge">{runs.length}</span>
      </div>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {canWrite && (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            const translationRates = String(form.get("translationRates") ?? "")
              .split(/\r?\n/)
              .map((line) => line.trim())
              .filter(Boolean)
              .map((line) => {
                const [currency, rate] = line.split("=").map((value) => value.trim());
                return { currency, rate };
              });
            void send(endpoint, {
              asOfDate: form.get("asOfDate"),
              presentationCurrency: form.get("presentationCurrency"),
              translationRates,
            });
          }}
        >
          <Field appearance="standard">
            As-of date
            <TextInput appearance="standard" name="asOfDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Presentation currency
            <TextInput appearance="standard"
              name="presentationCurrency"
              minLength={3}
              maxLength={3}
              defaultValue="THB"
              required
            />
          </Field>
          <Field appearance="standard">
            Translation rates (presentation currency per source currency)
            <TextArea appearance="standard" name="translationRates" rows={3} placeholder="USD=35.50\nEUR=38.20" />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            {busy ? "Preparing…" : "Prepare consolidation snapshot"}
          </Button>
        </FormLayout>
      )}
      {!canWrite && !runs.length && (
        <p className="muted">A Finance preparer must prepare the first consolidation snapshot.</p>
      )}
      {runs.length ? (
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
                <th>As of</th>
                <th>Currency</th>
                <th>Status</th>
                <th>Source snapshot</th>
                <th>Next action</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((run) => {
                const action =
                  run.status === "draft" && canWrite
                    ? "review"
                    : run.status === "review" && canApprove
                      ? "approve"
                      : run.status === "approved" && canApprove
                        ? "publish"
                        : null;
                return (
                  <tr key={run.id}>
                    <td>
                      <RecordLink href={`${base}/record-details/consolidation/${encodeURIComponent(run.id)}`}>{run.as_of_date}</RecordLink>
                      <div className="muted">{run.id}</div>
                    </td>
                    <td>{run.presentation_currency}</td>
                    <td>
                      {run.status} · v{run.version}
                      <div className="muted">{run.eliminations.length} elimination(s)</div>
                    </td>
                    <td>
                      <code>{String(run.source_hash).slice(0, 16)}</code>
                      <div className="muted">Prepared by {run.created_by}</div>
                    </td>
                    <td>
                      {action ? (
                        <form
                          className="ui-form-actions"
                          onSubmit={(event) => {
                            event.preventDefault();
                            const form = new FormData(event.currentTarget);
                            void send(`${endpoint}/${encodeURIComponent(run.id)}/${action}`, {
                              expectedVersion: Number(run.version),
                              reason: form.get("reason"),
                            });
                          }}
                        >
                          <TextInput appearance="standard"
                            name="reason"
                            required
                            minLength={3}
                            maxLength={2000}
                            placeholder="Review evidence"
                            aria-label={`Reason to ${action} ${run.as_of_date}`}
                          />
                          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                            {action}
                          </Button>
                        </form>
                      ) : run.status === "published" ? (
                        "Published"
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
        <p className="muted">
          No consolidation snapshots yet. Every active Group entity needs a closed period on or
          before the selected date.
        </p>
      )}
      {runs.map((run) => (
        <Card as="div" appearance="standard" className="ui-panel" key={`${run.id}-eliminations`}>
          <h3>Elimination accounts · {run.as_of_date}</h3>
          <p className="muted">
            Add a settled intercompany source while the run is draft, approve it independently, then
            post it to the controlled Group Finance journal before publishing.
          </p>
          {run.status === "draft" &&
            canWrite &&
            transactions.some((transaction) =>
              ["settled", "eliminated"].includes(transaction.status),
            ) &&
            accounts.length >= 2 && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget);
                  void send(`${endpoint}/${encodeURIComponent(run.id)}/eliminations`, {
                    sourceTransactionId: form.get("sourceTransactionId"),
                    debitAccountId: form.get("debitAccountId"),
                    creditAccountId: form.get("creditAccountId"),
                    amount: form.get("amount"),
                    description: form.get("description"),
                  });
                }}
              >
                <Field appearance="standard">
                  Settled source
                  <Select appearance="standard" name="sourceTransactionId" required>
                    <option value="">Choose transaction</option>
                    {transactions
                      .filter(
                        (transaction) =>
                          ["settled", "eliminated"].includes(transaction.status) &&
                          !run.eliminations.some(
                            (elimination) => elimination.source_transaction_id === transaction.id,
                          ),
                      )
                      .map((transaction) => (
                        <option key={transaction.id} value={transaction.id}>
                          {transaction.reference} · {transaction.currency}{" "}
                          {minorToDecimal(Number(transaction.amount_minor))}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Debit account
                  <Select appearance="standard" name="debitAccountId" required>
                    <option value="">Choose account</option>
                    {accounts.map((account) => (
                      <option key={account.id} value={account.id}>
                        {account.code} · {account.name} ({account.company_id})
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Credit account
                  <Select appearance="standard" name="creditAccountId" required>
                    <option value="">Choose account</option>
                    {accounts.map((account) => (
                      <option key={account.id} value={account.id}>
                        {account.code} · {account.name} ({account.company_id})
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Amount
                  <TextInput appearance="standard"
                    name="amount"
                    required
                    inputMode="decimal"
                    pattern="[0-9]+(\.[0-9]{1,6})?"
                  />
                </Field>
                <Field appearance="standard">
                  Description
                  <TextInput appearance="standard"
                    name="description"
                    required
                    minLength={3}
                    maxLength={500}
                    placeholder="Eliminate intercompany receivable/payable"
                  />
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  {busy ? "Saving…" : "Propose elimination"}
                </Button>
              </FormLayout>
            )}
          {run.eliminations.length ? (
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
                    <th>Description</th>
                    <th>Accounts</th>
                    <th>Amount</th>
                    <th>Status</th>
                    <th>Next action</th>
                  </tr>
                </thead>
                <tbody>
                  {run.eliminations.map((elimination) => {
                    const action =
                      elimination.status === "proposed" && canApprove
                        ? "approve"
                        : elimination.status === "approved" &&
                            run.status === "approved" &&
                            canApprove
                          ? "post"
                          : null;
                    return (
                      <tr key={elimination.id}>
                        <td>
                          <RecordLink href={recordCanonicalHref(base,{kind:"consolidation_elimination",id:elimination.id})}>{elimination.description}</RecordLink>
                          <div className="muted">
                            {elimination.source_transaction_id ?? "Manual source"}
                          </div>
                        </td>
                        <td>
                          {elimination.debit_account_id} → {elimination.credit_account_id}
                        </td>
                        <td>
                          {run.presentation_currency}{" "}
                          {minorToDecimal(Number(elimination.amount_minor))}
                        </td>
                        <td>
                          {elimination.status} · v{elimination.version}
                        </td>
                        <td>
                          {action ? (
                            <form
                              className="ui-form-actions"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const form = new FormData(event.currentTarget);
                                void send(
                                  `${apiBase}/finance/consolidation-eliminations/${encodeURIComponent(elimination.id)}/${action}`,
                                  {
                                    expectedVersion: Number(elimination.version),
                                    reason: form.get("reason"),
                                  },
                                );
                              }}
                            >
                              <TextInput appearance="standard"
                                name="reason"
                                required
                                minLength={3}
                                maxLength={2000}
                                placeholder="Evidence and reason"
                                aria-label={`Reason to ${action} ${elimination.description}`}
                              />
                              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                                {action}
                              </Button>
                            </form>
                          ) : elimination.status === "posted" ? (
                            `Posted · ${elimination.consolidation_journal_id ?? "journal"}`
                          ) : (
                            "Waiting for the required run state or independent reviewer"
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </Table>
            </TableFrame>
          ) : (
            <p className="muted">No elimination account entries yet.</p>
          )}
        </Card>
      ))}
    </Card>
  );
}
