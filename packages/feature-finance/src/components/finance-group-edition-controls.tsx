"use client";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import { useFinanceDraftScope, captureFinanceForm, acknowledgeFinanceForm } from "@erpx/feature-finance/components/finance-draft-boundary";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { useContext, useState } from "react";
import { EmbeddedRecord } from "@erpx/ui/components/record-context";
import { recordCanonicalHref } from "@erpx/feature-records/lib/record-drawer";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import type { z } from "zod";
import type {
  groupReportEditionSchema,
  groupExpansionWorkspace,
} from "@erpx/server-finance/public/finance-group-editions";
import { financeCommandFetch as workFetch } from "@erpx/feature-finance/lib/finance-command-fetch";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { financeReportPeriods } from "@erpx/feature-finance/lib/finance-report-periods";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  Table,
  TableFrame,
  TextInput,
  TextArea,
} from "@erpx/ui/components/foundation";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { RecordLink } from "@erpx/feature-records/components/record-link";
import { FinanceExpansionDecision } from "@erpx/feature-finance/components/finance-expansion-decision";
import { FinanceEvidenceDocuments } from "@erpx/feature-finance/components/finance-evidence-documents";
type Row = Record<string, string | number | null>;
type Data = Awaited<ReturnType<typeof groupExpansionWorkspace>>;
type Definition = z.infer<typeof groupReportEditionSchema>;
function ReportEditor({
  data,
  initial,
  run,
  busy,
}: {
  data: Data;
  initial?: Row;
  run: (path: string, input: unknown) => Promise<void>;
  busy: boolean;
}) {
  const draftScope=useFinanceDraftScope();
  const old = initial
      ? (JSON.parse(String(initial.content_json)) as Definition)
      : null,
    [books, setBooks] = useState<Record<string, string>>(
      Object.fromEntries(
        data.entities.map((e) => [
          String(e.company_id),
          old?.entities.find((x) => x.companyId === e.company_id)?.chartId ??
            "",
        ]),
      ),
    ),
    [start, setStart] = useState(old?.startDate ?? ""),
    [end, setEnd] = useState(old?.endDate ?? ""),
    [currency, setCurrency] = useState(old?.currency ?? "THB"),
    [chosenPeriods,setChosenPeriods]=useState<Record<string,string[]>>(Object.fromEntries(old?.entities.map(e=>[e.companyId,e.periodIds])??[])),
    selected = data.accounts.filter(
      (a) => books[String(a.company_id)] === a.chart_id,
    ),
    currencies = [
      ...new Set(
        data.charts
          .filter((c) => books[String(c.company_id)] === c.id)
          .map((c) => String(c.base_currency)),
      ),
    ].filter((c) => c !== currency);
  const periodsFor=(companyId:string)=>chosenPeriods[companyId]??financeReportPeriods(data.periods.filter(p=>p.company_id===companyId).map(p=>({id:String(p.id),start_date:String(p.start_date),end_date:String(p.end_date),state:String(p.state)})),start,end);
  return (
    <FormLayout data-finance-draft={draftScope}
      className="ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget),
          mappings = selected.map((a) => ({
            accountId: String(a.id),
            group: {
              code: String(f.get(`code-${a.id}`)),
              name: String(f.get(`name-${a.id}`)),
              category: String(a.category),
            },
            fxType: String(f.get(`fx-${a.id}`)),
            historicalRate:
              String(f.get(`historical-${a.id}`) || "") || undefined,
          })),
          acquisition = data.acquisitions.find(
            (a) => a.id === f.get("acquisitionId"),
          ),
          calculation = data.calculations.find(
            (a) => a.id === f.get("calculationId"),
          ),
          bridge = String(f.get("bridge")),
          debit = {
            code: String(f.get("bridgeDebit")),
            name: String(f.get("bridgeDebitName")),
            category: bridge === "equity" ? "asset" : "equity",
          },
          credit = {
            code: String(f.get("bridgeCredit")),
            name: String(f.get("bridgeCreditName")),
            category: bridge === "equity" ? "other_income" : "equity",
          };
        await run("group-editions", {
          startDate: start,
          endDate: end,
          currency,
          parentId: initial?.id,
          parentVersion: initial?.version,
          reason: f.get("reason"),
          entities: data.entities
            .filter((c) => books[String(c.company_id)])
            .map((c) => ({
              companyId: c.company_id,
              chartId: books[String(c.company_id)],
              method: f.get(`method-${c.company_id}`),
              periodIds: periodsFor(String(c.company_id)),
            })),
          mappings,
          rates: currencies.map((c) => ({
            currency: c,
            current: f.get(`current-${c}`),
            average: f.get(`average-${c}`),
          })),
          intercompanyPairIds: f.getAll("pair"),
          acquisitionEliminations: acquisition
            ? [
                {
                  acquisitionId: acquisition.id,
                  investmentAccountId: f.get("investmentAccountId"),
                  goodwill: {
                    code: "goodwill",
                    name: "Goodwill",
                    category: "asset",
                  },
                  nci: {
                    code: "nci",
                    name: "Noncontrolling interests",
                    category: "equity",
                  },
                  bargainPurchase: {
                    code: "bargain_purchase",
                    name: "Bargain purchase",
                    category: "other_income",
                  },
                },
              ]
            : [],
          adjustments: calculation
            ? [
                {
                  calculationRunId: calculation.id,
                  ownershipId: f.get("bridgeOwnershipId"),
                  output:
                    bridge === "equity" ? "equityMethodClosing" : "nciClosing",
                  debit,
                  credit,
                  existingDebitBalance: f.get("bridgeCarrying"),
                },
                ...(bridge !== "equity"
                  ? [
                      {
                        calculationRunId: calculation.id,
                        ownershipId: f.get("bridgeOwnershipId"),
                        output: "nciProfitShare",
                        debit,
                        credit,
                        existingDebitBalance: "0",
                      },
                    ]
                  : []),
              ]
            : [],
        });
      }}
    >
      <Field appearance="standard">
        Reporting start
        <TextInput
          appearance="standard"
          type="date"
          value={start}
          onChange={(e) => {setStart(e.target.value);setChosenPeriods({});}}
          required
        />
      </Field>
      <Field appearance="standard">
        Reporting end
        <TextInput
          appearance="standard"
          type="date"
          value={end}
          onChange={(e) => {setEnd(e.target.value);setChosenPeriods({});}}
          required
        />
      </Field>
      <Field appearance="standard">
        Presentation currency
        <TextInput
          appearance="standard"
          value={currency}
          onChange={(e) => setCurrency(e.target.value.toUpperCase())}
          minLength={3}
          maxLength={3}
          required
        />
      </Field>
      <p className="muted">
        Each Company&apos;s closed periods must cover the exact selected
        interval. Balance-sheet accounts include all prior postings in the
        selected book.
      </p>
      {data.entities.map((e) => (
        <Card appearance="standard" key={String(e.id)}>
          <b>{String(e.reporting_name)}</b>
          <Field appearance="standard">
            Book
            <Select
              appearance="standard"
              value={books[String(e.company_id)]}
              onChange={(x) =>
                setBooks({ ...books, [String(e.company_id)]: x.target.value })
              }
            >
              <option value="">Exclude Company</option>
              {data.charts
                .filter((c) => c.company_id === e.company_id)
                .map((c) => (
                  <option value={String(c.id)} key={String(c.id)}>
                    {String(c.name)} · {String(c.base_currency)}
                  </option>
                ))}
            </Select>
          </Field>
          {books[String(e.company_id)] && (
            <>
              <Field appearance="standard">
                Accounting method
                <Select
                  appearance="standard"
                  name={`method-${e.company_id}`}
                  defaultValue={
                    old?.entities.find((c) => c.companyId === e.company_id)
                      ?.method ?? "full"
                  }
                >
                  <option value="full">Full consolidation</option>
                  <option value="equity">Equity method</option>
                </Select>
              </Field>
              <fieldset><legend>Reviewed closed periods · {String(e.reporting_name)}</legend>
                <p className="muted">Review one continuous set without gaps or overlapping dates. The complete fiscal period is suggested when available.</p>
                {data.periods.filter(p=>p.company_id===e.company_id&&String(p.start_date)>=start&&String(p.end_date)<=end).map(p=><label key={String(p.id)}><input type="checkbox" checked={periodsFor(String(e.company_id)).includes(String(p.id))} onChange={event=>setChosenPeriods({...chosenPeriods,[String(e.company_id)]:event.target.checked?[...periodsFor(String(e.company_id)),String(p.id)]:periodsFor(String(e.company_id)).filter(id=>id!==p.id)})}/>{String(p.name)} · {String(p.start_date)}–{String(p.end_date)}</label>)}
                {!periodsFor(String(e.company_id)).length&&<p>No aligned closed period selected</p>}
              </fieldset>
            </>
          )}
        </Card>
      ))}
      <details>
        <summary>Group account mapping and translation</summary>
        <p>
          Current rates apply to carrying balances; Average rates apply to
          income and expense; Historical rates retain approved acquisition or
          equity values.
        </p>
        {currencies.map((c) => (
          <Card appearance="standard" key={c}>
            <b>
              {c} → {currency}
            </b>
            <Field appearance="standard">
              Current rate
              <TextInput
                appearance="standard"
                name={`current-${c}`}
                defaultValue={
                  old?.rates.find((r) => r.currency === c)?.current ?? ""
                }
                required
              />
            </Field>
            <Field appearance="standard">
              Average rate
              <TextInput
                appearance="standard"
                name={`average-${c}`}
                defaultValue={
                  old?.rates.find((r) => r.currency === c)?.average ?? ""
                }
                required
              />
            </Field>
          </Card>
        ))}
        {selected.map((a) => {
          const m = old?.mappings.find((m) => m.accountId === a.id);
          return (
            <Card appearance="standard" key={String(a.id)}>
              <b>
                {String(a.reporting_name)} · {String(a.code)} · {String(a.name)}
              </b>
              <Field appearance="standard">
                Group account code
                <TextInput
                  appearance="standard"
                  name={`code-${a.id}`}
                  defaultValue={
                    m?.group.code ??
                    `${a.category}_${String(a.code).replace(/[^a-zA-Z0-9._-]/g, "_")}`
                  }
                  required
                />
              </Field>
              <Field appearance="standard">
                Group account name
                <TextInput
                  appearance="standard"
                  name={`name-${a.id}`}
                  defaultValue={m?.group.name ?? String(a.name)}
                  required
                />
              </Field>
              <Field appearance="standard">
                Translation rule
                <Select
                  appearance="standard"
                  name={`fx-${a.id}`}
                  defaultValue={
                    m?.fxType ??
                    ([
                      "revenue",
                      "expense",
                      "other_income",
                      "other_expense",
                    ].includes(String(a.category))
                      ? "average"
                      : a.category === "equity"
                        ? "historical"
                        : "current")
                  }
                >
                  <option value="current">Current</option>
                  <option value="average">Average</option>
                  <option value="historical">Historical</option>
                </Select>
              </Field>
              <Field appearance="standard">
                Historical rate, when required
                <TextInput
                  appearance="standard"
                  name={`historical-${a.id}`}
                  defaultValue={m?.historicalRate ?? ""}
                />
              </Field>
            </Card>
          );
        })}
      </details>
      <details>
        <summary>Confirmed intercompany eliminations</summary>
        {data.pairs
          .filter((p) => p.status === "confirmed")
          .map((p) => (
            <label key={String(p.id)}>
              <input
                type="checkbox"
                name="pair"
                value={String(p.id)}
                defaultChecked={old?.intercompanyPairIds.includes(String(p.id))}
              />
              {String(p.reference)} · {minorToDecimal(Number(p.amount_minor))}{" "}
              {String(p.currency)}
            </label>
          ))}
      </details>
      <details>
        <summary>Acquisition elimination</summary>
        <p>
          The reviewed investment, acquisition-date subsidiary equity, goodwill
          and NCI are eliminated together.
        </p>
        <Field appearance="standard">
          Approved acquisition
          <Select
            appearance="standard"
            name="acquisitionId"
            defaultValue={old?.acquisitionEliminations[0]?.acquisitionId ?? ""}
          >
            <option value="">No acquisition elimination</option>
            {data.acquisitions
              .filter((a) => a.status === "approved")
              .map((a) => (
                <option key={String(a.id)} value={String(a.id)}>
                  {String(a.investee_name)} · {String(a.acquisition_date)} ·{" "}
                  {minorToDecimal(Number(a.consideration_minor))}
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Investor investment account
          <Select
            appearance="standard"
            name="investmentAccountId"
            defaultValue={
              old?.acquisitionEliminations[0]?.investmentAccountId ?? ""
            }
          >
            <option value="">Choose investment asset</option>
            {selected
              .filter((a) => a.category === "asset")
              .map((a) => (
                <option key={String(a.id)} value={String(a.id)}>
                  {String(a.reporting_name)} · {String(a.code)} {String(a.name)}
                </option>
              ))}
          </Select>
        </Field>
      </details>
      <details>
        <summary>NCI or equity-method bridge</summary>
        <Field appearance="standard">
          Approved consolidation calculation
          <Select appearance="standard" name="calculationId">
            <option value="">No calculation bridge</option>
            {data.calculations
              .filter(
                (c) =>
                  JSON.parse(String(c.explanation_json)).engine !==
                  "source-acquisition-v1",
              )
              .map((c) => (
                <option key={String(c.id)} value={String(c.id)}>
                  {String(c.created_at)} · Profit{" "}
                  {String(
                    JSON.parse(String(c.input_json)).subsidiaryProfit ?? "0",
                  )}{" "}
                  · {String(c.currency)}
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Approved ownership interest
          <Select appearance="standard" name="bridgeOwnershipId">
            <option value="">Choose effective ownership</option>
            {data.ownership
              .filter((o) => o.status === "approved" || o.status === "retired")
              .map((o) => (
                <option key={String(o.id)} value={String(o.id)}>
                  {String(o.investee_name)} · {Number(o.ownership_bps) / 100}% ·{" "}
                  {String(o.effective_from)}
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Bridge type
          <Select appearance="standard" name="bridge">
            <option value="nci">NCI closing and profit attribution</option>
            <option value="equity">Equity-method carrying value</option>
          </Select>
        </Field>
        <Field appearance="standard">
          Existing NCI credit or investment debit balance
          <TextInput
            appearance="standard"
            name="bridgeCarrying"
            defaultValue="0.00"
          />
        </Field>
        {[
          ["bridgeDebit", "Debit group code", "parent_retained"],
          [
            "bridgeDebitName",
            "Debit group name",
            "Parent retained equity allocation",
          ],
          ["bridgeCredit", "Credit group code", "nci"],
          ["bridgeCreditName", "Credit group name", "Noncontrolling interests"],
        ].map(([name, label, value]) => (
          <Field appearance="standard" key={name}>
            {label}
            <TextInput appearance="standard" name={name} defaultValue={value} />
          </Field>
        ))}
      </details>
      <Field appearance="standard">
        Preparation reason
        <TextArea appearance="standard" name="reason" minLength={3} required />
      </Field>
      <Button
        appearance="standard"
        clientManaged
        type="submit"
        disabled={busy || Object.values(books).filter(Boolean).length < 2}
      >
        Prepare retained group edition
      </Button>
    </FormLayout>
  );
}
function OwnershipEditor({
  data,
  initial,
  run,
  busy,
}: {
  data: Data;
  initial?: Row;
  run: (path: string, input: unknown) => Promise<void>;
  busy: boolean;
}) {
  const draftScope=useFinanceDraftScope();
  const old = initial?.source_json
    ? (JSON.parse(String(initial.source_json)).input as Record<
        string,
        string | number
      >)
    : null;
  return (
    <FormLayout data-finance-draft={draftScope}
      className="ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        await run("ownership-editions", {
          ...Object.fromEntries(f),
          ownershipBps: Math.round(Number(f.get("ownershipBps")) * 100),
          votingBps: Math.round(Number(f.get("votingBps")) * 100),
          effectiveTo: f.get("effectiveTo") || undefined,
          parentId: initial?.id,
          parentVersion: initial?.version,
        });
      }}
    >
      {["investorCompanyId", "investeeCompanyId"].map((n) => (
        <Field appearance="standard" key={n}>
          {n === "investorCompanyId" ? "Investor Company" : "Investee Company"}
          <Select
            appearance="standard"
            name={n}
            defaultValue={String(old?.[n] ?? "")}
            required
          >
            <option value="">Choose Company</option>
            {data.entities.map((c) => (
              <option key={String(c.id)} value={String(c.company_id)}>
                {String(c.reporting_name)}
              </option>
            ))}
          </Select>
        </Field>
      ))}
      <Field appearance="standard">
        Accounting method
        <Select
          appearance="standard"
          name="method"
          defaultValue={String(old?.method ?? "subsidiary")}
        >
          <option value="subsidiary">Subsidiary</option>
          <option value="equity">Equity method</option>
          <option value="cost">Cost</option>
          <option value="excluded">Excluded</option>
        </Select>
      </Field>
      {[
        ["ownershipBps", "Ownership (%)"],
        ["votingBps", "Voting rights (%)"],
      ].map(([n, l]) => (
        <Field appearance="standard" key={n}>
          {l}
          <TextInput
            appearance="standard"
            type="number"
            name={n}
            defaultValue={
              old?.[n] === undefined ? "" : String(Number(old[n]) / 100)
            }
            min={0}
            max={100}
            step="0.01"
            required
          />
        </Field>
      ))}
      {initial && (
        <p>
          The earlier interest remains valid through the day before this
          revision starts. Choose a later effective date and record the reason
          for the change.
        </p>
      )}
      <Field appearance="standard">
        Effective from
        <TextInput
          appearance="standard"
          type="date"
          name="effectiveFrom"
          required
        />
      </Field>
      <Field appearance="standard">
        Effective to, if fixed
        <TextInput
          appearance="standard"
          type="date"
          name="effectiveTo"
          defaultValue={String(old?.effectiveTo ?? "")}
        />
      </Field>
      <Field appearance="standard">
        Control assessment
        <TextArea
          appearance="standard"
          name="controlAssessment"
          defaultValue={String(old?.controlAssessment ?? "")}
          required
          minLength={3}
        />
      </Field>
      <Field appearance="standard">
        Reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" clientManaged type="submit" disabled={busy}>
        {initial
          ? "Prepare dated ownership revision"
          : "Prepare ownership draft"}
      </Button>
    </FormLayout>
  );
}

function AcquisitionEditor({
  data,
  initial,
  run,
  busy,
}: {
  data: Data;
  initial?: Row;
  run: (path: string, input: unknown) => Promise<void>;
  busy: boolean;
}) {
  const draftScope=useFinanceDraftScope();
  const old = initial?.source_json
    ? (JSON.parse(String(initial.source_json)).input as Record<
        string,
        string | number
      >)
    : null;
  return (
    <FormLayout data-finance-draft={draftScope}
      className="ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget),
          o = data.ownership.find((o) => o.id === f.get("ownershipId"));
        await run("acquisition-editions", {
          ...Object.fromEntries(f),
          ownershipVersion: o?.version,
          parentId: initial?.id,
          parentVersion: initial?.version,
        });
      }}
    >
      <Field appearance="standard">
        Approved subsidiary ownership
        <Select
          appearance="standard"
          name="ownershipId"
          defaultValue={String(old?.ownershipId ?? "")}
          required
        >
          <option value="">Choose ownership</option>
          {data.ownership
            .filter(
              (o) =>
                ["approved", "retired"].includes(String(o.status)) &&
                o.accounting_method === "subsidiary",
            )
            .map((o) => (
              <option key={String(o.id)} value={String(o.id)}>
                {String(o.investee_name)} · {Number(o.ownership_bps) / 100}%
              </option>
            ))}
        </Select>
      </Field>
      <Field appearance="standard">
        Acquisition date
        <TextInput
          appearance="standard"
          type="date"
          name="acquisitionDate"
          defaultValue={String(old?.acquisitionDate ?? "")}
          required
        />
      </Field>
      {[
        ["currency", "Presentation currency", "THB"],
        ["consideration", "Consideration", "0.00"],
        ["previousInterest", "Previous interest fair value", "0.00"],
        [
          "identifiableNetAssets",
          "Identifiable net assets before deferred tax",
          "0.00",
        ],
        ["deferredTax", "Deferred tax", "0.00"],
        ["nciFairValue", "NCI fair value, if selected", "0.00"],
        ["cguReference", "Cash-generating unit reference", ""],
      ].map(([n, l, v]) => (
        <Field appearance="standard" key={n}>
          {l}
          <TextInput
            appearance="standard"
            name={n}
            defaultValue={String(old?.[n] ?? v)}
            required
          />
        </Field>
      ))}
      <Field appearance="standard">
        NCI measurement
        <Select
          appearance="standard"
          name="nciMeasurement"
          defaultValue={String(old?.nciMeasurement ?? "proportionate")}
        >
          <option value="proportionate">Proportionate net assets</option>
          <option value="fair_value">Reviewed fair value</option>
        </Select>
      </Field>
      <Field appearance="standard">
        Reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" clientManaged type="submit" disabled={busy}>
        {initial
          ? "Prepare corrected acquisition draft"
          : "Prepare acquisition draft"}
      </Button>
    </FormLayout>
  );
}

export function FinanceGroupEditionControls({
  base,
  data,
  enabled,
  canWrite,
  canApprove,
  userId,
}: {
  base: string;
  data: Data;
  enabled: boolean;
  canWrite: boolean;
  canApprove: boolean;
  userId: string;
}) {
  const draftScope=useFinanceDraftScope();
  const [hasAttempt,setHasAttempt]=useState(false);
  const recordOnly = useContext(EmbeddedRecord),
    router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    api = `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance`;
  const canPrepare = useFinanceActionAllowed("new-work","finance.advanced"), canSettle = useFinanceActionAllowed("settle-existing","finance.advanced");
  async function run(path: string, input: unknown) {
    const completion=captureFinanceForm(),url=`${api}/${path}`,payload=JSON.stringify(input);
    if (!(["ownership-editions","acquisition-editions"].includes(path) ? canPrepare : canSettle) && !workFetch.hasAttempt(url,payload)) { setError("This group action is unavailable for the current package."); return; }

    setHasAttempt(true);
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(url, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": crypto.randomUUID(),
          },
          body: payload,
        }),
        b = await r.json();
      if (!r.ok)
        throw new Error(b.error?.message ?? "Group Finance command failed.");
      if (b.data === undefined) throw new Error("Group acknowledgement is missing. Retry the same action.");
      acknowledgeFinanceForm(completion);setHasAttempt(false);
      window.dispatchEvent(new Event("erpx:work-changed"));
      router.refresh();
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Group Finance command failed.",
      );
    } finally {
      setBusy(false);
    }
  }
  const decision = (row: Row, root: string) => (
    <FinanceExpansionDecision
      base={base}
      canWrite={canWrite}
      row={{ ...row, prepared_by: row.prepared_by ?? row.created_by }}
      root={root}
      approveAction="approve"
      canApprove={canApprove}
      userId={userId}
      busy={busy || (!canSettle && !hasAttempt)}
      run={run}
    />
  );
  return (
    <>
      <Card appearance="standard">
        <h2>Retained group reporting</h2>
        <p>
          Review explicit Company books, aligned closed periods, translated
          balances and source-bound group adjustments.
        </p>
        {!enabled && (
          <p className="muted">
            This Company&apos;s group tool is off. Enable it after acceptance
            checks in Finance setup.
          </p>
        )}
        {error && <InlineNotice tone="danger">{error}</InlineNotice>}
        {!recordOnly && enabled && canWrite && (
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Prepare group reporting edition"
          >
            <summary>Prepare group edition</summary>
            <ReportEditor data={data} run={run} busy={busy || (!canSettle && !hasAttempt)} />
          </FormDisclosure>
        )}
      </Card>
      {!recordOnly && enabled && canWrite && (
        <>
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Prepare ownership evidence"
          >
            <summary>Ownership and control</summary>
            <OwnershipEditor data={data} run={run} busy={busy || (!canPrepare && !hasAttempt)} />
          </FormDisclosure>
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Prepare acquisition evidence"
          >
            <summary>Acquisition and goodwill</summary>
            <AcquisitionEditor data={data} run={run} busy={busy || (!canPrepare && !hasAttempt)} />
          </FormDisclosure>
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Pair intercompany posted documents"
          >
            <summary>Pair original documents</summary>
            <FormLayout data-finance-draft={draftScope}
              className="ui-form-grid"
              onSubmit={async (e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  source = data.documents[Number(f.get("source"))],
                  target = data.documents[Number(f.get("target"))];
                await run("intercompany-pairs", {
                  reference: f.get("reference"),
                  reason: f.get("reason"),
                  currency: source?.currency,
                  amount: minorToDecimal(
                    Math.abs(Number(source?.amount_minor)),
                  ),
                  source: {
                    companyId: source?.company_id,
                    journalId: source?.journal_id,
                    accountId: source?.account_id,
                  },
                  target: {
                    companyId: target?.company_id,
                    journalId: target?.journal_id,
                    accountId: target?.account_id,
                  },
                });
              }}
            >
              {["source", "target"].map((n) => (
                <Field appearance="standard" key={n}>
                  {n === "source"
                    ? "First posted document account"
                    : "Opposite Company document account"}
                  <Select appearance="standard" name={n} required>
                    <option value="">Choose posted document account</option>
                    {data.documents.map((d, i) => (
                      <option key={`${d.journal_id}-${d.account_id}`} value={i}>
                        {String(d.reporting_name)} · {String(d.entry_date)} ·{" "}
                        {String(d.code)} ·{" "}
                        {minorToDecimal(Number(d.amount_minor))}{" "}
                        {String(d.currency)} · {String(d.memo ?? d.source_type)}
                      </option>
                    ))}
                  </Select>
                </Field>
              ))}
              <Field appearance="standard">
                Pair reference
                <TextInput
                  appearance="standard"
                  name="reference"
                  required
                  minLength={2}
                />
              </Field>
              <Field appearance="standard">
                Reason
                <TextArea
                  appearance="standard"
                  name="reason"
                  required
                  minLength={3}
                />
              </Field>
              <Button
                appearance="standard"
                clientManaged
                type="submit"
                disabled={busy || (!canSettle && !hasAttempt)}
              >
                Prepare document pair
              </Button>
            </FormLayout>
          </FormDisclosure>
        </>
      )}
      {data.ownership.map((o) => (
        <Card appearance="standard" key={String(o.id)}>
          <h3>
            <RecordLink
              href={recordCanonicalHref(base, {
                kind: "finance_group_ownership",
                id: String(o.id),
              })}
            >
              {String(o.investor_name)} → {String(o.investee_name)}
            </RecordLink>{" "}
            · {String(o.status)}
          </h3>
          <p>
            {Number(o.ownership_bps) / 100}% ownership ·{" "}
            {String(o.accounting_method)} · {String(o.effective_from)} to{" "}
            {String(o.effective_to ?? "ongoing")}
          </p>
          <p>{String(o.control_assessment)}</p>
          {enabled &&
            canWrite &&
            !recordOnly &&
            o.status === "approved" &&
            o.source_hash && (
              <FormDisclosure error={error}
                appearance="standard"
                drawerTitle="Prepare dated ownership revision"
              >
                <summary>Revise ownership from a later date</summary>
                <OwnershipEditor
                  data={data}
                  initial={o}
                  run={run}
                  busy={busy || (!canSettle && !hasAttempt)}
                />
              </FormDisclosure>
            )}
          {decision(o, "ownership-editions")}
        </Card>
      ))}
      {data.acquisitions.map((a) => (
        <Card appearance="standard" key={String(a.id)}>
          <h3>
            <RecordLink
              href={recordCanonicalHref(base, {
                kind: "finance_group_acquisition",
                id: String(a.id),
              })}
            >
              {String(a.investee_name)} acquisition
            </RecordLink>{" "}
            · {String(a.status)}
          </h3>
          <p>
            {String(a.acquisition_date)} · Consideration{" "}
            {minorToDecimal(Number(a.consideration_minor))} · Goodwill{" "}
            {minorToDecimal(Number(a.goodwill_minor))} · Bargain purchase{" "}
            {minorToDecimal(Number(a.bargain_purchase_minor))}{" "}
            {String(a.currency)}
          </p>
          <p>
            {String(a.reason ?? "")}
            {a.parent_id ? " · Corrects a retained acquisition edition" : ""}
          </p>
          {enabled &&
            canWrite &&
            !recordOnly &&
            a.status === "approved" &&
            a.source_hash && (
              <FormDisclosure error={error}
                appearance="standard"
                drawerTitle="Correct acquisition measurement"
              >
                <summary>Correct acquisition measurement</summary>
                <AcquisitionEditor
                  data={data}
                  initial={a}
                  run={run}
                  busy={busy || (!canSettle && !hasAttempt)}
                />
              </FormDisclosure>
            )}
          {decision(a, "acquisition-editions")}
        </Card>
      ))}
      {data.pairs.map((p) => (
        <Card appearance="standard" key={String(p.id)}>
          <h3>
            <RecordLink
              href={recordCanonicalHref(base, {
                kind: "finance_group_pair",
                id: String(p.id),
              })}
            >
              {String(p.reference)}
            </RecordLink>{" "}
            · {String(p.status)}
          </h3>
          <p>
            {minorToDecimal(Number(p.amount_minor))} {String(p.currency)}
          </p>
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Retained paired documents"
          >
            <summary>Review both Company documents</summary>
            {(
              JSON.parse(String(p.source_json)).evidence as {
                entity: Row;
                journal: Row;
                lines: Row[];
              }[]
            ).map((s, i) => (
              <section key={i}>
                <h3>
                  {String(s.entity.reporting_name)} ·{" "}
                  {String(s.journal.entry_date)}
                </h3>
                <p>
                  {String(s.journal.memo ?? "")} ·{" "}
                  {String(s.journal.source_type ?? "manual")} ·{" "}
                  {String(s.journal.source_id ?? s.journal.id)}
                </p>
                <TableFrame appearance="standard" label="Intercompany source lines"><Table>
                  <thead>
                    <tr>
                      <th>Account</th>
                      <th>Debit {String(s.journal.currency)}</th>
                      <th>Credit</th>
                    </tr>
                  </thead>
                  <tbody>
                    {s.lines.map((l) => (
                      <tr key={String(l.id)}>
                        <td>{String(l.code)}</td>
                        <td>{minorToDecimal(Number(l.debit_minor))}</td>
                        <td>{minorToDecimal(Number(l.credit_minor))}</td>
                      </tr>
                    ))}
                  </tbody>
                </Table></TableFrame>
              </section>
            ))}
          </FormDisclosure>
          <FinanceEvidenceDocuments
            base={base}
            row={p}
            targetType="finance_intercompany_pair"
            canWrite={canWrite}
          />
          {canApprove &&
            p.prepared_by !== userId &&
            ["draft", "confirmed"].includes(String(p.status)) && (
              <FormLayout data-finance-draft={draftScope}
                onSubmit={async (e) => {
                  e.preventDefault();
                  const f = new FormData(
                    e.currentTarget,
                    (e.nativeEvent as SubmitEvent).submitter,
                  );
                  await run(`intercompany-pairs/${p.id}/${f.get("action")}`, {
                    expectedVersion: p.version,
                    reason: f.get("reason"),
                  });
                }}
              >
                <Field appearance="standard">
                  Review reason
                  <TextArea
                    appearance="standard"
                    name="reason"
                    required
                    minLength={3}
                  />
                </Field>
                {(p.status === "draft"
                  ? ["confirm", "reject"]
                  : ["reverse"]
                ).map((a) => (
                  <Button
                    appearance="standard"
                    clientManaged
                    type="submit"
                    name="action"
                    value={a}
                    key={a}
                    disabled={busy || (!canSettle && !hasAttempt)}
                  >
                    {a} document pair
                  </Button>
                ))}
              </FormLayout>
            )}
        </Card>
      ))}
      {data.editions.map((e) => {
        const r = JSON.parse(String(e.result_json)) as {
            balanceSheet: { code: string; name: string; amount: string }[];
            profitAndLoss: { code: string; name: string; amount: string }[];
            profit: string;
            profitAttribution: { parent: string; nci: string };
          },
          proof = JSON.parse(String(e.source_json)) as {
            entities: {
              registration: { reporting_name: string; chart_name: string };
              periods: { name: string }[];
              lines: Row[];
            }[];
          };
        return (
          <Card appearance="standard" key={String(e.id)}>
            <h3>
              <RecordLink
                href={recordCanonicalHref(base, {
                  kind: "finance_group_edition",
                  id: String(e.id),
                })}
              >
                {String(e.start_date)} → {String(e.end_date)} · Edition{" "}
                {String(e.edition)}
              </RecordLink>{" "}
              · {String(e.status)}
            </h3>
            <p>
              Group profit {r.profit} {String(e.currency)} · Parent{" "}
              {r.profitAttribution.parent} · NCI {r.profitAttribution.nci}
            </p>
            {[
              ["Balance sheet", r.balanceSheet],
              ["Profit and loss", r.profitAndLoss],
            ].map(([label, rows]) => (
              <details key={String(label)}>
                <summary>{String(label)}</summary>
                <TableFrame appearance="standard" label={String(label)}><Table>
                  <thead>
                    <tr>
                      <th>Group account</th>
                      <th>Amount {String(e.currency)}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(
                      rows as { code: string; name: string; amount: string }[]
                    ).map((a) => (
                      <tr key={a.code}>
                        <td>
                          <RecordLink
                            href={recordCanonicalHref(base, {
                              kind: "finance_group_sources",
                              id: String(e.id),
                              args: { accountCode: a.code },
                            })}
                          >
                            {a.code} · {a.name}
                          </RecordLink>
                        </td>
                        <td>{a.amount}</td>
                      </tr>
                    ))}
                  </tbody>
                </Table></TableFrame>
              </details>
            ))}
            <details>
              <summary>Retained Company source evidence</summary>
              {proof.entities.map((p, i) => (
                <section key={i}>
                  <b>
                    {p.registration.reporting_name} ·{" "}
                    {p.registration.chart_name}
                  </b>
                  <p>{p.periods.map((p) => p.name).join(", ")}</p>
                  {[...new Set(p.lines.map((l) => String(l.journal_id)))]
                    .slice(0, 100)
                    .map((id) => (
                      <p key={id}>
                        <RecordLink
                          href={recordCanonicalHref(base, {
                            kind: "finance_group_source",
                            id,
                            args: { editionId: String(e.id) },
                          })}
                        >
                          Source journal {id}
                        </RecordLink>
                      </p>
                    ))}
                </section>
              ))}
            </details>
            {decision(e, "group-editions")}
            {["approved", "superseded"].includes(String(e.status)) && (
              <a href={`${api}/group-editions/${e.id}/export`}>
                Download retained CSV
              </a>
            )}
            {enabled && canWrite && e.status === "approved" && (
              <FormDisclosure error={error}
                appearance="standard"
                drawerTitle="Revise retained group edition"
              >
                <summary>Prepare revised edition</summary>
                <ReportEditor data={data} initial={e} run={run} busy={busy || (!canSettle && !hasAttempt)} />
              </FormDisclosure>
            )}
          </Card>
        );
      })}
    </>
  );
}
