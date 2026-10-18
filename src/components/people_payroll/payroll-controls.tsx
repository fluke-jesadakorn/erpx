"use client";
import {PayrollCommandRecovery} from "./payroll-command-recovery";
import { PayrollFormDrawer as Dialog } from "./payroll-form-drawer";
import { PayrollForm } from "./payroll-form";
import {FormDisclosure} from "@/components/ui/form-disclosure";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { MoneyAmount } from "@/components/finance/money/currency-provider";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { RecordDocumentPanel } from "@/components/files/record-document-panel";
import { usePayrollCommand } from "./use-payroll-command";
import { type UiAppearance } from "@/components/ui/foundation";
type Row = Record<string, unknown>;
export function usePeopleCommand(base: string, ownerKey = "", source = "") {
  return usePayrollCommand(base, ownerKey, source);
}
export function PayrollRunEditor({ appearance = "legacy",
  base,
  groups,
  packs = [],
  simulationMode = false,
}: ({
  base: string;
  groups: Row[];
  packs?: Row[];
  simulationMode?: boolean;
  userId?: string;
}) & { appearance?: UiAppearance }) {
  const [open, setOpen] = useState(false);
  const [mode,setMode]=useState("preview");
  const { send, busy, error, notice, canPrepare, recovery} = usePeopleCommand(base, "runs", JSON.stringify([groups, packs, simulationMode]));
  if (!canPrepare) return null;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <Button appearance={appearance} type="submit" variant="primary" onClick={() => setOpen(!open)}>
        {simulationMode ? "Create payroll simulation (mock)" : "Create payroll run"}
      </Button>
      {open && (
        <Dialog appearance={appearance} open={open} onClose={() => setOpen(false)} title="Create payroll run" onDiscard={() => {setMode("preview"); setOpen(false);}} dismissDisabled={busy}><div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-stack" : "content-stack"}>{error && <p data-ui-appearance={appearance} role="alert" className="error">{error}</p>}<PayrollForm data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              group = groups.find((g) => g.id === f.get("payGroupId"));
            void send(
              "payroll/runs",
              {
                payGroupId: f.get("payGroupId"),
                currency: group?.currency,
                periodStart: f.get("periodStart"),
                periodEnd: f.get("periodEnd"),
                payDate: f.get("payDate"),
                mode,
                ...(mode==="production"?{countryPackId:f.get("countryPackId")} : {}),
              },
              true,
            );
          }}
        >
          <Field appearance={appearance}>
            {"Pay group"}
            <Select appearance={appearance} name="payGroupId" required>
              <option data-ui-appearance={appearance} value="">{"Select pay group"}</option>
              {groups.map((g) => (
                <option data-ui-appearance={appearance} key={String(g.id)} value={String(g.id)}>
                  {String(g.name)} · {String(g.currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            {"Calculation"}
            <Select appearance={appearance} name="mode" value={mode} onChange={e=>setMode(e.target.value)}>
              <option data-ui-appearance={appearance} value="preview">Gross pay preview</option>
              {!simulationMode&&packs.some(p=>p.status==="active"&&p.adapter_id==="thai-payroll-v1")&&<option data-ui-appearance={appearance} value="production">Thai statutory rules</option>}
            </Select>
          </Field>
          {mode==="production"&&<Field appearance={appearance}>
            {"Approved rule revision"}
            <Select appearance={appearance} name="countryPackId" required>
              <option data-ui-appearance={appearance} value="">Select effective revision</option>
              {packs.filter(p=>p.status==="active"&&p.adapter_id==="thai-payroll-v1").map(p=><option data-ui-appearance={appearance} key={String(p.id)} value={String(p.id)}>{String(p.version)} · {String(p.effective_from)}–{String(p.effective_to)}</option>)}
            </Select>
          </Field>}
          <Field appearance={appearance}>
            {"Period start"}
            <TextInput appearance={appearance} name="periodStart" type="date" required />
          </Field>
          <Field appearance={appearance}>
            {"Period end"}
            <TextInput appearance={appearance} name="periodEnd" type="date" required />
          </Field>
          <Field appearance={appearance}>
            {"Pay date"}
            <TextInput appearance={appearance} name="payDate" type="date" required />
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
            {"Save payroll run"}
          </Button>
          <p data-ui-appearance={appearance} className="muted">
            {simulationMode
              ? "Simulation only: fixed synthetic values; no Thai statutory calculation or real transfer."
              : mode==="production"?"Requires independently approved employee allowances, opening YTD and dated rules. Bank transfer is handled separately.":"Gross pay preview only. Choose Thai statutory rules after the source revision and employee evidence are approved."}
          </p>
        </PayrollForm></div></Dialog>
      )}
      <PayrollCommandRecovery {...recovery} appearance={appearance}/>{error && <p data-ui-appearance={appearance} role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
    </section>
  );
}
export function PayGroupEditor({ appearance = "legacy", base }: ({ base: string; userId?: string }) & { appearance?: UiAppearance }) {
  const { send, busy, error, notice, canPrepare, recovery} = usePeopleCommand(base, "pay-groups", JSON.stringify([]));
  const [open, setOpen] = useState(false);
  if (!canPrepare) return null;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <Button appearance={appearance} variant="secondary" type="button" onClick={() => setOpen(!open)}>
        {open ? "Close pay group form" : "Configure pay group"}
      </Button>
      <PayrollCommandRecovery {...recovery} appearance={appearance}/>{error && <p data-ui-appearance={appearance} role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
      {open && (
        <Dialog appearance={appearance} open={open} onClose={() => setOpen(false)} title="Create pay group" dismissDisabled={busy}><div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-stack" : "content-stack"}>{error && <p data-ui-appearance={appearance} role="alert" className="error">{error}</p>}<PayrollForm data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={async (event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            if (
              await send("pay-groups", {
                name: form.get("name"),
                countryCode: form.get("countryCode"),
                currency: form.get("currency"),
                frequency: form.get("frequency"),
                timezone: form.get("timezone"),
                overtimeMultiplierBps: Math.round(Number(form.get("overtimeMultiplier")) * 10000),
                prorationMethod: form.get("prorationMethod"),
              })
            )
              setOpen(false);
          }}
        >
          <Field appearance={appearance}>
            {"Name"}
            <TextInput appearance={appearance} name="name" required maxLength={120} />
          </Field>
          <Field appearance={appearance}>
            {"Country code"}
            <TextInput appearance={appearance} name="countryCode" required minLength={2} maxLength={2} defaultValue="TH" />
          </Field>
          <Field appearance={appearance}>
            {"Currency"}
            <TextInput appearance={appearance} name="currency" required minLength={3} maxLength={3} defaultValue="THB" />
          </Field>
          <Field appearance={appearance}>
            {"Frequency"}
            <Select appearance={appearance} name="frequency" defaultValue="monthly">
              <option data-ui-appearance={appearance} value="weekly">{"Weekly"}</option>
              <option data-ui-appearance={appearance} value="fortnightly">{"Fortnightly"}</option>
              <option data-ui-appearance={appearance} value="semimonthly">{"Semimonthly"}</option>
              <option data-ui-appearance={appearance} value="monthly">{"Monthly"}</option>
            </Select>
          </Field>
          <Field appearance={appearance}>
            {"Timezone"}
            <TextInput appearance={appearance} name="timezone" required defaultValue="Asia/Bangkok" />
          </Field>
          <Field appearance={appearance}>
            {"Overtime multiplier"}
            <TextInput appearance={appearance}
              name="overtimeMultiplier"
              type="number"
              min="1"
              max="5"
              step="0.01"
              required
              defaultValue="1.5"
            />
          </Field>
          <Field appearance={appearance}>
            {"Proration"}
            <Select appearance={appearance} name="prorationMethod" defaultValue="calendar_days">
              <option data-ui-appearance={appearance} value="calendar_days">{"Calendar days"}</option>
              <option data-ui-appearance={appearance} value="none">{"Require manual full-period inputs"}</option>
            </Select>
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
            {"Save pay group policy"}
          </Button>
        </PayrollForm></div></Dialog>
      )}
      <p data-ui-appearance={appearance} className="muted">
        {
          "This policy controls core payroll calculations; it does not claim Thai statutory compliance."
        }
      </p>
    </section>
  );
}
export function PayrollActions({ appearance = "legacy",
  base,
  run,
  simulationMode = false,
  canCalculate,
  canApprove,
  canExport,
  canPost,
  postingOptions,
  paymentBatch,
  paymentItems,
  canPreparePayment,
  canApprovePayment,
  canExportPayment,
  canConfirmPayment,
}: ({
  base: string;
  run: Row;
  simulationMode?: boolean;
  canCalculate: boolean;
  canApprove: boolean;
  canExport: boolean;
  canPost: boolean;
  postingOptions: {
    charts: Row[];
    periods: Row[];
    accounts: Row[];
  } | null;
  paymentBatch: Row | null;
  paymentItems: Row[];
  canPreparePayment: boolean;
  canApprovePayment: boolean;
  canExportPayment: boolean;
  canConfirmPayment: boolean;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error, canSettle, canRead, recovery} = usePeopleCommand(base, `run:${run.id}`, JSON.stringify([run, postingOptions, paymentBatch, paymentItems, canCalculate, canApprove, canPost, canExport, canPreparePayment, canApprovePayment, canExportPayment, canConfirmPayment])),
    [chartId, setChartId] = useState(String(postingOptions?.charts[0]?.id ?? "")),
    totals = JSON.parse(String(run.totals_json ?? "{}")) as {
      deductions?: number;
      employer?: number;
    },
    accounts = (postingOptions?.accounts ?? []).filter(
      (account) => String(account.chart_id) === chartId,
    ),
    expenses = accounts.filter((account) => String(account.category) === "expense"),
    liabilities = accounts.filter((account) => String(account.category) === "liability"),
    batch = paymentBatch;
  canCalculate &&= canSettle; canApprove &&= canSettle; canPost &&= canSettle; canExport &&= canRead;
  canPreparePayment &&= canSettle; canApprovePayment &&= canSettle; canExportPayment &&= canSettle; canConfirmPayment &&= canSettle;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"}>
        {canCalculate &&
          ["draft", "calculating", "preview", "failed"].includes(String(run.status)) && (
            <Button appearance={appearance}
              type="submit"
              variant="plain"
              className={run.status === "preview" ? "button secondary" : "button"}
              disabled={busy}
              onClick={() => send(`payroll/runs/${run.id}/calculate`, {})}
            >
              {simulationMode
                ? "Calculate deterministic mock payroll"
                : "Calculate gross-pay preview"}
            </Button>
          )}
        {canApprove && run.status === "in_review" && (
          <Button appearance={appearance}
            type="submit"
            variant="primary"
            disabled={busy}
            onClick={() => send(`payroll/runs/${run.id}/approve`, { expectedVersion: run.version })}
          >
            Approve submitted review
          </Button>
        )}
        {canExport && ["preview", "in_review", "approved"].includes(String(run.status)) && (
          <Button appearance={appearance}
            type="submit"
            variant="secondary"
            disabled={busy}
            onClick={() => send(`payroll/runs/${run.id}/export-review`, {})}
          >
            Export this reviewed revision
          </Button>
        )}
      </div>
      <PayrollCommandRecovery {...recovery} appearance={appearance}/>{error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      {canPost && run.status === "approved" && postingOptions && (
        <PayrollForm data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`payroll/runs/${run.id}/post`, {
              chartId: f.get("chartId"),
              periodId: f.get("periodId"),
              entryDate: f.get("entryDate"),
              expenseAccountId: f.get("expenseAccountId"),
              netPayableAccountId: f.get("netPayableAccountId"),
              taxPayableAccountId: f.get("taxPayableAccountId") || undefined,
              employerContributionAccountId: f.get("employerContributionAccountId") || undefined,
              expectedVersion: Number(run.version),
            });
          }}
        >
          <h2 data-ui-appearance={appearance}>Post approved payroll to Finance</h2>
          <p data-ui-appearance={appearance} className="muted">
            Finance posting creates one balanced, source-linked journal and unlocks a controlled
            payment batch.
          </p>
          <Field appearance={appearance}>
            Chart
            <Select appearance={appearance}
              name="chartId"
              value={chartId}
              onChange={(e) => setChartId(e.target.value)}
              required
            >
              {postingOptions.charts.map((chart) => (
                <option data-ui-appearance={appearance} key={String(chart.id)} value={String(chart.id)}>
                  {String(chart.name)} · {String(chart.base_currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Open accounting period
            <Select appearance={appearance} name="periodId" required>
              <option data-ui-appearance={appearance} value="">Select period</option>
              {postingOptions.periods.map((period) => (
                <option data-ui-appearance={appearance} key={String(period.id)} value={String(period.id)}>
                  {String(period.name)} · {String(period.start_date)} to {String(period.end_date)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Posting date
            <TextInput appearance={appearance} name="entryDate" type="date" defaultValue={String(run.pay_date)} required />
          </Field>
          <Field appearance={appearance}>
            Payroll expense account
            <Select appearance={appearance} name="expenseAccountId" required>
              <option data-ui-appearance={appearance} value="">Select expense account</option>
              {expenses.map((account) => (
                <option data-ui-appearance={appearance} key={String(account.id)} value={String(account.id)}>
                  {String(account.code)} · {String(account.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Net-pay payable account
            <Select appearance={appearance} name="netPayableAccountId" required>
              <option data-ui-appearance={appearance} value="">Select liability account</option>
              {liabilities.map((account) => (
                <option data-ui-appearance={appearance} key={String(account.id)} value={String(account.id)}>
                  {String(account.code)} · {String(account.name)}
                </option>
              ))}
            </Select>
          </Field>
          {Number(totals.deductions ?? 0) > 0 && (
            <Field appearance={appearance}>
              Deductions payable account
              <Select appearance={appearance} name="taxPayableAccountId" required>
                <option data-ui-appearance={appearance} value="">Select liability account</option>
                {liabilities.map((account) => (
                  <option data-ui-appearance={appearance} key={String(account.id)} value={String(account.id)}>
                    {String(account.code)} · {String(account.name)}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          {Number(totals.employer ?? 0) > 0 && (
            <Field appearance={appearance}>
              Employer contribution account
              <Select appearance={appearance} name="employerContributionAccountId" required>
                <option data-ui-appearance={appearance} value="">Select liability account</option>
                {liabilities.map((account) => (
                  <option data-ui-appearance={appearance} key={String(account.id)} value={String(account.id)}>
                    {String(account.code)} · {String(account.name)}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          <Button appearance={appearance}
            clientManaged type="submit"
            variant="primary"
            disabled={
              busy ||
              !postingOptions.charts.length ||
              !postingOptions.periods.length ||
              !expenses.length ||
              !liabilities.length
            }
          >
            Post GL and create payable obligation
          </Button>
        </PayrollForm>
      )}
      {canPost && run.status === "approved" && !postingOptions && (
        <p data-ui-appearance={appearance} className="notice">
          Finance posting needs an active chart, open accounting period, and approved liability
          accounts.
        </p>
      )}
      {canPreparePayment && run.status === "posted" && !batch && (
        <PayrollForm data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send("payments/batches", {
              sourceType: "payroll",
              sourceId: String(run.id),
              formatCode: f.get("formatCode"),
            });
          }}
        >
          <h2 data-ui-appearance={appearance}>Prepare payroll payment batch</h2>
          <p data-ui-appearance={appearance} className="muted">
            Create a payable instruction from posted net pay. An independent Finance or Treasury
            user must approve it before export.
          </p>
          <Field appearance={appearance}>
            Export format
            <Select appearance={appearance} name="formatCode" defaultValue="generic-csv">
              <option data-ui-appearance={appearance} value="generic-csv">Generic CSV sandbox adapter</option>
              <option data-ui-appearance={appearance} value="iso20022">ISO 20022 sandbox adapter</option>
            </Select>
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
            Create payment batch
          </Button>
        </PayrollForm>
      )}
      {batch && (
        <section data-ui-appearance={appearance} className="stacked-list" aria-label="Payroll payment batch">
          <h2 data-ui-appearance={appearance}>Payroll payment batch</h2>
          <p data-ui-appearance={appearance} className="muted">
            {String(batch.status)} ·{" "}
            <MoneyAmount
              units={String(batch.total_minor)}
              currency={String(batch.currency)}
              legacyScale={2}
            />{" "}
            · {String(paymentItems.length)} payees · prepared by {String(batch.created_by)}
          </p>
          <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"}>
            {canApprovePayment && batch.status === "draft" && (
              <Button appearance={appearance}
                type="submit"
                variant="primary"
                disabled={busy}
                onClick={() =>
                  send(`payments/batches/${batch.id}/approve`, {
                    expectedVersion: Number(batch.version),
                  })
                }
              >
                Approve payment batch
              </Button>
            )}
            {canExportPayment && batch.status === "approved" && (
              <Button appearance={appearance}
                type="submit"
                variant="primary"
                disabled={busy}
                onClick={() => send(`payments/batches/${batch.id}/export`, {})}
              >
                Export payment instruction
              </Button>
            )}
            {["exported", "partially_settled", "settled"].includes(String(batch.status)) && (
              <UiActionLink appearance={appearance}
                variant="secondary"
                href={`/api/v1${base}/people/payments/batches/${batch.id}/file`}
              >
                Download payment file
              </UiActionLink>
            )}
          </div>
          {canConfirmPayment &&
            ["exported", "partially_settled"].includes(String(batch.status)) && (
              <PayrollForm data-ui-appearance={appearance}
                className={appearance === "standard" ? "ui-form-grid" : "business-form"}
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget),
                    rows = paymentItems.map((item) => ({
                      reference: String(item.reference),
                      status: String(f.get(`status:${item.reference}`)),
                      confirmationRef: String(f.get(`ref:${item.reference}`) || "") || undefined,
                    }));
                  void send(`payments/batches/${batch.id}/confirm`, { rows });
                }}
              >
                <h3 data-ui-appearance={appearance}>Confirm sandbox bank results</h3>
                {paymentItems.map((item) => (
                  <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-record-item" : "row"} key={String(item.reference)}>
                    <div data-ui-appearance={appearance} className="row-main">
                      <strong data-ui-appearance={appearance}>{String(item.payee_name)}</strong>
                      <span data-ui-appearance={appearance}>
                        {String(item.reference)} ·{" "}
                        <MoneyAmount
                          units={String(item.amount_minor)}
                          currency={String(item.currency)}
                          legacyScale={2}
                        />{" "}
                        · {String(item.status)}
                      </span>
                    </div>
                    <Field appearance={appearance}>
                      Result
                      <Select appearance={appearance}
                        name={`status:${item.reference}`}
                        defaultValue={String(item.status) === "failed" ? "failed" : "confirmed"}
                      >
                        <option data-ui-appearance={appearance} value="confirmed">Confirmed</option>
                        <option data-ui-appearance={appearance} value="failed">Failed</option>
                      </Select>
                    </Field>
                    <Field appearance={appearance}>
                      Bank reference
                      <TextInput appearance={appearance} name={`ref:${item.reference}`} required />
                    </Field>
                  </div>
                ))}
                <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
                  Save payment results
                </Button>
              </PayrollForm>
            )}
        </section>
      )}
      <p data-ui-appearance={appearance} className="muted">
        {simulationMode
          ? "Mock payroll results are synthetic test fixtures only. They do not implement Thai statutory calculation and cannot initiate a real bank transfer."
          : "This core payroll evidence uses versioned inputs, configured proration and overtime. Country-specific statutory calculation and live bank/provider execution remain separately gated."}
      </p>
    </section>
  );
}
export function PayrollCorrectionControls({ appearance = "legacy",
  base,
  run,
  corrections,
  userId,
  canRequest,
  canReview,
}: ({
  base: string;
  run: Row;
  corrections: Row[];
  userId: string;
  canRequest: boolean;
  canReview: boolean;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error, canSettle, recovery} = usePeopleCommand(base, `run:${run.id}:corrections`, JSON.stringify([run, corrections, canRequest, canReview])),
    pending = corrections.find((c) => c.status === "pending"),
    [correctionFiles, setCorrectionFiles] = useState<AttachmentPanelChange | null>(null);
  const [formRevision, setFormRevision] = useState(0);
  canRequest &&= canSettle; canReview &&= canSettle;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"} id="payroll-corrections">
      <h2 data-ui-appearance={appearance}>{"Reviewed corrections"}</h2>
      <p data-ui-appearance={appearance} className="muted">
        {
          "An approved preview remains locked until a different approver authorizes reopening it. Prior exports remain as revision evidence."
        }
      </p>
      <PayrollCommandRecovery {...recovery} appearance={appearance}/>{error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      {canRequest && run.status === "approved" && !pending && (
        <FormDisclosure appearance="standard" drawerTitle="Request controlled payroll correction" onDiscard={() => {setFormRevision(value => value + 1); setCorrectionFiles(null);}}><summary>Request controlled correction</summary><PayrollForm key={formRevision} data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`payroll/runs/${run.id}/corrections`, {
              expectedVersion: Number(run.version),
              reason: f.get("reason"),
              attachmentSetIds:
                correctionFiles?.setId && correctionFiles.readyCount > 0 && correctionFiles.bindable
                  ? [correctionFiles.setId]
                  : [],
            });
          }}
        >
          <Field appearance={appearance}>
            {"Correction reason and source evidence"}
            <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={2000} />
          </Field>
          <AttachmentPanel appearance={appearance}
            base={base}
            targetType="payroll_run"
            targetId={String(run.id)}
            expectedTargetVersion={Number(run.version)}
            componentKind="action"
            componentRef="correction-request"
            purposeKey="correction_support"
            title={"Correction support"}
            description={
              "Attach the controlled source document supporting this correction request."
            }
            visibility={"Restricted to authorized Company Payroll document roles."}
            onChange={setCorrectionFiles}
          />
          <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy}>
            {"Request controlled correction"}
          </Button>
        </PayrollForm></FormDisclosure>
      )}
      {corrections.map((c) => (
        <article data-ui-appearance={appearance} id={`correction-${String(c.id)}`} className={appearance === "standard" ? "ui-record-item" : "row"} key={String(c.id)}>
          <div data-ui-appearance={appearance} className="row-main">
            <strong data-ui-appearance={appearance}>
              {String(c.status)} · {"source revision"} {String(c.run_revision)}
            </strong>
            <span data-ui-appearance={appearance}>{String(c.reason)}</span>
            {Boolean(c.review_reason) && (
              <span data-ui-appearance={appearance}>
                {"Decision"}: {String(c.review_reason)}
              </span>
            )}
          </div>
          {canSettle && c.status === "pending" && c.requested_by === userId && (
            <Button appearance={appearance}
              type="submit"
              variant="secondary"
              disabled={busy}
              onClick={() =>
                send(`payroll/corrections/${c.id}/decide`, {
                  expectedVersion: Number(c.version),
                  action: "cancel",
                  reason: "Correction request withdrawn before review",
                })
              }
            >
              {"Cancel request"}
            </Button>
          )}
          {c.status === "pending" && canReview && c.requested_by !== userId && (
            <PayrollForm data-ui-appearance={appearance}
              className={appearance === "standard" ? "ui-form-grid" : "business-form"}
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send(`payroll/corrections/${c.id}/decide`, {
                  expectedVersion: Number(c.version),
                  action: f.get("action"),
                  reason: f.get("reason"),
                });
              }}
            >
              <Field appearance={appearance}>
                {"Independent decision"}
                <Select appearance={appearance} name="action">
                  <option data-ui-appearance={appearance} value="approve">{"Approve and reopen calculation"}</option>
                  <option data-ui-appearance={appearance} value="reject">{"Reject correction request"}</option>
                </Select>
              </Field>
              <Field appearance={appearance}>
                {"Decision evidence"}
                <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={2000} />
              </Field>
              <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
                {"Record correction decision"}
              </Button>
            </PayrollForm>
          )}
        </article>
      ))}
      {!corrections.length && <p data-ui-appearance={appearance}>{"No reviewed corrections."}</p>}
    </section>
  );
}
export function PayrollAdvanceConsole({ appearance = "legacy",
  base,
  data,
  userId,
  canPrepare,
  canApprove,
}: ({
  base: string;
  data: {
    items: Row[];
    options: Row[];
  };
  userId: string;
  canPrepare: boolean;
  canApprove: boolean;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error, canPrepare: packagePrepare, canSettle, recovery} = usePeopleCommand(base, "advances", JSON.stringify([data, canPrepare, canApprove])),
    items = data.items ?? [],
    options = data.options ?? [];
  const [formRevision, setFormRevision] = useState(0);
  canApprove &&= canSettle;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"} id="payroll-advances">
      <h2 data-ui-appearance={appearance}>{"Payroll advances"}</h2>
      <p data-ui-appearance={appearance} className="muted">
        {
          "Approved advances create scheduled deductions with their reason, reviewer, and evidence retained."
        }
      </p>
      <PayrollCommandRecovery {...recovery} appearance={appearance}/>{error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      {packagePrepare && (canPrepare || options.length === 1) && options.length > 0 && (
        <FormDisclosure appearance="standard" drawerTitle="Request payroll advance" onDiscard={() => setFormRevision(value => value + 1)}><summary>Request payroll advance</summary><PayrollForm key={formRevision} data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send("payroll/advances", {
              engagementId: f.get("engagementId"),
              amount: f.get("amount"),
              installmentCount: Number(f.get("installmentCount")),
              firstDeductionPeriod: f.get("firstDeductionPeriod"),
              currency: f.get("currency"),
              reason: f.get("reason"),
            });
          }}
        >
          <Field appearance={appearance}>
            {"Employee"}
            <Select appearance={appearance} name="engagementId" required>
              {options.map((option) => (
                <option data-ui-appearance={appearance} key={String(option.engagement_id)} value={String(option.engagement_id)}>
                  {String(option.legal_name)} · {String(option.currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            {"Amount"}
            <TextInput appearance={appearance} name="amount" type="number" min="0.01" step="0.01" required />
          </Field>
          <Field appearance={appearance}>
            {"Installments"}
            <TextInput appearance={appearance}
              name="installmentCount"
              type="number"
              min="1"
              max="24"
              defaultValue="1"
              required
            />
          </Field>
          <Field appearance={appearance}>
            {"First deduction period"}
            <TextInput appearance={appearance} name="firstDeductionPeriod" type="date" required />
          </Field>
          <Field appearance={appearance}>
            {"Currency"}
            <Select appearance={appearance} name="currency" required>
              {Array.from(new Set(options.map((option) => String(option.currency)))).map(
                (currency) => (
                  <option data-ui-appearance={appearance} key={currency}>{currency}</option>
                ),
              )}
            </Select>
          </Field>
          <Field appearance={appearance}>
            {"Reason"}
            <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={2000} />
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
            {"Submit payroll advance"}
          </Button>
        </PayrollForm></FormDisclosure>
      )}
      {!options.length && (
        <p data-ui-appearance={appearance} className="muted">{"No active pay-group engagement is available for an advance."}</p>
      )}
      <div data-ui-appearance={appearance} className="stacked-list">
        {items.map((item) => (
          <article data-ui-appearance={appearance} className={appearance === "standard" ? "ui-record-item" : "row"} id={`payroll-advance-${String(item.id)}`} key={String(item.id)}>
            <div data-ui-appearance={appearance} className="row-main">
              <strong data-ui-appearance={appearance}>
                {String(item.legal_name)} ·{" "}
                <MoneyAmount
                  units={String(item.amount_minor)}
                  currency={String(item.currency)}
                  legacyScale={2}
                />
              </strong>
              <span data-ui-appearance={appearance}>
                {String(item.status)} · {String(item.installment_count)} {"installments from"}{" "}
                {String(item.first_deduction_period)}
              </span>
              <span data-ui-appearance={appearance}>{String(item.reason)}</span>
              {Boolean(item.review_reason) && (
                <span data-ui-appearance={appearance}>
                  {"Decision"}: {String(item.review_reason)}
                </span>
              )}
              <RecordDocumentPanel appearance={appearance}
                base={base}
                targetType="payroll_advance"
                targetId={String(item.id)}
                expectedTargetVersion={Number(item.version)}
                editable={canSettle && item.status === "pending" && (canPrepare || item.requested_by === userId)}
                title={"Advance support"}
                purposes={[
                  {
                    key: "advance_support",
                    label: "Payroll advance support",
                    description: "Attach the employee request or reviewed supporting document.",
                    visibility:
                      "Only the employee owner and authorized Payroll document roles can access these files.",
                  },
                ]}
              />
            </div>
            {canSettle && item.status === "pending" && item.requested_by === userId && (
              <Button appearance={appearance}
                type="submit"
                variant="secondary"
                disabled={busy}
                onClick={() =>
                  send(`payroll/advances/${String(item.id)}/decide`, {
                    expectedVersion: Number(item.version),
                    action: "cancel",
                    reason: "Requester withdrew the advance before review",
                  })
                }
              >
                {"Cancel request"}
              </Button>
            )}
            {item.status === "pending" && canApprove && item.requested_by !== userId && (
              <PayrollForm data-ui-appearance={appearance}
                className={appearance === "standard" ? "ui-form-grid" : "business-form"}
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget);
                  void send(`payroll/advances/${String(item.id)}/decide`, {
                    expectedVersion: Number(item.version),
                    action: f.get("action"),
                    reason: f.get("reason"),
                  });
                }}
              >
                <Field appearance={appearance}>
                  {"Decision"}
                  <Select appearance={appearance} name="action">
                    <option data-ui-appearance={appearance} value="approve">{"Approve and schedule deductions"}</option>
                    <option data-ui-appearance={appearance} value="reject">{"Reject request"}</option>
                  </Select>
                </Field>
                <Field appearance={appearance}>
                  {"Decision evidence"}
                  <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={2000} />
                </Field>
                <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
                  {"Record decision"}
                </Button>
              </PayrollForm>
            )}
          </article>
        ))}
      </div>
      {!items.length && <p data-ui-appearance={appearance}>{"No payroll advances yet."}</p>}
    </section>
  );
}
export function CompensationEditor({ appearance = "legacy",
  base,
  person,
  groups,
  revisions,
  canEdit,
  canApprove,
  userId,
}: ({
  base: string;
  person: Row;
  groups: Row[];
  revisions: Row[];
  canEdit: boolean;
  canApprove: boolean;
  userId: string;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error, recovery} = usePeopleCommand(base, `person:${person.id}`, JSON.stringify([person, groups, revisions, canEdit, canApprove]));
  const [open, setOpen] = useState(false);
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Compensation and pay group</h2>
      <PayrollCommandRecovery {...recovery} appearance={appearance}/>{error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      {canEdit && (
        <>
          <PayrollForm data-ui-appearance={appearance}
            className={appearance === "standard" ? "ui-form-grid" : "business-form"}
            onSubmit={(e) => {
              e.preventDefault();
              void send(`engagements/${person.engagement_id}/pay-group`, {
                expectedVersion: person.engagement_version,
                payGroupId: new FormData(e.currentTarget).get("payGroupId") || null,
              });
            }}
          >
            <Field appearance={appearance}>
              Pay group
              <Select appearance={appearance} name="payGroupId" defaultValue={String(person.pay_group_id ?? "")}>
                <option data-ui-appearance={appearance} value="">Not assigned</option>
                {groups
                  .filter((g) => g.currency === person.currency)
                  .map((g) => (
                    <option data-ui-appearance={appearance} key={String(g.id)} value={String(g.id)}>
                      {String(g.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy}>
              Update pay group
            </Button>
          </PayrollForm>
          <Button appearance={appearance} type="submit" variant="secondary" onClick={() => setOpen(!open)}>
            New compensation revision
          </Button>
          {open && (
            <Dialog appearance={appearance} open={open} onClose={() => setOpen(false)} title="New compensation revision" dismissDisabled={busy}><div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-stack" : "content-stack"}>{error && <p data-ui-appearance={appearance} role="alert" className="error">{error}</p>}<PayrollForm data-ui-appearance={appearance}
              className={appearance === "standard" ? "ui-form-grid" : "business-form"}
              onSubmit={async (e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  components = [] as Array<{
                    code: string;
                    label: string;
                    kind: "earning" | "deduction";
                    amount: FormDataEntryValue | null;
                    taxable: boolean;
                    incomeTiming?:"regular";
                    socialSecurityWage?:boolean;
                  }>;
                if (Number(f.get("allowance")) > 0)
                  components.push({
                    code: "ALLOWANCE",
                    label: "Contractual allowance",
                    kind: "earning",
                    amount: f.get("allowance"),
                    taxable: true,
                    incomeTiming:"regular",
                    socialSecurityWage:true,
                  });
                if (Number(f.get("deduction")) > 0)
                  components.push({
                    code: "RECURRING_DEDUCTION",
                    label: "Recurring deduction",
                    kind: "deduction",
                    amount: f.get("deduction"),
                    taxable: false,
                  });
                if (
                  await send("compensation", {
                    engagementId: person.engagement_id,
                    currency: person.currency,
                    effectiveFrom: f.get("effectiveFrom"),
                    effectiveTo: f.get("effectiveTo") || null,
                    payFrequency: f.get("frequency"),
                    baseAmount: f.get("base"),
                    hourlyRate: f.get("hourly"),
                    dailyRate:f.get("daily"),
                    standardDailyHours:Number(f.get("dailyHours")),
                    expectedHours: Number(f.get("hours")),
                    components,
                  })
                )
                  setOpen(false);
              }}
            >
              <Field appearance={appearance}>
                Effective from
                <TextInput appearance={appearance} name="effectiveFrom" type="date" required />
              </Field>
              <Field appearance={appearance}>
                Effective through (optional)
                <TextInput appearance={appearance} name="effectiveTo" type="date" />
              </Field>
              <Field appearance={appearance}>
                Pay frequency
                <Select appearance={appearance} name="frequency" defaultValue="monthly">
                  {["weekly", "fortnightly", "semimonthly", "monthly", "off_cycle"].map((v) => (
                    <option data-ui-appearance={appearance} key={v}>{v}</option>
                  ))}
                </Select>
              </Field>
              <Field appearance={appearance}>
                Base pay ({String(person.currency)})
                <TextInput appearance={appearance}
                  name="base"
                  type="number"
                  min="0"
                  step="0.01"
                  defaultValue="0"
                  required
                />
              </Field>
              <Field appearance={appearance}>
                Hourly rate
                <TextInput appearance={appearance}
                  name="hourly"
                  type="number"
                  min="0"
                  step="0.01"
                  defaultValue="0"
                  required
                />
              </Field>
              <Field appearance={appearance}>
                Daily rate
                <TextInput appearance={appearance} name="daily" type="number" min="0" step="0.01" defaultValue="0" required/>
              </Field>
              <Field appearance={appearance}>
                Regular hours in a paid day
                <TextInput appearance={appearance} name="dailyHours" type="number" min="0.01" max="24" step="0.01" defaultValue="8" required/>
              </Field>
              <Field appearance={appearance}>
                Expected hours
                <TextInput appearance={appearance}
                  name="hours"
                  type="number"
                  min="0"
                  max="744"
                  step="0.01"
                  defaultValue="0"
                  required
                />
              </Field>
              <Field appearance={appearance}>
                Recurring allowance
                <TextInput appearance={appearance}
                  name="allowance"
                  type="number"
                  min="0"
                  step="0.01"
                  defaultValue="0"
                  required
                />
              </Field>
              <Field appearance={appearance}>
                Recurring deduction
                <TextInput appearance={appearance}
                  name="deduction"
                  type="number"
                  min="0"
                  step="0.01"
                  defaultValue="0"
                  required
                />
              </Field>
              <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
                Save compensation draft
              </Button>
            </PayrollForm></div></Dialog>
          )}
        </>
      )}
      {revisions.map((r) => (
        <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-record-item" : "row"} key={String(r.id)}>
          <div data-ui-appearance={appearance} className="row-main">
            <strong data-ui-appearance={appearance}>
              <MoneyAmount
                units={String(r.base_amount_minor)}
                currency={String(r.currency)}
                date={String(r.effective_from)}
                legacyScale={2}
              />{" "}
              · {String(r.pay_frequency)}
            </strong>
            <span data-ui-appearance={appearance}>
              {String(r.effective_from)} → {String(r.effective_to ?? "ongoing")} ·{" "}
              {String(r.status)}
            </span>
          </div>
          {canApprove &&
            r.created_by !== userId &&
            ["draft", "approved"].includes(String(r.status)) && (
              <Button appearance={appearance}
                type="submit"
                variant="secondary"
                disabled={busy}
                onClick={() =>
                  send(`compensation/${r.id}/transition`, {
                    expectedVersion: r.version,
                    status: r.status === "draft" ? "approved" : "superseded",
                  })
                }
              >
                {r.status === "draft" ? "Approve revision" : "Supersede revision"}
              </Button>
            )}
        </div>
      ))}
    </section>
  );
}
