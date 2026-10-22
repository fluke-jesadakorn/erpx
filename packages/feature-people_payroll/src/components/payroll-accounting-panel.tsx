"use client";
import {PayrollCommandRecovery} from "@erpx/feature-people_payroll/components/payroll-command-recovery";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";

import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { useRef, useState } from "react";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@erpx/feature-people_payroll/lib/payroll-errors";
import { Card } from "@erpx/ui/components/foundation";
import {usePayrollCommand} from "@erpx/feature-people_payroll/components/use-payroll-command";
import {PayrollForm} from "@erpx/feature-people_payroll/components/payroll-draft-boundary";
import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import {capturePayrollForm, payrollFormUnchanged} from "@erpx/feature-people_payroll/lib/payroll-command-fetch";
import type {WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
type Row = Record<string, unknown>;
type Options = {
  charts: Row[];
  periods: Row[];
  accounts: Row[];
  mappings?: Row[];
};
type Payload = Record<string, string | number>;
type Preview = {
  lines: Array<{
    accountId: string;
    description: string;
    debit: string;
    credit: string;
    dimensions?: Record<string, string>;
  }>;
  totals: {
    debitMinor: number;
    creditMinor: number;
  };
  categories: Record<string, number>;
};
const mappings: Array<{
  name: string;
  label: string;
  category: string;
  required?: boolean;
}> = [
  { name: "expenseAccountId", label: "Wages expense", category: "expense", required: true },
  { name: "overtimeExpenseAccountId", label: "OT expense", category: "expense" },
  { name: "bonusExpenseAccountId", label: "Bonus expense", category: "expense" },
  { name: "serviceChargeExpenseAccountId", label: "Service charge expense", category: "expense" },
  { name: "employerExpenseAccountId", label: "Employer contribution expense", category: "expense" },
  { name: "netPayableAccountId", label: "Net pay payable", category: "liability", required: true },
  { name: "taxPayableAccountId", label: "Withholding tax payable", category: "liability" },
  {
    name: "employerContributionAccountId",
    label: "Employer contributions payable",
    category: "liability",
  },
  {
    name: "socialSecurityPayableAccountId",
    label: "Social security payable",
    category: "liability",
  },
  { name: "advanceReceivableAccountId", label: "Advance receivable", category: "asset" },
  { name: "benefitPayableAccountId", label: "Benefits payable", category: "liability" },
  {
    name: "otherDeductionPayableAccountId",
    label: "Other deductions payable",
    category: "liability",
  },
];
export function PayrollAccountingPanel({
  base,
  run,
  options,
}: {
  base: string;
  run: Row;
  options: Options | null;
  userId: string;
}) {
  const {send, busy: commandBusy, error: commandError, setError: clearCommandError, notice, canSettle, recovery} = usePayrollCommand(base, `run:${run.id}:accounting`, JSON.stringify([run, options]));
  const approvedMapping = options?.mappings?.find(
    (mapping) =>
      String(mapping.pay_group_id) === String(run.pay_group_id) &&
      String(mapping.status ?? "approved") === "approved",
  );
  const [chartId, setChartId] = useState(
    String(approvedMapping?.chart_id ?? options?.charts[0]?.id ?? ""),
  );
  const [manual, setManual] = useState(false);
  const [preview, setPreview] = useState<{
    value: Preview;
    payload: Payload;
    completion?: WorkCommandCompletion;
  } | null>(null);
  const [previewBusy, setBusy] = useState(false);
  const busy = previewBusy || commandBusy;
  const previewReads = useRef(0);
  const [formRevision, setFormRevision] = useState(0);
  const [error, setError] = useState("");
  if (!options)
    return (
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>{"Accounting preview"}</h2><PayrollCommandRecovery {...recovery} appearance="standard"/>
        <InlineNotice tone="info" role="status">{"An active chart and open period are required."}</InlineNotice>
      </Card>
    );
  const accounts = options.accounts.filter((account) => String(account.chart_id) === chartId);
  const periodEnd = String(run.period_end);
  const period =
    options.periods.find(
      (item) => String(item.start_date) <= periodEnd && periodEnd <= String(item.end_date),
    ) ?? options.periods[0];
  const entryDate =
    period && String(period.start_date) > periodEnd ? String(period.start_date) : periodEnd;
  function payload(form: HTMLFormElement): Payload {
    const fields = new FormData(form);
    const result: Payload = {
      chartId: String(fields.get("chartId")),
      periodId: String(fields.get("periodId")),
      entryDate: String(fields.get("entryDate")),
      expectedVersion: Number(run.version),
    };
    for (const mapping of mappings) {
      const value = String(fields.get(mapping.name) ?? "");
      if (value) result[mapping.name] = value;
    }
    return result;
  }
  async function showPreview(input: Payload) {
    if (!canSettle) return;
    const read = ++previewReads.current, completion = capturePayrollForm();
    clearCommandError("");
    setBusy(true);
    setError("");
    setPreview(null);
    try {
      const response = await workFetch(
        `/api/v1${base}/people/payroll/runs/${String(run.id)}/accounting-preview`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
          body: JSON.stringify(input),
        }, completion, "caller",
      );
      const body = (await response.json()) as {
        data?: Preview;
        error?: PayrollApiError;
      };
      if (!response.ok || !body.data)
        throw body.error ?? { message: "Accounting preview is unavailable." };
      if (body.data.totals.debitMinor !== body.data.totals.creditMinor)
        throw {
          code: "PAYROLL_TOTALS_INVALID",
          message: "Journal debits and credits do not balance.",
        };
      if (read !== previewReads.current || !payrollFormUnchanged(completion)) return;
      setPreview({ value: body.data, payload: input, completion });
    } catch (cause) {
      setError(payrollErrorMessage(cause, "Accounting preview is unavailable."));
    } finally {
      setBusy(false);
    }
  }
  async function post() {
    if (preview) await send(`payroll/runs/${String(run.id)}/post`, preview.payload, false, preview.completion);
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="payroll-accounting-heading">
      <h2 id="payroll-accounting-heading">{"Accounting preview"}</h2>
      <p className="muted">{"Accrue at work-period end or adjust in an open period."}</p>
      {approvedMapping ? (
        <div className="stacked-list">
          <p>
            <strong>{"Approved pay-group mapping"}</strong> · version{" "}
            {String(approvedMapping.version)} ·{" "}
            {String(
              options.charts.find((chart) => String(chart.id) === String(approvedMapping.chart_id))
                ?.name ?? approvedMapping.chart_id,
            )}
          </p>
          <p className="muted">
            {
              "An open accounting period and work-period end are selected automatically; closed periods use an open adjustment period."
            }
          </p>
          <Button appearance="standard"
            type="submit"
            variant="primary"
            disabled={busy || !canSettle}
            onClick={() =>
              void showPreview({
                expectedVersion: Number(run.version),
                accountMappingId: String(approvedMapping.id),
              })
            }
          >
            {"Preview journal"}
          </Button>
          <Button appearance="standard"
            variant="secondary"
            type="button"
            disabled={!canSettle}
            onClick={() => {
              setManual(!manual);
              setPreview(null);
            }}
          >
            {manual ? "Close manual mapping" : "Override for this run"}
          </Button>
        </div>
      ) : (
        <p className="notice">
          {
            "Finance must approve a pay-group mapping before recurring payroll can post automatically."
          }
        </p>
      )}
      <PayrollCommandRecovery {...recovery} appearance="standard"/>{(error || commandError) && <InlineNotice tone="danger" role="alert">{error || commandError}</InlineNotice>}{notice && <p role="status">{notice}</p>}
      {canSettle && (!approvedMapping || manual) && (
        <FormDisclosure appearance="standard" drawerTitle="Map this payroll journal" onDiscard={() => {setFormRevision(value => value + 1); setChartId(String(approvedMapping?.chart_id ?? options.charts[0]?.id ?? "")); setPreview(null); previewReads.current++;}}><summary>Map payroll journal</summary><PayrollForm key={formRevision}
          className="ui-form-grid"
          onChange={() => {previewReads.current++; setPreview(null);}}
          onSubmit={(event) => {
            event.preventDefault();
            void showPreview(payload(event.currentTarget));
          }}
        >
          <Field appearance="standard">
            {"Chart"}
            <Select appearance="standard"
              name="chartId"
              value={chartId}
              required
              onChange={(event) => setChartId(event.currentTarget.value)}
            >
              {options.charts.map((chart) => (
                <option value={String(chart.id)} key={String(chart.id)}>
                  {String(chart.name)} · {String(chart.base_currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            {"Open period"}
            <Select appearance="standard" name="periodId" defaultValue={String(period?.id ?? "")} required>
              {options.periods.map((item) => (
                <option value={String(item.id)} key={String(item.id)}>
                  {String(item.name)} · {String(item.start_date)} – {String(item.end_date)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            {"Entry date"}
            <TextInput appearance="standard" name="entryDate" type="date" defaultValue={entryDate} required />
          </Field>
          {mappings.map((mapping) => (
            <Field appearance="standard" key={mapping.name}>
              {mapping.label}
              <Select appearance="standard" name={mapping.name} required={mapping.required} defaultValue="">
                <option value="">{mapping.required ? "Choose account" : "Not used"}</option>
                {accounts
                  .filter(
                    (account) =>
                      String(account.category) === mapping.category &&
                      String(account.currency) === String(run.currency),
                  )
                  .map((account) => (
                    <option value={String(account.id)} key={String(account.id)}>
                      {String(account.code)} · {String(account.name)}
                    </option>
                  ))}
              </Select>
            </Field>
          ))}
          <Button appearance="standard"
            clientManaged type="submit"
            variant="primary"
            disabled={busy || !options.charts.length || !options.periods.length}
          >
            {"Preview journal"}
          </Button>
        </PayrollForm></FormDisclosure>
      )}
      {canSettle && preview && (
        <div className="stacked-list" aria-live="polite">
          <h3>{"Journal lines"}</h3>
          <TableFrame appearance="standard" label="Records" className="business-table-wrap">
            <Table className="business-table">
              <thead>
                <tr>
                  <th>{"Account"}</th>
                  <th>{"Description"}</th>
                  <th>{"Unit"}</th>
                  <th>{"Debit"}</th>
                  <th>{"Credit"}</th>
                </tr>
              </thead>
              <tbody>
                {preview.value.lines.map((line, index) => (
                  <tr key={`${line.accountId}:${index}`}>
                    <td>
                      {String(
                        accounts.find((account) => String(account.id) === line.accountId)?.code ??
                          line.accountId,
                      )}
                    </td>
                    <td>{line.description}</td>
                    <td>
                      {[line.dimensions?.branchId, line.dimensions?.departmentId]
                        .filter(Boolean)
                        .join(" / ") || "—"}
                    </td>
                    <td>{line.debit}</td>
                    <td>{line.credit}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
          <p>
            <strong>
              {"Debit"} {String(run.currency)} {minorToDecimal(preview.value.totals.debitMinor)} ={" "}
              {"Credit"} {String(run.currency)} {minorToDecimal(preview.value.totals.creditMinor)}
            </strong>
          </p>
          <Button appearance="standard" type="submit" variant="primary" disabled={busy} onClick={() => void post()}>
            {"Post reviewed journal"}
          </Button>
        </div>
      )}
      {!canSettle && <p role="status">Payroll posting is available for retained history.</p>}
      <p className="muted">
        {"Review posted evidence in"} <Link href={canonicalUiHref(`${base}/finance`)}>Finance</Link>
      </p>
    </Card>
  );
}
export function PayrollAccountMappingSettings({
  base,
  groups,
  options,
  userId,
}: {
  base: string;
  groups: Row[];
  options: Options;
  userId: string;
}) {
  const {send, busy, error, notice, canPrepare, canSettle, recovery} = usePayrollCommand(base, "account-mappings", JSON.stringify([groups, options]));
  const [groupId, setGroupId] = useState(String(groups[0]?.id ?? ""));
  const [chartId, setChartId] = useState(
    String(
      options.charts.find((chart) => String(chart.base_currency) === String(groups[0]?.currency))
        ?.id ??
        options.charts[0]?.id ??
        "",
    ),
  );
  const accounts = options.accounts.filter(
    (account) =>
      String(account.chart_id) === chartId &&
      String(account.currency) ===
        String(groups.find((group) => String(group.id) === groupId)?.currency),
  );
  const groupMappings = (options.mappings ?? []).filter(
    (mapping) => String(mapping.pay_group_id) === groupId,
  );
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="payroll-mapping-heading">
      <h2 id="payroll-mapping-heading">{"Payroll account mapping"}</h2>
      <p className="muted">
        {"One Finance user drafts the mapping; another approves before recurring use."}
      </p>
      <PayrollCommandRecovery {...recovery} appearance="standard"/>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice && <p role="status">{notice}</p>}
      <Field appearance="standard">
        {"Pay group"}
        <Select appearance="standard"
          value={groupId}
          onChange={(event) => {
            const id = event.currentTarget.value;
            setGroupId(id);
            setChartId(
              String(
                options.charts.find(
                  (chart) =>
                    String(chart.base_currency) ===
                    String(groups.find((group) => String(group.id) === id)?.currency),
                )?.id ?? "",
              ),
            );
          }}
        >
          {groups.map((group) => (
            <option key={String(group.id)} value={String(group.id)}>
              {String(group.name)} · {String(group.currency)}
            </option>
          ))}
        </Select>
      </Field>
      {groupMappings.map((mapping) => (
        <div className="ui-record-item" key={String(mapping.id)}>
          <div className="row-main">
            <strong>
              {String(mapping.status)} · version {String(mapping.version)}
            </strong>
            <span>
              {String(
                options.charts.find((chart) => String(chart.id) === String(mapping.chart_id))
                  ?.name ?? mapping.chart_id,
              )}
            </span>
          </div>
          {canSettle && mapping.status === "draft" && mapping.created_by !== userId && (
            <Button appearance="standard"
              type="submit"
              variant="primary"
              disabled={busy}
              onClick={() =>
                void send(`payroll/account-mappings/${String(mapping.id)}/approve`, {
                  expectedVersion: Number(mapping.version),
                })
              }
            >
              {"Approve mapping"}
            </Button>
          )}
        </div>
      ))}
      {canPrepare && groupId && (
        <PayrollForm
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            const input: Record<string, string> = { chartId: String(data.get("chartId")) };
            for (const mapping of mappings) {
              const value = String(data.get(mapping.name) ?? "");
              if (value) input[mapping.name] = value;
            }
            void send(`payroll/pay-groups/${encodeURIComponent(groupId)}/account-mappings`, input);
          }}
        >
          <Field appearance="standard">
            {"Chart"}
            <Select appearance="standard"
              name="chartId"
              value={chartId}
              onChange={(event) => setChartId(event.currentTarget.value)}
              required
            >
              {options.charts
                .filter(
                  (chart) =>
                    String(chart.base_currency) ===
                    String(groups.find((group) => String(group.id) === groupId)?.currency),
                )
                .map((chart) => (
                  <option key={String(chart.id)} value={String(chart.id)}>
                    {String(chart.name)}
                  </option>
                ))}
            </Select>
          </Field>
          {mappings.map((mapping) => (
            <Field appearance="standard" key={mapping.name}>
              {mapping.label}
              <Select appearance="standard" name={mapping.name} required={mapping.required} defaultValue="">
                <option value="">{mapping.required ? "Choose account" : "Not used"}</option>
                {accounts
                  .filter((account) => String(account.category) === mapping.category)
                  .map((account) => (
                    <option key={String(account.id)} value={String(account.id)}>
                      {String(account.code)} · {String(account.name)}
                    </option>
                  ))}
              </Select>
            </Field>
          ))}
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !options.charts.length}>
            {"Save mapping draft"}
          </Button>
        </PayrollForm>
      )}
    </Card>
  );
}
