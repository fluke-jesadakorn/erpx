"use client";
import {FinanceEvidenceDocuments} from "./finance-evidence-documents";
import {FinanceBillingRevision} from "./finance-billing-revision";
import {FinancePrepareOn,financeDraftCommand} from "./finance-prepare-on";
import { useState,useContext } from "react";
import {EmbeddedRecord} from "@/components/records/record-context";
import {recordCanonicalHref} from "@/lib/records/record-drawer";
import { useRouter } from "next/navigation";
import { financeCommandFetch as workFetch } from "@/lib/finance/finance-command-fetch";
import { financeCsvRows } from "@/lib/finance/finance-csv";
import type {
  BusinessContractInput,
  RecognitionItem,
} from "@/server/modules/finance/public/rules/finance-contract-policy";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  TextInput,
  TextArea,
  Table,
  TableFrame,
} from "@/components/ui/foundation";
import { RecordLink } from "@/components/records/record-link";
type Row = Record<string, string | number | null>;
type Data = {
  contracts: Row[];
  bills: Row[];
  recognitions: Row[];
  corrections?:Row[];
  accounts: Row[];
  parties: Row[];
  charts: Row[];
  periods: Row[];
};
function ContractEditor({
  data,
  initial,
  onSave,
  busy,
}: {
  data: Data;
  initial?: Row;
  onSave: (input: unknown) => Promise<void>;
  busy: boolean;
}) {
  const original = initial
    ? (JSON.parse(String(initial.content_json)) as BusinessContractInput)
    : null;
  const [chart, setChart] = useState(
      original?.chartId ?? String(data.charts[0]?.id ?? ""),
    ),
    accounts = data.accounts.filter((a) => a.chart_id === chart),
    currency = String(
      data.charts.find((c) => c.id === chart)?.base_currency ?? "THB",
    );
  function accountField(
    name: string,
    label: string,
    filter: (r: Row) => boolean,
    value?: string,
    required = true,
  ) {
    return (
      <Field appearance="standard">
        {label}
        <Select
          appearance="standard"
          name={name}
          required={required}
          defaultValue={value ?? ""}
        >
          <option value="">Select account</option>
          {accounts.filter(filter).map((a) => (
            <option key={String(a.id)} value={String(a.id)}>
              {String(a.code)} · {String(a.name)}
            </option>
          ))}
        </Select>
      </Field>
    );
  }
  return (
    <FormLayout
      className="ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget),
          text = (k: string) => String(f.get(k) ?? "");
        let obligations: BusinessContractInput["obligations"],
          pricing: BusinessContractInput["pricing"];
        try {
          const detailed = text("obligations").trim();
          obligations = detailed
            ? financeCsvRows(detailed).map((r) => ({
                key: r.key,
                name: r.name,
                standalonePrice: r.standalonePrice,
                revenueAccountId: String(
                  accounts.find((a) => a.code === r.revenueAccountCode)?.id ??
                    "",
                ),
                mode: r.mode as "point" | "monthly" | "milestone",
                milestones: r.milestones
                  ? r.milestones.split("|").map((m) => {
                      const [date, weight] = m.split(":");
                      return { date, weight: Number(weight) };
                    })
                  : [],
              }))
            : [
                {
                  key: "service",
                  name: text("name"),
                  standalonePrice: text("amount"),
                  revenueAccountId: text("income"),
                  mode: text("mode") as "point" | "monthly",
                  milestones: [],
                },
              ];
          pricing = text("tiers").trim()
            ? {
                kind: "tier",
                tiers: financeCsvRows(text("tiers")).map((t) => ({
                  upTo: t.upTo === "unlimited" ? null : Number(t.upTo),
                  unitPrice: t.unitPrice,
                })),
              }
            : { kind: "flat", amount: text("amount") };
        } catch (error) {
          await onSave({
            csvError: error instanceof Error ? error.message : "Invalid CSV",
          });
          return;
        }
        await onSave({
          chartId: chart,
          partyId: text("party"),
          number: text("number"),
          name: text("name"),
          currency,
          startDate: text("startDate"),
          endDate: text("endDate"),
          cycle: text("cycle"),
          pricing,
          receivableAccountId: text("receivable"),
          deferredAccountId: text("deferred"),
          taxBps: Number(text("taxBps")),
          taxCode: text("taxCode") || undefined,
          taxAccountId: text("taxAccount") || undefined,
          obligations,
          dimensions: Object.fromEntries(
            text("dimensions")
              .split("\n")
              .filter(Boolean)
              .map((v) => v.split("=")),
          ),
          reason: text("reason"),
          ...(initial
            ? { parentId: initial.id, expectedVersion: Number(initial.version) }
            : {}),
        });
      }}
    >
      <Field appearance="standard">
        Primary book
        <Select
          appearance="standard"
          name="chart"
          value={chart}
          onChange={(e) => setChart(e.target.value)}
          disabled={!!initial}
        >
          {data.charts.map((c) => (
            <option key={String(c.id)} value={String(c.id)}>
              {String(c.name)} · {String(c.base_currency)}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance="standard">
        Customer
        <Select
          appearance="standard"
          name="party"
          required
          defaultValue={original?.partyId ?? ""}
          disabled={!!initial}
        >
          {initial && (
            <option value={original!.partyId}>
              {String(
                data.parties.find((p) => p.id === original!.partyId)?.name ??
                  "Current customer",
              )}
            </option>
          )}
          {!initial && <option value="">Select customer</option>}
          {!initial &&
            data.parties
              .filter((p) => p.currency === currency)
              .map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {String(p.name)}
                </option>
              ))}
        </Select>
        {initial && (
          <input type="hidden" name="party" value={original!.partyId} />
        )}
      </Field>
      <Field appearance="standard">
        Contract number
        <TextInput
          appearance="standard"
          name="number"
          required
          defaultValue={original?.number}
          readOnly={!!initial}
        />
      </Field>
      <Field appearance="standard">
        Service name
        <TextInput
          appearance="standard"
          name="name"
          required
          defaultValue={original?.name}
        />
      </Field>
      <Field appearance="standard">
        Service starts
        <TextInput
          appearance="standard"
          type="date"
          name="startDate"
          required
          defaultValue={original?.startDate}
        />
      </Field>
      <Field appearance="standard">
        Service ends
        <TextInput
          appearance="standard"
          type="date"
          name="endDate"
          required
          defaultValue={original?.endDate}
        />
      </Field>
      <Field appearance="standard">
        Billing
        <Select
          appearance="standard"
          name="cycle"
          defaultValue={original?.cycle ?? "monthly"}
        >
          <option value="monthly">Monthly, prorate calendar days</option>
          <option value="once">Once for the service term</option>
          <option value="usage">Reviewed usage, units × price</option>
        </Select>
      </Field>
      <Field appearance="standard">
        Price ({currency})
        <TextInput
          appearance="standard"
          name="amount"
          required
          inputMode="decimal"
          defaultValue={
            original?.pricing.kind === "flat" ? original.pricing.amount : "0.00"
          }
        />
      </Field>
      {accountField(
        "receivable",
        "Receivable control",
        (a) => a.control_kind === "receivable",
        original?.receivableAccountId,
      )}
      {accountField(
        "deferred",
        "Deferred revenue",
        (a) => a.category === "liability",
        original?.deferredAccountId,
      )}
      {accountField(
        "income",
        "Revenue",
        (a) => a.category === "revenue",
        original?.obligations[0]?.revenueAccountId,
      )}
      <Field appearance="standard">
        Recognition
        <Select
          appearance="standard"
          name="mode"
          defaultValue={original?.obligations[0]?.mode ?? "monthly"}
        >
          <option value="monthly">Monthly over the service term</option>
          <option value="point">Once at service completion</option>
        </Select>
      </Field>
      <details className="full-span">
        <summary>
          VAT, reporting dimensions, tiers and multiple obligations
        </summary>
        <Field appearance="standard">
          Ordinary Thai VAT
          <Select
            appearance="standard"
            name="taxBps"
            defaultValue={original?.taxBps ?? 0}
          >
            <option value="0">No VAT charge</option>
            <option value="700">7%</option>
          </Select>
        </Field>
        <Field appearance="standard">
          VAT code
          <TextInput
            appearance="standard"
            name="taxCode"
            defaultValue={original?.taxCode}
          />
        </Field>
        {accountField(
          "taxAccount",
          "Output VAT account",
          (a) => a.category === "liability",
          original?.taxAccountId,
          false,
        )}
        <Field appearance="standard">
          Dimensions, one code=value per line
          <TextArea
            appearance="standard"
            name="dimensions"
            defaultValue={
              original
                ? Object.entries(original.dimensions)
                    .map(([k, v]) => `${k}=${v}`)
                    .join("\n")
                : ""
            }
          />
        </Field>
        <Field appearance="standard">
          Graduated usage tiers CSV
          <TextArea
            appearance="standard"
            name="tiers"
            placeholder={"upTo,unitPrice\n10,2.00\nunlimited,1.00"}
            defaultValue={
              original?.pricing.kind === "tier"
                ? "upTo,unitPrice\n" +
                  original.pricing.tiers
                    .map((t) => `${t.upTo ?? "unlimited"},${t.unitPrice}`)
                    .join("\n")
                : ""
            }
          />
        </Field>
        <Field appearance="standard">
          Obligations CSV, optional
          <TextArea
            appearance="standard"
            name="obligations"
            placeholder={
              "key,name,standalonePrice,revenueAccountCode,mode,milestones\nservice,Support,100.00,4000,monthly,"
            }
            defaultValue={
              original
                ? "key,name,standalonePrice,revenueAccountCode,mode,milestones\n" +
                  original.obligations
                    .map((o) =>
                      [
                        o.key,
                        o.name,
                        o.standalonePrice,
                        data.accounts.find((a) => a.id === o.revenueAccountId)
                          ?.code ?? "",
                        o.mode,
                        o.milestones
                          .map((m) => `${m.date}:${m.weight}`)
                          .join("|"),
                      ]
                        .map((v) => `"${String(v).replaceAll('"', '""')}"`)
                        .join(","),
                    )
                    .join("\n")
                : ""
            }
          />
        </Field>
        <p className="muted">
          Modes: point, monthly, milestone. Milestones use date:weight separated
          by |. Standalone prices allocate the billed subtotal; they do not
          change the invoice total.
        </p>
      </details>
      <Field appearance="standard">
        Agreement evidence and reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" clientManaged type="submit" disabled={busy}>
        Prepare for independent approval
      </Button>
    </FormLayout>
  );
}
export function FinanceBusinessContractControls({
  base,
  data,
  canWrite,
  canApprove,
  userId,
}: {
  base: string;
  data: Data;
  canWrite: boolean;
  canApprove: boolean;
  userId: string;
}) {
  const recordOnly=useContext(EmbeddedRecord);
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function run(path: string, input: unknown) {
    setBusy(true);
    setError("");
    try {
      if (input && typeof input === "object" && "csvError" in input)
        throw new Error(String(input.csvError));
      const r = await workFetch(
          `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/${path}`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": crypto.randomUUID(),
            },
            body: JSON.stringify(input),
          },
        ),
        body = await r.json();
      if (!r.ok)
        throw new Error(body.error?.message ?? "Finance action failed");
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Finance action failed");
    } finally {
      setBusy(false);
    }
  }
  function decision(path: string, row: Row, actions: string[]) {
    return (
      <FormLayout
        className="ui-form-grid"
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(
            e.currentTarget,
            (e.nativeEvent as SubmitEvent).submitter,
          );
          await run(`${path}/${row.id}/${f.get("action")}`, {
            expectedVersion: Number(row.version),
            reason: f.get("reason"),
          });
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
        {actions.map((a) => (
          <Button
            appearance="standard"
            key={a}
            name="action"
            value={a}
            type="submit"
            clientManaged
            disabled={busy}
          >
            {a}
          </Button>
        ))}
      </FormLayout>
    );
  }
  return (
    <>
      {error && <InlineNotice tone="danger">{error}</InlineNotice>}
      <Card appearance="standard" as="section" className="ui-panel">
        <h2>Business contracts</h2>
        <p className="muted">
          Approve the agreement, prepare its ordinary invoice, then recognize
          delivered service from the retained allocation.
        </p>
        {canWrite && !recordOnly && (
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Prepare business contract"
          >
            <summary>Prepare contract</summary>
            <ContractEditor
              data={data}
              onSave={(i) => run("business-contracts", i)}
              busy={busy}
            />
          </FormDisclosure>
        )}
        {data.contracts.map((c) => (
          <Card
            appearance="standard"
            key={String(c.id)}
            as="section"
            className="ui-panel"
          >
            <h3>
              <RecordLink record={{kind:"finance_business_contract",id:String(c.id)}} href={recordCanonicalHref(base,{kind:"finance_business_contract",id:String(c.id)})}>{String(c.number)} · {String(c.name)}</RecordLink> · edition{" "}
              {String(c.edition)}
            </h3>
            <p>
              {String(c.start_date)} to {String(c.end_date)} ·{" "}
              {String(c.status)}
            </p>
            <FinanceEvidenceDocuments base={base} row={c} targetType="finance_business_contract" canWrite={canWrite}/>
            {canApprove &&
              c.prepared_by !== userId &&
              c.status === "draft" &&
              decision("business-contracts", c, ["approve", "reject"])}
            {c.status === "active" && (
              <>
                {canWrite && (
                  <FormDisclosure error={error}
                    appearance="standard"
                    drawerTitle={`Bill ${c.number}`}
                  >
                    <summary>Prepare invoice</summary>
                    <FormLayout
                      className="ui-form-grid"
                      onSubmit={async (e) => {
                        e.preventDefault();
                        const f = new FormData(e.currentTarget);
                        await run(...financeDraftCommand(`business-contracts/${c.id}/bill`, "billing", c.id, f, {
                          expectedVersion: Number(c.version),
                          month: f.get("month") || undefined,
                          invoiceDate: f.get("date"),
                          dueDate: f.get("due"),
                          usageUnits: Number(f.get("usage") || 0),
                          usageEvidence: f.get("evidence") || "",
                        }));
                      }}
                    >
                      <Field appearance="standard">
                        Billing month (monthly / usage)
                        <TextInput
                          appearance="standard"
                          name="month"
                          type="month"
                        />
                      </Field>
                      <FinancePrepareOn/>
                      <Field appearance="standard">
                        Invoice date
                        <TextInput
                          appearance="standard"
                          name="date"
                          type="date"
                          required
                        />
                      </Field>
                      <Field appearance="standard">
                        Due date
                        <TextInput
                          appearance="standard"
                          name="due"
                          type="date"
                          required
                        />
                      </Field>
                      <Field appearance="standard">
                        Usage units
                        <TextInput
                          appearance="standard"
                          name="usage"
                          type="number"
                          min={0}
                          step={1}
                        />
                      </Field>
                      <Field appearance="standard">
                        Usage evidence
                        <TextArea appearance="standard" name="evidence" />
                      </Field>
                      <Button
                        appearance="standard"
                        clientManaged
                        type="submit"
                        disabled={busy}
                      >
                        Prepare draft invoice
                      </Button>
                    </FormLayout>
                  </FormDisclosure>
                )}
                {canWrite && (
                  <FormDisclosure error={error}
                    appearance="standard"
                    drawerTitle={`Prepare next edition of ${c.number}`}
                  >
                    <summary>Amend or renew</summary>
                    <ContractEditor
                      data={data}
                      initial={c}
                      onSave={(i) => run("business-contracts", i)}
                      busy={busy}
                    />
                  </FormDisclosure>
                )}
                {canApprove &&
                  c.prepared_by !== userId &&
                  decision("business-contracts", c, ["cancel"])}
              </>
            )}
            <details>
              <summary>Retained agreement and allocation policy</summary>
              <pre className="muted">
                {JSON.stringify(JSON.parse(String(c.content_json)), null, 2)}
              </pre>
              <p>Evidence {String(c.source_hash)}</p>
            </details>
          </Card>
        ))}
      </Card>
      <Card appearance="standard" as="section" className="ui-panel">
        <h2>Billing and recognition plans</h2>
        {data.bills.map((b) => {
          const result = JSON.parse(String(b.result_json)) as {
            subtotal: string;
            tax: string;
            items: RecognitionItem[];
          };
          return (
            <Card
              appearance="standard"
              as="section"
              key={String(b.id)}
              className="ui-panel"
            >
              <h3>
                {b.invoice_id ? <RecordLink href={`${base}/finance/invoices/${b.invoice_id}`}>
                  {String(b.invoice_number)}
                </RecordLink> : <>Zero usage · no charge</>}{" "}
                · {String(b.invoice_status)} <RecordLink record={{kind:"finance_business_bill",id:String(b.id)}} href={recordCanonicalHref(base,{kind:"finance_business_bill",id:String(b.id)})}>View billing plan</RecordLink>
              </h3>
              <p>
                {String(b.service_from)} to {String(b.service_to)} · service{" "}
                {result.subtotal}, VAT {result.tax}
              </p>
              <p>Billing edition {Number(b.bill_edition??1)} {b.is_current===0?'· retained earlier edition':''} {b.parent_run_id&&<RecordLink record={{kind:'finance_business_bill',id:String(b.parent_run_id)}} href={recordCanonicalHref(base,{kind:'finance_business_bill',id:String(b.parent_run_id)})}>Previous billing edition</RecordLink>}{b.revision_reason&&<> · {String(b.revision_reason)}</>}</p>
              {canWrite&&b.is_current!==0&&(!b.invoice_id||['draft','voided'].includes(String(b.invoice_status)))&&b.effective_contract_content_json&&<FinanceBillingRevision bill={b} accounts={data.accounts} busy={busy} onSave={input=>run(`business-bills/${b.id}/revisions`,input)}/>}
              {canWrite&&b.is_current!==0&&['posted','partially_paid','paid'].includes(String(b.invoice_status))&&!(data.corrections??[]).some(c=>c.billing_run_id===b.id&&c.status!=='rejected')&&<FinanceBillingRevision bill={b} accounts={data.accounts} periods={data.periods} posted busy={busy} onSave={input=>run(`business-bills/${b.id}/corrections`,input)}/>}
              {(data.corrections??[]).filter(c=>c.billing_run_id===b.id).map(c=><div key={String(c.id)}><h4>Credit {String(c.number)} · {String(c.status)}</h4><p>{String(c.entry_date)} · {String(c.reason)}</p><FinanceEvidenceDocuments base={base} row={c} targetType="finance_billing_correction" canWrite={canWrite}/>{c.credit_journal_id&&<RecordLink href={`${base}/finance/journals/${c.credit_journal_id}`}>Credit journal</RecordLink>}{c.replacement_bill_id&&<RecordLink record={{kind:"finance_business_bill",id:String(c.replacement_bill_id)}} href={recordCanonicalHref(base,{kind:"finance_business_bill",id:String(c.replacement_bill_id)})}>Replacement billing edition</RecordLink>}{Number(c.refund_minor)>0&&<RecordLink href={`${base}/finance/refunds/refund-${c.id}`}>Review refund</RecordLink>}{canApprove&&c.prepared_by!==userId&&c.status==='draft'&&decision('billing-corrections',c,['post','reject'])}</div>)}
              {!b.invoice_id && <><p>Reviewed usage evidence: {String((JSON.parse(String(b.input_json)) as {usageEvidence:string}).usageEvidence)}</p><FinanceEvidenceDocuments base={base} row={{...b,status:b.is_current===0?'superseded':b.no_charge_status}} targetType="finance_business_bill" canWrite={canWrite&&b.is_current!==0}/>{canApprove && b.created_by!==userId && b.no_charge_status==='draft' && b.is_current!==0 && decision("zero-usage-bills",b,["approve","reject"])}</>}
              <TableFrame appearance="standard" label="Recognition plan">
                <Table>
                  <thead>
                    <tr>
                      <th>Obligation</th>
                      <th>Due</th>
                      <th>Amount</th>
                      <th>Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {result.items.map((item) => {
                      const current = data.recognitions.find(
                        (r) =>
                          r.billing_run_id === b.id &&
                          r.item_key === item.key &&
                          ["draft", "posted"].includes(String(r.status)),
                      );
                      return (
                        <tr key={item.key}>
                          <td><RecordLink href={recordCanonicalHref(base,{kind:"finance_obligation",id:String(b.id),args:{item:item.key}})}>{item.name}</RecordLink></td>
                          <td>{item.date}</td>
                          <td>{item.amount}</td>
                          <td>
                            {current ? (
                              String(current.status)
                            ) : canWrite &&
                              Number(item.amount) > 0 &&
                              ["posted", "partially_paid", "paid"].includes(
                                String(b.invoice_status),
                              ) ? (
                              <FormDisclosure error={error}
                                appearance="standard"
                                drawerTitle={`Recognize ${item.name}`}
                              >
                                <summary>Prepare recognition</summary>
                                <FormLayout
                                  className="ui-form-grid"
                                  onSubmit={async (e) => {
                                    e.preventDefault();
                                    const f = new FormData(e.currentTarget);
                                    await run(...financeDraftCommand("recognition-plans", "recognition", b.id, f, {
                                      billingRunId: b.id,
                                      itemKey: item.key,
                                      periodId: f.get("period"),
                                      entryDate: f.get("date"),
                                      reason: f.get("reason"),
                                    }));
                                  }}
                                >
                                  <Field appearance="standard">
                                    Open period
                                    <Select
                                      appearance="standard"
                                      name="period"
                                      required
                                    >
                                      {data.periods.map((p) => (
                                        <option
                                          key={String(p.id)}
                                          value={String(p.id)}
                                        >
                                          {String(p.name)}
                                        </option>
                                      ))}
                                    </Select>
                                  </Field>
                                  <FinancePrepareOn/>
                                  <Field appearance="standard">
                                    Accounting date
                                    <TextInput
                                      appearance="standard"
                                      type="date"
                                      name="date"
                                      required
                                      min={item.date}
                                      defaultValue={item.date}
                                    />
                                  </Field>
                                  <Field appearance="standard">
                                    Delivery evidence
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
                                    disabled={busy}
                                  >
                                    Prepare recognition
                                  </Button>
                                </FormLayout>
                              </FormDisclosure>
                            ) : (
                              "Post invoice first"
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </Table>
              </TableFrame>
            </Card>
          );
        })}
      </Card>
      <Card appearance="standard" as="section" className="ui-panel">
        <h2>Recognition evidence</h2>
        {data.recognitions.map((r) => (
          <Card
            appearance="standard"
            as="section"
            className="ui-panel"
            key={String(r.id)}
          >
            <h3>
              <RecordLink record={{kind:"finance_recognition",id:String(r.id)}} href={recordCanonicalHref(base,{kind:"finance_recognition",id:String(r.id)})}>{String(r.item_key)}</RecordLink> · {String(r.status)}
            </h3>
            <p>
              Due {String(r.due_date)} · accounting date {String(r.entry_date)}{" "}
              · {String(r.reason)}
            </p>
            {r.journal_id && (
              <RecordLink href={`${base}/finance/journals/${r.journal_id}`}>
                Posted journal
              </RecordLink>
            )}
            {r.reversal_journal_id && (
              <RecordLink
                href={`${base}/finance/journals/${r.reversal_journal_id}`}
              >
                Reversal journal
              </RecordLink>
            )}
            <FinanceEvidenceDocuments base={base} row={r} targetType="finance_recognition_run" canWrite={canWrite}/>
            {canApprove &&
              r.prepared_by !== userId &&
              r.status === "draft" &&
              decision("recognition-plans", r, ["post", "reject"])}
            {canApprove &&
              r.prepared_by !== userId &&
              r.status === "posted" && (
                <FormDisclosure error={error}
                  appearance="standard"
                  drawerTitle="Reverse with retained evidence"
                >
                  <summary>Reverse recognition</summary>
                  <FormLayout
                    className="ui-form-grid"
                    onSubmit={async (e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget);
                      await run(`recognition-plans/${r.id}/reverse`, {
                        expectedVersion: Number(r.version),
                        periodId: f.get("period"),
                        entryDate: f.get("date"),
                        reason: f.get("reason"),
                      });
                    }}
                  >
                    <Field appearance="standard">
                      Open period
                      <Select appearance="standard" name="period" required>
                        {data.periods.map((p) => (
                          <option key={String(p.id)} value={String(p.id)}>
                            {String(p.name)}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Reversal date
                      <TextInput
                        appearance="standard"
                        type="date"
                        name="date"
                        min={String(r.entry_date)}
                        required
                      />
                    </Field>
                    <Field appearance="standard">
                      Correction reason
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
                      disabled={busy}
                    >
                      Reverse
                    </Button>
                  </FormLayout>
                </FormDisclosure>
              )}
          </Card>
        ))}
      </Card>
    </>
  );
}
