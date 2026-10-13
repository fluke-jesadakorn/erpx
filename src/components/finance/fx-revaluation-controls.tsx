"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { financeCommandFetch as workFetch } from "@/lib/finance/finance-command-fetch";
import { FormLayout, FormGrid, Stack, Card } from "@/components/ui/foundation";

type Row = Record<string, string | number | null>;
type Workspace = { period: Row; currencies: Row[]; accounts: Row[]; runs: Row[] };

function apiBase(base: string) {
  return base.replace("/holdings/", "/api/v1/holdings/");
}

function ErrorMessage({ message }: { message: string }) {
  return message ? (
    <InlineNotice tone="danger" className="error" role="alert">
      {message}
    </InlineNotice>
  ) : null;
}

function RunDecision({
  base,
  run,
  canApprove,
  userId,
}: {
  base: string;
  run: Row;
  canApprove: boolean;
  userId: string;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  if (run.status !== "prepared" || !canApprove) return null;
  if (String(run.created_by) === userId)
    return (
      <p className="muted">
        The preparer cannot approve this run. A different Finance approver must review it.
      </p>
    );

  return (
    <FormLayout
      className="ui-form-grid"
      onSubmit={async (event) => {
        event.preventDefault();
        setBusy(true);
        setError("");
        const form = new FormData(event.currentTarget);
        const submitter = (event.nativeEvent as SubmitEvent).submitter as HTMLButtonElement | null;
        const decision = submitter?.value === "reject" ? "reject" : "approve";
        try {
          const response = await workFetch(
            `${apiBase(base)}/finance/fx-revaluations/${run.id}/${decision}`,
            {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
                "Idempotency-Key": crypto.randomUUID(),
              },
              body: JSON.stringify({
                expectedVersion: Number(run.version),
                proof: String(form.get("proof")),
              }),
            },
          );
          const body = (await response.json()) as { error?: { message?: string } };
          if (!response.ok)
            throw new Error(body.error?.message ?? "Unable to save the review decision");
          router.refresh();
        } catch (reason) {
          setError(reason instanceof Error ? reason.message : "Unable to save the review decision");
        } finally {
          setBusy(false);
        }
      }}
    >
      <Field appearance="standard">
        Review evidence
        <TextInput appearance="standard"
          name="proof"
          required
          minLength={3}
          placeholder="Closing-rate source and balance review"
        />
      </Field>
      <div className="button-row">
        <Button appearance="standard" clientManaged type="submit" variant="primary" name="decision" value="approve" disabled={busy}>
          Approve and post
        </Button>
        <Button appearance="standard" clientManaged type="submit" variant="secondary" name="decision" value="reject" disabled={busy}>
          Return for correction
        </Button>
      </div>
      <ErrorMessage message={error} />
    </FormLayout>
  );
}

function ChartPreparation({
  base,
  data,
  chartId,
  canPrepare,
}: {
  base: string;
  data: Workspace;
  chartId: string;
  canPrepare: boolean;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const currencies = data.currencies.filter((row) => String(row.chart_id) === chartId);
  const accounts = data.accounts.filter((row) => String(row.chart_id) === chartId);
  const gains = accounts.filter((row) => row.category === "other_income");
  const losses = accounts.filter((row) => row.category === "other_expense");
  const posted = data.runs.some(
    (row) => String(row.chart_id) === chartId && row.status === "posted",
  );
  const chartName = String(currencies[0]?.chart_name ?? chartId);
  const baseCurrency = String(currencies[0]?.base_currency ?? "");

  if (!canPrepare || posted) return null;
  if (!gains.length || !losses.length)
    return (
      <p className="error">
        {chartName}: configure functional-currency FX gain and loss accounts in {baseCurrency}{" "}
        before preparation.
      </p>
    );

  return (
    <form
      className="ui-panel ui-stack"
      onSubmit={async (event) => {
        event.preventDefault();
        setBusy(true);
        setError("");
        const form = new FormData(event.currentTarget);
        try {
          const response = await workFetch(`${apiBase(base)}/finance/fx-revaluations`, {
            method: "POST",
            headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
            body: JSON.stringify({
              chartId,
              periodId: String(data.period.id),
              entryDate: String(data.period.end_date),
              rates: currencies.map((row) => ({
                currency: String(row.currency),
                rate: String(form.get(`rate-${row.currency}`)),
              })),
              gainAccountId: String(form.get("gainAccountId")),
              lossAccountId: String(form.get("lossAccountId")),
              proof: String(form.get("proof")),
            }),
          });
          const body = (await response.json()) as { error?: { message?: string } };
          if (!response.ok)
            throw new Error(body.error?.message ?? "Unable to prepare the revaluation");
          router.refresh();
        } catch (reason) {
          setError(reason instanceof Error ? reason.message : "Unable to prepare the revaluation");
        } finally {
          setBusy(false);
        }
      }}
    >
      <div>
        <h4>{chartName}</h4>
        <p className="muted">
          Functional currency {baseCurrency} · posting date {String(data.period.end_date)}
        </p>
      </div>
      <FormGrid as="div" className="ui-form-grid">
        {currencies.map((row) => (
          <Field appearance="standard" key={String(row.currency)}>
            Closing rate: 1 {String(row.currency)} = {baseCurrency}
            <TextInput appearance="standard"
              name={`rate-${row.currency}`}
              inputMode="decimal"
              required
              placeholder="For example, 36.250000"
            />
          </Field>
        ))}
        <Field appearance="standard">
          Unrealized gain account
          <Select appearance="standard" name="gainAccountId" required>
            {gains.map((row) => (
              <option key={String(row.id)} value={String(row.id)}>
                {String(row.code)} · {String(row.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Unrealized loss account
          <Select appearance="standard" name="lossAccountId" required>
            {losses.map((row) => (
              <option key={String(row.id)} value={String(row.id)}>
                {String(row.code)} · {String(row.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Closing-rate evidence
          <TextInput appearance="standard"
            name="proof"
            required
            minLength={3}
            placeholder="Rate source and review date"
          />
        </Field>
      </FormGrid>
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
        {busy ? "Preparing…" : "Prepare for approval"}
      </Button>
      <ErrorMessage message={error} />
    </form>
  );
}

export function FxRevaluationControls({
  base,
  data,
  canPrepare,
  canApprove,
  userId,
}: {
  base: string;
  data: Workspace;
  canPrepare: boolean;
  canApprove: boolean;
  userId: string;
}) {
  const chartIds = [...new Set(data.currencies.map((row) => String(row.chart_id)))];
  if (data.period.state !== "open" && !data.runs.length) return null;
  return (
    <Stack as="section" className="ui-stack" aria-labelledby={`fx-${data.period.id}`}>
      <div>
        <h3 id={`fx-${data.period.id}`}>Period-end foreign-currency revaluation</h3>
        <p className="muted">
          Finance supplies closing rates. A different approver reviews and posts the
          functional-currency adjustment with retained source evidence.
        </p>
      </div>
      {data.period.state === "open" &&
        chartIds.map((chartId) => (
          <ChartPreparation
            key={chartId}
            base={base}
            data={data}
            chartId={chartId}
            canPrepare={canPrepare}
          />
        ))}
      {!chartIds.length && !data.runs.length && (
        <p className="muted">
          No foreign-currency receivables or payables are outstanding at period end.
        </p>
      )}
      {data.runs.map((run) => (
        <Card as="article" appearance="standard" className="ui-panel ui-stack" key={String(run.id)}>
          <div className="split-row">
            <div>
              <strong>{String(run.chart_name)}</strong>
              <p className="muted">
                Closing rates:{" "}
                {Object.entries(JSON.parse(String(run.rates_json)) as Record<string, string>)
                  .map(([currency, rate]) => `${currency} ${rate}`)
                  .join(" · ")}
              </p>
            </div>
            <span className="status-badge">{String(run.status)}</span>
          </div>
          <p className="muted">
            Preparation evidence: {String(run.preparation_proof)} · source{" "}
            {String(run.source_hash).slice(0, 12)}
          </p>
          {run.decision_proof && (
            <p className="muted">Decision evidence: {String(run.decision_proof)}</p>
          )}
          <RunDecision base={base} run={run} canApprove={canApprove} userId={userId} />
        </Card>
      ))}
    </Stack>
  );
}
