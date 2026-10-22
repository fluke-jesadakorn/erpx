"use client";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import { useFinanceDraftScope, captureFinanceForm, acknowledgeFinanceForm } from "@erpx/feature-finance/components/finance-draft-boundary";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {EmbeddedRecord} from "@erpx/ui/components/record-context";
import {recordCanonicalHref} from "@erpx/feature-records/lib/record-drawer";
import { useContext,useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { financeCommandFetch as workFetch } from "@erpx/feature-finance/lib/finance-command-fetch";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  Table,
  TextArea,
  TextInput,
} from "@erpx/ui/components/foundation";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { FinanceExpansionDecision } from "@erpx/feature-finance/components/finance-expansion-decision";
import {FinancePrepareOn,financeDraftCommand} from "@erpx/feature-finance/components/finance-prepare-on";
import type { InstrumentInput } from "@erpx/server-finance/public/rules/finance-instrument-policy";
import { RecordLink } from "@erpx/feature-records/components/record-link";
type Row = Record<string, string | number | null>;
type Data = {
  editions: Row[];
  runs: Row[];
  accounts: Row[];
  periods: Row[];
  bank: Row[];
};
function InstrumentEditor({
  data,
  initial,
  busy,
  onSave,
}: {
  data: Data;
  initial?: Row;
  busy: boolean;
  onSave: (input: unknown) => Promise<void>;
}) {
  const draftScope=useFinanceDraftScope();
  const old = initial
      ? (JSON.parse(String(initial.content_json)) as InstrumentInput)
      : null,
    [kind, setKind] = useState(old?.kind ?? "lease"),
    charts = [...new Set(data.accounts.map((a) => String(a.chart_id)))],
    [chart, setChart] = useState(old?.chartId ?? charts[0] ?? ""),
    accounts = data.accounts.filter((a) => a.chart_id === chart),
    currency = String(accounts[0]?.currency ?? "THB");
  function mapped(
    name: string,
    label: string,
    category: string,
    control = "none",
  ) {
    return (
      <Field appearance="standard">
        {label}
        <Select
          appearance="standard"
          name={name}
          required
          defaultValue={String(old?.[name as keyof InstrumentInput] ?? "")}
        >
          <option value="">Choose account</option>
          {accounts
            .filter(
              (a) => a.category === category && a.control_kind === control,
            )
            .map((a) => (
              <option key={String(a.id)} value={String(a.id)}>
                {String(a.code)} · {String(a.name)}
              </option>
            ))}
        </Select>
      </Field>
    );
  }
  return (
    <FormLayout data-finance-draft={draftScope}
      className="ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget),
          cash = data.bank.find((b) => b.id === f.get("bankTransactionId"));
        await onSave({
          ...Object.fromEntries(f),
          kind,
          chartId: chart,
          currency,
          firstDueDate: f.get("firstDueDate") || undefined,
          months: Number(f.get("months")),
          annualRateBps: Number(f.get("annualRateBps")),
          principal: kind === "loan" ? f.get("principal") : undefined,
          bankTransactionId: kind === "loan" ? cash?.id : undefined,
          bankVersion: kind === "loan" ? cash?.version : undefined,
          parentId: initial?.id,
          parentVersion: initial?.version,
          dimensions: Object.fromEntries(
            String(f.get("dimensions") ?? "")
              .split("\n")
              .filter(Boolean)
              .map((l) => l.split("=")),
          ),
        });
      }}
    >
      <Field appearance="standard">
        Agreement type
        <Select
          appearance="standard"
          value={kind}
          onChange={(e) => setKind(e.target.value as "lease" | "loan")}
          disabled={!!initial}
        >
          <option value="lease">Fixed-payment lease</option>
          <option value="loan">Fixed-rate loan</option>
        </Select>
      </Field>
      <Field appearance="standard">
        Book
        <Select
          appearance="standard"
          value={chart}
          onChange={(e) => setChart(e.target.value)}
          disabled={!!initial}
        >
          {charts.map((c) => (
            <option key={c} value={c}>
              {String(
                data.accounts.find((a) => a.chart_id === c)?.chart_name ?? c,
              )}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance="standard">
        Agreement number
        <TextInput
          appearance="standard"
          name="number"
          required
          defaultValue={old?.number}
          readOnly={!!initial}
        />
      </Field>
      <Field appearance="standard">
        Name
        <TextInput
          appearance="standard"
          name="name"
          required
          defaultValue={old?.name}
        />
      </Field>
      <Field appearance="standard">
        Commencement or amendment date
        <TextInput
          appearance="standard"
          name="startDate"
          type="date"
          required
          defaultValue={old?.startDate}
        />
      </Field>
      <Field appearance="standard">
        First payment due date (optional)
        <TextInput
          appearance="standard"
          name="firstDueDate"
          type="date"
        />
        <small className="muted">Leave blank for one calendar month after commencement. Use the next unpaid date when amending a lease.</small>
      </Field>
      <Field appearance="standard">
        Open accounting period
        <Select appearance="standard" name="periodId" required>
          {data.periods.map((p) => (
            <option key={String(p.id)} value={String(p.id)}>
              {String(p.name)}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance="standard">
        Monthly payments
        <TextInput
          appearance="standard"
          name="months"
          type="number"
          min={1}
          max={240}
          required
          defaultValue={old?.months ?? 12}
        />
      </Field>
      <Field appearance="standard">
        Fixed monthly amount · {currency}
        <TextInput
          appearance="standard"
          name="payment"
          required
          inputMode="decimal"
          defaultValue={old?.payment}
        />
      </Field>
      <Field appearance="standard">
        Annual rate in basis points · 1200 = 12%
        <TextInput
          appearance="standard"
          name="annualRateBps"
          type="number"
          min={0}
          max={100000}
          required
          defaultValue={old?.annualRateBps ?? 0}
        />
      </Field>
      {mapped("bankAccountId", "Bank account", "asset", "bank")}
      {mapped("liabilityAccountId", "Financing liability", "liability")}
      {mapped("interestAccountId", "Interest expense", "expense")}
      {kind === "loan" ? (
        <>
          <Field appearance="standard">
            Funded principal · {currency}
            <TextInput
              appearance="standard"
              name="principal"
              required
              inputMode="decimal"
              defaultValue={old?.principal}
            />
          </Field>
          <Field appearance="standard">
            Imported funding credit
            <Select appearance="standard" name="bankTransactionId" required>
              <option value="">Choose exact actual bank credit</option>
              {data.bank
                .filter((b) => Number(b.amount_minor) > 0)
                .map((b) => (
                  <option key={String(b.id)} value={String(b.id)}>
                    {String(b.transaction_date)} ·{" "}
                    {minorToDecimal(Number(b.amount_minor))} ·{" "}
                    {String(b.external_id)}
                  </option>
                ))}
            </Select>
          </Field>
        </>
      ) : (
        <>
          {mapped("rightOfUseAccountId", "Right-of-use asset", "asset")}
          {mapped(
            "accumulatedAccountId",
            "Accumulated ROU depreciation",
            "asset",
          )}
          {mapped(
            "depreciationAccountId",
            "ROU depreciation expense",
            "expense",
          )}
        </>
      )}
      <details>
        <summary>Reporting dimensions and calculation convention</summary>
        <p className="muted">
          Payments fall monthly from the first due date, or one month after commencement when it is blank. Each scheduled instalment uses the annual
          rate divided by 12, rounded to cents. A loan&apos;s final payment settles
          its remaining balance. Early loan payoff uses actual days / 365
          between settled instalments.
        </p>
        <Field appearance="standard">
          Dimensions · one code=value per line
          <TextArea
            appearance="standard"
            name="dimensions"
            defaultValue={Object.entries(old?.dimensions ?? {})
              .map(([k, v]) => `${k}=${v}`)
              .join("\n")}
          />
        </Field>
      </details>
      <Field appearance="standard">
        Terms evidence and reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" clientManaged type="submit" disabled={busy}>
        Prepare for review
      </Button>
    </FormLayout>
  );
}
export function FinanceInstrumentControls({
  base,
  data,
  canWrite,
  canApprove,
  userId,
  enabled,
}: {
  base: string;
  data: Data;
  canWrite: boolean;
  canApprove: boolean;
  userId: string;
  enabled: boolean;
}) {
  const draftScope=useFinanceDraftScope();
  const [hasAttempt,setHasAttempt]=useState(false);
  const recordOnly=useContext(EmbeddedRecord);
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const canPrepare = useFinanceActionAllowed("new-work","finance.advanced"), canSettle = useFinanceActionAllowed("settle-existing","finance.advanced");
  async function run(path: string, input: unknown) {
    const completion=captureFinanceForm(),url=`${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/${path}`,payload=JSON.stringify(input);
    if (!(path === "instruments" ? canPrepare : canSettle) && !workFetch.hasAttempt(url,payload)) { setError("This financing action is unavailable for the current package."); return; }

    setHasAttempt(true);
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(
          url,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": crypto.randomUUID(),
            },
            body: payload,
          },
        ),
        body = await r.json();
      if (!r.ok)
        throw new Error(body.error?.message ?? "Financing command failed");
      if (body.data === undefined) throw new Error("Financing acknowledgement is missing. Retry the same action.");
      acknowledgeFinanceForm(completion);setHasAttempt(false);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Financing command failed");
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <Card appearance="standard" as="section" className="ui-panel">
        <h2>Leases and loans</h2>
        <p className="muted">
          Review a fixed monthly schedule, then post each actual imported bank
          movement with independent approval.
        </p>
        {!enabled && (
          <p className="muted">
            This Company&apos;s tool is off. Enable it in Finance setup after its
            acceptance checks.
          </p>
        )}
        {error && <InlineNotice tone="danger">{error}</InlineNotice>}
        {!recordOnly && enabled && canWrite && (
          <FormDisclosure error={error}
            appearance="standard"
            drawerTitle="Prepare financing agreement"
          >
            <summary>Prepare agreement</summary>
            <InstrumentEditor
              data={data}
              busy={busy || (!canPrepare && !hasAttempt)}
              onSave={(i) => run("instruments", i)}
            />
          </FormDisclosure>
        )}
      </Card>
      {data.editions.map((e) => {
        const c = JSON.parse(String(e.content_json)) as InstrumentInput,
          s = JSON.parse(String(e.schedule_json)) as {
            principal: string;
            rows: Array<{
              ordinal: number;
              dueDate: string;
              payment: string;
              interest: string;
              principal: string;
              closing: string;
              depreciation: string;
            }>;
          };
        return (
          <Card
            key={String(e.id)}
            appearance="standard"
            as="section"
            className="ui-panel"
          >
            <h2>
              <RecordLink href={recordCanonicalHref(base,{kind:"finance_instrument",id:String(e.id)})}>{String(e.number)} · {String(e.name)}</RecordLink>
            </h2>
            <p>
              {String(e.kind)} · {String(e.status)} · edition{" "}
              {String(e.edition)} · {c.currency} · principal {s.principal}
            </p>
            {e.activation_journal_id && (
              <RecordLink
                href={canonicalUiHref(`${base}/finance/journals/${e.activation_journal_id}`)}
              >
                Recognition journal
              </RecordLink>
            )}
            <details>
              <summary>Retained schedule</summary>
              <Table>
                <thead>
                  <tr>
                    <th>Due</th>
                    <th>Cash</th>
                    <th>Interest</th>
                    <th>Principal</th>
                    <th>Balance</th>
                    <th>ROU depreciation</th>
                  </tr>
                </thead>
                <tbody>
                  {s.rows.map((r) => (
                    <tr key={r.ordinal}>
                      <td>{r.dueDate}</td>
                      <td>{r.payment}</td>
                      <td>{r.interest}</td>
                      <td>{r.principal}</td>
                      <td>{r.closing}</td>
                      <td>{r.depreciation}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </details>
            <FinanceExpansionDecision
              base={base} canWrite={canWrite}
              row={e}
              root="instruments"
              approveAction="approve"
              canApprove={canApprove}
              userId={userId}
              busy={busy || (!canSettle && !hasAttempt)}
              run={run}
            />
            {enabled && canWrite && e.status === "active" && (
              <>
                <FormDisclosure error={error}
                  appearance="standard"
                  drawerTitle="Prepare bank-backed instalment"
                >
                  <summary>Prepare instalment or payoff</summary>
                  <FormLayout data-finance-draft={draftScope}
                    className="ui-form-grid"
                    onSubmit={async (ev) => {
                      ev.preventDefault();
                      const f = new FormData(ev.currentTarget),
                        b = data.bank.find(
                          (b) => b.id === f.get("bankTransactionId"),
                        );
                      await run(...financeDraftCommand(`instruments/${e.id}/runs`, "instrument_payment", e.id, f, {
                        ...Object.fromEntries(f),
                        expectedVersion: e.version,
                        bankVersion: b?.version,
                      }));
                    }}
                  >
                    <Field appearance="standard">
                      Payment type
                      <Select appearance="standard" name="kind">
                        <option value="payment">Scheduled instalment</option>
                        {e.kind === "loan" && (
                          <option value="payoff">Full early payoff</option>
                        )}
                      </Select>
                    </Field>
                    <FinancePrepareOn/>
                    <Field appearance="standard">
                      Open period
                      <Select appearance="standard" name="periodId" required>
                        {data.periods.map((p) => (
                          <option key={String(p.id)} value={String(p.id)}>
                            {String(p.name)}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Actual bank date
                      <TextInput
                        appearance="standard"
                        type="date"
                        name="entryDate"
                        required
                      />
                    </Field>
                    <Field appearance="standard">
                      Actual imported debit
                      <Select
                        appearance="standard"
                        name="bankTransactionId"
                        required
                      >
                        <option value="">
                          Choose exact statement movement
                        </option>
                        {data.bank
                          .filter(
                            (b) =>
                              Number(b.amount_minor) < 0 &&
                              b.bank_account === c.bankAccountId,
                          )
                          .map((b) => (
                            <option key={String(b.id)} value={String(b.id)}>
                              {String(b.transaction_date)} ·{" "}
                              {minorToDecimal(Number(b.amount_minor))} ·{" "}
                              {String(b.external_id)}
                            </option>
                          ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Evidence and reason
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
                      Prepare payment draft
                    </Button>
                  </FormLayout>
                </FormDisclosure>
                {e.kind === "lease" && (
                  <FormDisclosure error={error}
                    appearance="standard"
                    drawerTitle="Remeasure lease"
                  >
                    <summary>Prepare lease amendment</summary>
                    <InstrumentEditor
                      initial={e}
                      data={data}
                      busy={busy || (!canPrepare && !hasAttempt)}
                      onSave={(i) => run("instruments", i)}
                    />
                  </FormDisclosure>
                )}
              </>
            )}
          </Card>
        );
      })}
      <h2>Payment and correction evidence</h2>
      {data.runs.map((r) => {
        const e = data.editions.find((e) => e.id === r.instrument_id),
          result = JSON.parse(String(r.result_json)) as {
            payment: string;
            interest: string;
            principal: string;
            closing: string;
          };
        return (
          <Card
            key={String(r.id)}
            appearance="standard"
            as="section"
            className="ui-panel"
          >
            <b>
              <RecordLink href={recordCanonicalHref(base,{kind:"finance_instrument_run",id:String(r.id)})}>{String(e?.number ?? "")} · instalment {String(r.ordinal)} · {String(r.status)}</RecordLink>
            </b>
            <p>
              Cash {result.payment} · interest {result.interest} · principal{" "}
              {result.principal} · remaining {result.closing}
            </p>
            {r.journal_id && (
              <RecordLink
                href={canonicalUiHref(`${base}/finance/journals/${r.journal_id}`)}
              >
                Posting journal
              </RecordLink>
            )}
            {r.reversal_journal_id && (
              <RecordLink
                href={canonicalUiHref(`${base}/finance/journals/${r.reversal_journal_id}`)}
              >
                Retained reversal
              </RecordLink>
            )}
            <FinanceExpansionDecision
              base={base} canWrite={canWrite}
              row={r}
              root="instrument-runs"
              approveAction="post"
              canApprove={canApprove}
              userId={userId}
              busy={busy || (!canSettle && !hasAttempt)}
              run={run}
              periods={data.periods}
            />
          </Card>
        );
      })}
    </>
  );
}
