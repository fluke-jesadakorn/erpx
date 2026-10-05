"use client";
import {FinanceEvidenceDocuments} from "./finance-evidence-documents";
import {EmbeddedRecord} from "@/components/records/record-context";
import {RecordLink} from "@/components/records/record-link";
import {recordCanonicalHref} from "@/lib/record-drawer";
import { useContext,useState } from "react";
import { useRouter } from "next/navigation";
import { financeCommandFetch as workFetch } from "@/lib/finance-command-fetch";
import { financeCsvRows } from "@/lib/finance-csv";
import { minorToDecimal } from "@/lib/finance-money";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  TextArea,
  TextInput,
  Table,
  TableFrame,
} from "@/components/ui/foundation";
import { FormDisclosure } from "@/components/ui/form-disclosure";
type Row = Record<string, unknown>;
type Account = { id: string; code: string; name: string; category: string };
type EditorRow = {
  key: string;
  label: string;
  kind: "accounts" | "sum" | "difference" | "ratio";
  accounts: string;
  references: string;
  basis: "period" | "cumulative";
  sign: "debit" | "credit";
};
const blank = (): EditorRow => ({
  key: "",
  label: "",
  kind: "accounts",
  accounts: "",
  references: "",
  basis: "period",
  sign: "debit",
});
const paths = {
  budget: "budget-revisions",
  forecast: "forecasts",
  report: "report-definitions",
};
export function FinancePlanningControls({
  base,
  chartId,
  data,
  canWrite,
  canApprove,
  userId,
}: {
  base: string;
  chartId: string;
  data: { accounts: Account[]; editions: Row[];snapshots:Row[] };
  canWrite: boolean;
  canApprove: boolean;
  userId: string;
}) {
  const recordOnly=useContext(EmbeddedRecord);
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [output, setOutput] = useState<{ rows: Row[]; snapshotId?: string;planId?:string;cashFlow?:Record<string,string>|null } | null>(
      null,
    ),
    [editor, setEditor] = useState<EditorRow[]>([blank()]);
  const api = `${base.replace("/holdings/", "/api/v1/holdings/")}/finance`;
  async function run(path: string, input: unknown) {
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`${api}/${path}`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": crypto.randomUUID(),
          },
          body: JSON.stringify(input),
        }),
        body = await r.json();
      if (!r.ok)
        throw new Error(body.error?.message ?? "Finance action failed");
      if (body.data.rows)
        setOutput({ rows: body.data.rows, snapshotId: body.data.id,cashFlow:body.data.cashFlow });
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Finance action failed");
    } finally {
      setBusy(false);
    }
  }
  async function actuals(id: string) {
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`${api}/budget-revisions/${id}/actuals`),
        body = await r.json();
      if (!r.ok) throw new Error(body.error?.message ?? "Report unavailable");
      setOutput({ rows: body.data.lines,planId:id });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Report unavailable");
    } finally {
      setBusy(false);
    }
  }
  const parentFields = (kind: string) => (
    <>
      <Field appearance="standard">
        Name
        <TextInput appearance="standard" name="name" required />
      </Field>
      <Field appearance="standard">
        Amend approved edition
        <Select appearance="standard" name="parentId">
          <option value="">New plan</option>
          {data.editions
            .filter(
              (r) =>
                r.kind === kind &&
                r.status === "approved" &&
                !data.editions.some(
                  (child) =>
                    child.parent_id === r.id && child.status !== "rejected",
                ),
            )
            .map((r) => (
              <option key={String(r.id)} value={String(r.id)}>
                {String(r.name)} · {String(r.edition)}
              </option>
            ))}
        </Select>
      </Field>
    </>
  );
  const amendment = (f: FormData) => {
    const p = data.editions.find((r) => r.id === f.get("parentId"));
    return p
      ? {
          parentId: String(p.id),
          expectedVersion: Number(p.version),
          name: String(p.name),
        }
      : { name: String(f.get("name")) };
  };
  function change(index: number, key: keyof EditorRow, value: string) {
    setEditor((rows) =>
      rows.map((r, i) => (i === index ? { ...r, [key]: value } : r)),
    );
  }
  return (
    <Card appearance="standard" as="section" className="ui-panel">
      <h2>Budgets, forecasts and saved reports</h2>
      <p className="muted">
        Prepare an edition and have another person approve it. Amendments retain
        earlier approved amounts and report definitions.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert">
          {error}
        </InlineNotice>
      )}
      {canWrite && (
        (!recordOnly && <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Prepare a budget edition"
        >
          <summary>Prepare budget</summary>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void run("budget-revisions/import", {
                chartId,
                ...amendment(f),
                year: Number(f.get("year")),
                csv: f.get("csv"),
              });
            }}
          >
            {parentFields("budget")}
            <Field appearance="standard">
              Year
              <TextInput
                appearance="standard"
                name="year"
                type="number"
                min={2000}
                max={2199}
                defaultValue={2026}
                required
              />
            </Field>
            <Field appearance="standard">
              CSV: account, month (YYYY-MM), amount; optional dimension:code columns
              <TextArea
                appearance="standard"
                name="csv"
                rows={8}
                placeholder={"account,month,amount\n5000,2026-09,10000.00"}
                required
              />
            </Field>
            <Field appearance="standard">
              Load CSV file
              <TextInput
                appearance="standard"
                type="file"
                accept=".csv,text/csv"
                onChange={async (e) => {
                  const file = e.target.files?.[0],
                    form = e.currentTarget.form;
                  if (file && form) {
                    const text = await file.text();
                    const area = form.elements.namedItem(
                      "csv",
                    ) as HTMLTextAreaElement;
                    area.value = text;
                    area.dispatchEvent(new Event("change", { bubbles: true }));
                  }
                }}
              />
            </Field>
            <Button
              appearance="standard"
              clientManaged
              type="submit"
              disabled={busy}
            >
              Save draft budget
            </Button>
          </FormLayout>
        </FormDisclosure>)
      )}
      {canWrite && (
        (!recordOnly && <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Prepare a twelve-month forecast"
        >
          <summary>Prepare forecast</summary>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              try {
                const rows = financeCsvRows(String(f.get("csv")));
                void run("forecasts", {
                  chartId,
                  ...amendment(f),
                  startMonth: f.get("startMonth"),
                  assumptions: f.get("assumptions"),
                  lines: rows.map((r) => ({
                    accountId:
                      data.accounts.find((a) => a.code === r.account)?.id ?? "",
                    monthlyAmount: r.amount,
                    monthlyGrowthBps: Number(r.growth_bps ?? 0),
                    goodBps: Number(r.good_bps ?? 1000),
                    badBps: Number(r.bad_bps ?? -1000),
                  })),
                });
              } catch (e) {
                setError(e instanceof Error ? e.message : "Invalid CSV");
              }
            }}
          >
            {parentFields("forecast")}
            <Field appearance="standard">
              Start month
              <TextInput
                appearance="standard"
                name="startMonth"
                type="month"
                required
              />
            </Field>
            <Field appearance="standard">
              Assumptions
              <TextArea
                appearance="standard"
                name="assumptions"
                required
                minLength={3}
              />
            </Field>
            <Field appearance="standard">
              CSV: account, amount, growth_bps, good_bps, bad_bps
              <TextArea
                appearance="standard"
                name="csv"
                required
                rows={6}
                placeholder={
                  "account,amount,growth_bps,good_bps,bad_bps\n4000,10000.00,0,1000,-1000"
                }
              />
            </Field>
            <Button
              appearance="standard"
              clientManaged
              type="submit"
              disabled={busy}
            >
              Save draft forecast
            </Button>
          </FormLayout>
        </FormDisclosure>)
      )}
      {canWrite && (
        (!recordOnly && <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Build a financial report"
        >
          <summary>Build report</summary>
          <FormLayout
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              let cashFlow;
              if(f.get("bankCodes")){
                try { cashFlow={bankAccountIds:String(f.get("bankCodes")).split(",").map(code=>data.accounts.find(a=>a.code===code.trim())?.id??""),classifications:financeCsvRows(String(f.get("flowCsv"))).map(r=>({accountId:data.accounts.find(a=>a.code===r.account)?.id??"",kind:r.kind}))}; }
                catch(e){setError(e instanceof Error?e.message:"Invalid cash-flow classifications");return;}
              }
              void run("report-definitions", {
                chartId,
                ...amendment(f),
                cashFlow,
                rows: editor.map((r) =>
                  r.kind === "accounts"
                    ? {
                        key: r.key,
                        label: r.label,
                        kind: r.kind,
                        accountIds: r.accounts
                          .split(",")
                          .map(
                            (s) =>
                              data.accounts.find((a) => a.code === s.trim())
                                ?.id ?? "",
                          ),
                        basis: r.basis,
                        sign: r.sign,
                      }
                    : {
                        key: r.key,
                        label: r.label,
                        kind: r.kind,
                        references: r.references
                          .split(",")
                          .map((s) => s.trim()),
                      },
                ),
              });
            }}
          >
            {parentFields("report")}
            {editor.map((r, i) => (
              <fieldset className="ui-form-grid" key={i}>
                <legend>Row {i + 1}</legend>
                <Field appearance="standard">
                  Key
                  <TextInput
                    appearance="standard"
                    value={r.key}
                    onChange={(e) => change(i, "key", e.target.value)}
                    required
                  />
                </Field>
                <Field appearance="standard">
                  Label
                  <TextInput
                    appearance="standard"
                    value={r.label}
                    onChange={(e) => change(i, "label", e.target.value)}
                    required
                  />
                </Field>
                <Field appearance="standard">
                  Calculation
                  <Select
                    appearance="standard"
                    value={r.kind}
                    onChange={(e) => change(i, "kind", e.target.value)}
                  >
                    <option value="accounts">Account group</option>
                    <option value="sum">Sum earlier rows</option>
                    <option value="difference">Difference of two rows</option>
                    <option value="ratio">Ratio of two rows</option>
                  </Select>
                </Field>
                {r.kind === "accounts" ? (
                  <>
                    <Field appearance="standard">
                      Account codes, separated by commas
                      <TextInput
                        appearance="standard"
                        value={r.accounts}
                        onChange={(e) => change(i, "accounts", e.target.value)}
                        required
                      />
                    </Field>
                    <Field appearance="standard">
                      Basis
                      <Select
                        appearance="standard"
                        value={r.basis}
                        onChange={(e) => change(i, "basis", e.target.value)}
                      >
                        <option value="period">Date range</option>
                        <option value="cumulative">Through end date</option>
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Positive balance
                      <Select
                        appearance="standard"
                        value={r.sign}
                        onChange={(e) => change(i, "sign", e.target.value)}
                      >
                        <option value="debit">Debit</option>
                        <option value="credit">Credit</option>
                      </Select>
                    </Field>
                  </>
                ) : (
                  <Field appearance="standard">
                    Earlier row keys, separated by commas
                    <TextInput
                      appearance="standard"
                      value={r.references}
                      onChange={(e) => change(i, "references", e.target.value)}
                      required
                    />
                  </Field>
                )}
                <Button
                  appearance="standard"
                  clientManaged
                  type="button"
                  disabled={editor.length === 1}
                  onClick={() =>
                    setEditor((rows) => rows.filter((_, j) => j !== i))
                  }
                >
                  Remove row
                </Button>
              </fieldset>
            ))}
            <Button
              appearance="standard"
              clientManaged
              type="button"
              onClick={() => setEditor((rows) => [...rows, blank()])}
            >
              Add row
            </Button>
            <details><summary>Include cash-flow statement</summary><Field appearance="standard">Bank account codes, separated by commas<TextInput appearance="standard" name="bankCodes"/></Field><Field appearance="standard">Classify counterpart accounts: account, kind (operating / investing / financing)<TextArea appearance="standard" name="flowCsv" rows={6} placeholder={"account,kind\n5000,operating\n1500,investing\n2500,financing"}/></Field></details>
            <Button
              appearance="standard"
              clientManaged
              type="submit"
              disabled={busy}
            >
              Save report for review
            </Button>
          </FormLayout>
        </FormDisclosure>)
      )}
      <TableFrame appearance="standard" label="Finance plan editions">
        <Table>
          <thead>
            <tr>
              <th>Plan</th>
              <th>Edition</th>
              <th>Status</th>
              <th>Review and reports</th>
            </tr>
          </thead>
          <tbody>
            {data.editions.map((r) => (
              <tr key={String(r.id)}>
                <td>
                  <RecordLink href={recordCanonicalHref(base,{kind:"finance_plan",id:String(r.id)})}>{String(r.name)} · {String(r.kind)}</RecordLink>
                </td>
                <td>{String(r.edition)}</td>
                <td>{String(r.status)}</td>
                <td>
                  <details>
                    <summary>Prepared amounts and assumptions</summary>
                    {r.kind === "forecast" ? (
                      <>
                        <p>
                          {String(
                            JSON.parse(String(r.content_json)).assumptions,
                          )}
                        </p>
                        <Table>
                          <thead>
                            <tr>
                              <th>Month / account</th>
                              <th>Base</th>
                              <th>Good</th>
                              <th>Bad</th>
                            </tr>
                          </thead>
                          <tbody>
                            {(
                              JSON.parse(String(r.content_json)).months as Row[]
                            ).map((l, i) => (
                              <tr key={i}>
                                <td>
                                  <RecordLink href={recordCanonicalHref(base,{kind:"finance_account",id:String(l.accountId)})}>{String(l.month)} · {data.accounts.find(a=>a.id===l.accountId)?.name}</RecordLink>
                                </td>
                                <td>{minorToDecimal(Number(l.baseMinor))}</td>
                                <td>{minorToDecimal(Number(l.goodMinor))}</td>
                                <td>{minorToDecimal(Number(l.badMinor))}</td>
                              </tr>
                            ))}
                          </tbody>
                        </Table>
                      </>
                    ) : (
                      <ul>
                        {(
                          (JSON.parse(String(r.content_json)).lines ??
                            JSON.parse(String(r.content_json)).rows) as Row[]
                        ).map((l, i) => (
                          <li key={i}>
                            {String(
                              l.label ??
                                data.accounts.find((a) => a.id === l.accountId)
                                  ?.name,
                            )}{" "}
                            · {String(l.month ?? l.kind)} ·{" "}
                            {l.minor !== undefined
                              ? minorToDecimal(Number(l.minor))
                              : String(l.key)}
                          </li>
                        ))}
                      </ul>
                    )}
                  </details>
                  <FinanceEvidenceDocuments base={base} row={r} targetType="finance_plan_edition" canWrite={canWrite}/>
                  {canApprove &&
                    r.status === "draft" &&
                    r.prepared_by !== userId && (
                      <FormLayout
                        onSubmit={(e) => {
                          e.preventDefault();
                          const f = new FormData(e.currentTarget);
                          void run(
                            `${paths[r.kind as keyof typeof paths]}/${r.id}/${f.get("action")}`,
                            {
                              expectedVersion: Number(r.version),
                              reason: f.get("reason"),
                            },
                          );
                        }}
                      >
                        <Field appearance="standard">
                          Review reason
                          <TextInput
                            appearance="standard"
                            name="reason"
                            required
                            minLength={3}
                          />
                        </Field>
                        <Select appearance="standard" name="action">
                          <option value="approve">Approve</option>
                          <option value="reject">Reject</option>
                        </Select>
                        <Button
                          appearance="standard"
                          clientManaged
                          type="submit"
                          disabled={busy}
                        >
                          Confirm review
                        </Button>
                      </FormLayout>
                    )}
                  {r.status === "approved" && r.kind === "budget" && (
                    <Button
                      appearance="standard"
                      clientManaged
                      disabled={busy}
                      onClick={() => void actuals(String(r.id))}
                    >
                      Budget vs actual
                    </Button>
                  )}
                  {canWrite &&
                    r.status === "approved" &&
                    r.kind === "report" && (
                      <FormLayout
                        className="ui-form-grid"
                        onSubmit={(e) => {
                          e.preventDefault();
                          const f = new FormData(e.currentTarget);
                          void run(`report-definitions/${r.id}/snapshot`, {
                            startDate: f.get("startDate"),
                            endDate: f.get("endDate"),
                          });
                        }}
                      >
                        <Field appearance="standard">
                          From
                          <TextInput
                            appearance="standard"
                            name="startDate"
                            type="date"
                            required
                          />
                        </Field>
                        <Field appearance="standard">
                          Through
                          <TextInput
                            appearance="standard"
                            name="endDate"
                            type="date"
                            required
                          />
                        </Field>
                        <Button
                          appearance="standard"
                          clientManaged
                          type="submit"
                          disabled={busy}
                        >
                          Create report
                        </Button>
                      </FormLayout>
                    )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
      {data.snapshots.length>0&&<section><h3>Retained reports</h3><ul>{data.snapshots.map(s=><li key={String(s.id)}><RecordLink href={recordCanonicalHref(base,{kind:"finance_report_snapshot",id:String(s.id)})}>{String(s.name)} · View retained report</RecordLink>{" · "}<a href={`${api}/report-snapshots/${s.id}/export`}>{String(s.name)} · {String(s.start_date)}–{String(s.end_date)} · Download CSV</a></li>)}</ul></section>}
      {output && (
        <section aria-live="polite">
          <h3>Report results</h3>
          {output.snapshotId && (
            <a href={`${api}/report-snapshots/${output.snapshotId}/export`}>
              Download retained CSV
            </a>
          )}
          <TableFrame appearance="standard" label="Report result">
            <Table>
              <thead>
                <tr>
                  <th>Row / month</th>
                  <th>Budget / amount</th>
                  <th>Actual</th>
                  <th>Variance</th>
                </tr>
              </thead>
              <tbody>
                {output.rows.map((r, i) => (
                  <tr key={i}>
                    <td>
                      {output.snapshotId?<RecordLink href={recordCanonicalHref(base,{kind:'finance_report_row',id:output.snapshotId,args:{key:String(r.key)}})}>{String(r.label)}</RecordLink>:output.planId?<RecordLink href={recordCanonicalHref(base,{kind:'finance_budget_sources',id:output.planId,args:{line:String(i)}})}>{data.accounts.find(a=>a.id===r.accountId)?.name} · {String(r.month)}</RecordLink>:String(r.label??r.month??'')}
                    </td>
                    <td>{String(r.budget ?? r.value ?? "—")}</td>
                    <td>{String(r.actual ?? "")}</td>
                    <td>{String(r.variance ?? "")}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
          {output.cashFlow&&<TableFrame appearance="standard" label="Cash-flow statement"><Table><thead><tr><th>Cash flow</th><th>Amount</th></tr></thead><tbody>{["opening","operating","investing","financing","change","closing"].map(key=><tr key={key}><td>{output.snapshotId?<RecordLink href={recordCanonicalHref(base,{kind:"finance_report_row",id:output.snapshotId,args:{cashFlow:key}})}>{key}</RecordLink>:key}</td><td>{output.cashFlow![key]}</td></tr>)}</tbody></Table></TableFrame>}
        </section>
      )}
    </Card>
  );
}
