"use client";
import {PayrollCommandRecovery} from "@erpx/feature-people_payroll/components/payroll-command-recovery";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { useEffect, useState } from "react";
import { decimalToMinor, minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@erpx/feature-people_payroll/lib/payroll-errors";
import { Card } from "@erpx/ui/components/foundation";
import {usePayrollCommand} from "@erpx/feature-people_payroll/components/use-payroll-command";
import {PayrollForm} from "@erpx/feature-people_payroll/components/payroll-draft-boundary";
import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import {payrollSnapshotMatches} from "@erpx/feature-people_payroll/lib/payroll-purchases";
type Obligation = {
  kind: "withholding_tax" | "social_security";
  accountId: string | null;
  accruedMinor: number;
  remittedMinor: number;
  outstandingMinor: number;
};
export type PayrollLiabilitiesSnapshot = {
  runId: string;
  runVersion: number;
  currency: string;
  obligations: Obligation[];
};
export function PayrollLiabilitiesPanel({
  base,
  runId,
  version,
  initial,
}: {
  base: string;
  runId: string;
  version: number;
  userId: string;
  initial?: PayrollLiabilitiesSnapshot | null;
}) {
  const {send, busy, error: commandError, notice, canSettle, recovery} = usePayrollCommand(base, `run:${runId}:liabilities`, JSON.stringify([runId, version, initial]));
  const currentInitial = payrollSnapshotMatches(initial, runId, version) ? initial : null;
  const [data, setData] = useState<PayrollLiabilitiesSnapshot | null>(currentInitial);
  const [previousInitial, setPreviousInitial] = useState(initial);
  const [error, setError] = useState("");
  const [drawerRevisions, setDrawerRevisions] = useState<Record<string, number>>({});
  if (previousInitial !== initial) {setPreviousInitial(initial); setData(currentInitial);}
  useEffect(() => {
    if (payrollSnapshotMatches(initial, runId, version)) return;
    let active = true;
    void workFetch(`/api/v1${base}/people/payroll/runs/${encodeURIComponent(runId)}/liabilities`, {
      cache: "no-store",
    })
      .then(async (response) => {
        const result = (await response.json()) as {
          data?: PayrollLiabilitiesSnapshot;
          error?: PayrollApiError;
        };
        if (!response.ok || !result.data)
          throw result.error ?? { message: "Payroll liabilities are unavailable." };
        if (active) setData(result.data);
      })
      .catch((cause) => {
        if (active) setError(payrollErrorMessage(cause, "Payroll liabilities are unavailable."));
      });
    return () => {
      active = false;
    };
  }, [base, runId, version, initial]);
  async function remit(form: HTMLFormElement, kind: Obligation["kind"]) {
    if (!data) return;
    const fields = new FormData(form);
    let amountMinor: number;
    try {
      amountMinor = decimalToMinor(String(fields.get("amount") ?? ""));
    } catch {
      setError("Enter an amount with at most two decimal places.");
      return;
    }
    const input = {
      kind,
      amountMinor,
      expectedRunVersion: data.runVersion,
      bankAccountId: String(fields.get("bankAccountId") ?? ""),
      bankTransactionId: String(fields.get("bankTransactionId") ?? ""),
      bankVersion: Number(fields.get("bankVersion")),
      bankReference: String(fields.get("bankReference") ?? ""),
    };
    await send(`payroll/runs/${encodeURIComponent(runId)}/remit`, input);
  }
  const labels: Record<Obligation["kind"], string> = {
    withholding_tax: "Withholding tax",
    social_security: "Social security",
  };
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="payroll-liabilities-heading">
      <h2 id="payroll-liabilities-heading">{"Statutory liabilities"}</h2>
      <p className="muted">{"Balances remain payable until bank evidence confirms remittance."}</p>
      <PayrollCommandRecovery {...recovery} appearance="standard"/>{(error || commandError) && <InlineNotice tone="danger" role="alert">{error || commandError}</InlineNotice>}{notice && <p role="status">{notice}</p>}
      {!data && !error && <p className="muted">{"Loading liabilities…"}</p>}
      {data?.obligations.map((obligation) => (
        <article className="ui-record-item" key={obligation.kind}>
          <div className="row-main">
            <strong>{labels[obligation.kind]}</strong>
            <span>
              {"Accrued"} {data.currency} {minorToDecimal(obligation.accruedMinor)} · {"Remitted"}{" "}
              {data.currency} {minorToDecimal(obligation.remittedMinor)}
            </span>
            <strong>
              {"Outstanding"} {data.currency} {minorToDecimal(obligation.outstandingMinor)}
            </strong>
          </div>
          {canSettle && obligation.outstandingMinor > 0 && (
            <FormDisclosure appearance="standard" drawerTitle={`Record ${labels[obligation.kind]} remittance`} onDiscard={() => setDrawerRevisions(current => ({...current, [obligation.kind]: (current[obligation.kind] ?? 0) + 1}))}><summary>Record remittance</summary><PayrollForm key={drawerRevisions[obligation.kind] ?? 0}
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                void remit(event.currentTarget, obligation.kind);
              }}
            >
              <Field appearance="standard">
                {"Amount"} ({data.currency})
                <TextInput appearance="standard"
                  name="amount"
                  type="number"
                  min="0.01"
                  step="0.01"
                  max={minorToDecimal(obligation.outstandingMinor)}
                  defaultValue={minorToDecimal(obligation.outstandingMinor)}
                  required
                />
              </Field>
              <Field appearance="standard">
                {"Bank account ID"}
                <TextInput appearance="standard" name="bankAccountId" required />
              </Field>
              <Field appearance="standard">
                {"Bank transaction ID"}
                <TextInput appearance="standard" name="bankTransactionId" required />
              </Field>
              <Field appearance="standard">
                {"Bank transaction version"}
                <TextInput appearance="standard" name="bankVersion" type="number" min="1" step="1" required />
              </Field>
              <Field appearance="standard">
                {"Bank reference"}
                <TextInput appearance="standard" name="bankReference" required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                {"Record remittance"}
              </Button>
            </PayrollForm></FormDisclosure>
          )}
        </article>
      ))}
      <p className="muted">
        {"Select an imported debit transaction from"}{" "}
        <Link href={canonicalUiHref(`${base}/finance/bank`)}>Finance · Cash & banking</Link> {"before recording."}
      </p>
    </Card>
  );
}
