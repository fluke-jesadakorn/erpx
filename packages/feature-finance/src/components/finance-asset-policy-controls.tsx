"use client";
import {useFinanceCommand} from "@erpx/feature-finance/components/use-finance-command";
import {useFinanceDraftScope} from "@erpx/feature-finance/components/finance-draft-boundary";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { EmbeddedRecord } from "@erpx/ui/components/record-context";
import { recordCanonicalHref } from "@erpx/feature-records/lib/record-drawer";
import { useContext, useState } from "react";
import {
  Button,
  Card,
  Field,
  FormLayout,
  InlineNotice,
  Select,
  TextArea,
  TextInput,
} from "@erpx/ui/components/foundation";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { FinanceExpansionDecision } from "@erpx/feature-finance/components/finance-expansion-decision";
import { FinancePrepareOn, financeDraftCommand } from "@erpx/feature-finance/components/finance-prepare-on";
import {
  monthlyDate,
  type DepreciationPolicy,
} from "@erpx/server-finance/public/rules/finance-instrument-policy";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { RecordLink } from "@erpx/feature-records/components/record-link";
type Row = Record<string, string | number | null>;
type Data = {
  policies: Row[];
  runs: Row[];
  assets: Row[];
  books: Row[];
  periods: Row[];
};
type Policy = {
  assetId: string;
  books: Array<{ chartId: string; policy: DepreciationPolicy }>;
  dimensions: Record<string, string>;
};
function BookMethod({
  book,
  initial,
}: {
  book: Row;
  initial?: DepreciationPolicy;
}) {
  const [method, setMethod] = useState(initial?.method ?? "straight_line"),
    name = `book:${book.id}`;
  return (
    <fieldset>
      <legend>{String(book.name)}</legend>
      <Field appearance="standard">
        Depreciation method
        <Select
          appearance="standard"
          name={name}
          value={method}
          onChange={(e) =>
            setMethod(e.target.value as DepreciationPolicy["method"])
          }
        >
          <option value="straight_line">Straight line</option>
          <option value="declining_balance">Declining balance</option>
          <option value="units_of_production">Units of production</option>
        </Select>
      </Field>
      {method === "declining_balance" && (
        <Field appearance="standard">
          Annual depreciation rate (%)
          <TextInput
            appearance="standard"
            name={`${name}:rate`}
            type="number"
            min={0.01}
            max={100}
            step={0.01}
            required
            defaultValue={
              initial?.method === "declining_balance"
                ? initial.annualRateBps / 100
                : 20
            }
          />
        </Field>
      )}
      {method === "units_of_production" && (
        <Field appearance="standard">
          Reviewed lifetime units
          <TextInput
            appearance="standard"
            name={`${name}:units`}
            type="number"
            min={1}
            max={1000000000}
            required
            defaultValue={
              initial?.method === "units_of_production"
                ? initial.totalUnits
                : undefined
            }
          />
        </Field>
      )}
    </fieldset>
  );
}
function PolicyEditor({
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
      ? (JSON.parse(String(initial.content_json)) as Policy)
      : null,
    [id, setId] = useState(old?.assetId ?? String(data.assets[0]?.id ?? "")),
    asset = data.assets.find((a) => a.id === id),
    books = data.books.filter(
      (b) => b.id === asset?.chart_id || b.primary_chart_id === asset?.chart_id,
    );
  return (
    <FormLayout data-finance-draft={draftScope}
      className="ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        await onSave({
          assetId: id,
          expectedVersion: asset?.version,
          parentId: initial?.id,
          parentVersion: initial?.version,
          effectiveDate: String(f.get("effectiveDate") ?? "") || undefined,
          adoptLegacy: f.get("adoptLegacy") === "on",
          books: books.map((b) => {
            const name = `book:${b.id}`,
              method = String(f.get(name));
            return {
              chartId: b.id,
              policy:
                method === "declining_balance"
                  ? {
                      method,
                      annualRateBps: Math.round(
                        Number(f.get(`${name}:rate`)) * 100,
                      ),
                    }
                  : method === "units_of_production"
                    ? { method, totalUnits: Number(f.get(`${name}:units`)) }
                    : { method },
            };
          }),
          dimensions: Object.fromEntries(
            String(f.get("dimensions") ?? "")
              .split("\n")
              .filter(Boolean)
              .map((l) => l.split("=")),
          ),
          reason: f.get("reason"),
        });
      }}
    >
      <Field appearance="standard">
        Recognized asset
        <Select
          appearance="standard"
          value={id}
          onChange={(e) => setId(e.target.value)}
          disabled={!!initial}
          required
        >
          <option value="">Choose asset</option>
          {data.assets.map((a) => (
            <option key={String(a.id)} value={String(a.id)}>
              {String(a.asset_number)} · {String(a.name)}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance="standard">
        First date under this policy (optional)
        <TextInput appearance="standard" name="effectiveDate" type="date" />
        <span className="muted">
          Defaults to the next unposted asset month. Use the opening date when
          enabling another book.
        </span>
      </Field>
      {!initial && Number(asset?.accumulated_depreciation_minor ?? 0) > 0 && (
        <Field appearance="standard">
          <label>
            <input type="checkbox" name="adoptLegacy" required /> Adopt the
            retained depreciation history
          </label>
          <span className="muted">
            Primary book accumulated depreciation:{" "}
            {minorToDecimal(Number(asset?.accumulated_depreciation_minor ?? 0))}
            . The reviewer receives the original journals and each book’s
            carrying basis. Posted history stays intact.
          </span>
        </Field>
      )}
      {books.map((b) => (
        <BookMethod
          key={`${id}:${b.id}`}
          book={b}
          initial={old?.books.find((p) => p.chartId === b.id)?.policy}
        />
      ))}
      <details>
        <summary>Reporting dimensions and policy convention</summary>
        <p className="muted">
          Straight line assigns the final cent in the last full month. Declining
          balance caps depreciation at the residual value and settles the
          remainder in the final month. Units of production uses cumulative
          reviewed readings. Changes allocate the remaining carrying value
          prospectively. Every enabled book has its own reviewed method.
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
        Policy evidence and reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button
        appearance="standard"
        clientManaged
        type="submit"
        disabled={busy || !asset}
      >
        Prepare policy review
      </Button>
    </FormLayout>
  );
}
function PolicyDisclosure({data,initial,busy,onSave,error}:{data:Data;initial?:Row;busy:boolean;onSave:(input:unknown)=>Promise<void>;error:string}){
 const [revision,setRevision]=useState(0);
 return <FormDisclosure appearance="standard" error={error} drawerTitle={initial?"Prepare revised asset policies":"Prepare asset book policies"} onDiscard={()=>setRevision(value=>value+1)}><summary>{initial?"Revise policy":"Prepare depreciation policies"}</summary><PolicyEditor key={revision} data={data} initial={initial} busy={busy} onSave={onSave}/></FormDisclosure>;
}
export function FinanceAssetPolicyControls({
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
  const recordOnly = useContext(EmbeddedRecord);
  const {send,busy,error,draftScope,canPrepare,canSettle}=useFinanceCommand(base);
  async function run(path:string,input:unknown){await send(path,input,path==="asset-policies"?"new-work":"settle-existing");}
  return (
    <>
      <Card appearance="standard" as="section" className="ui-panel">
        <h2>Depreciation by book</h2>
        <p className="muted">
          Prepare one policy per enabled book. An independent reviewer posts
          monthly depreciation and retains any correction.
        </p>
        {!enabled && (
          <p className="muted">
            This Company&apos;s tool is off. Enable it in Finance setup after
            its acceptance checks.
          </p>
        )}
        {error && <InlineNotice tone="danger">{error}</InlineNotice>}
        {!recordOnly && enabled && canWrite && canPrepare && (
          <PolicyDisclosure data={data} busy={busy} error={error} onSave={i=>run("asset-policies",i)}/>
        )}
      </Card>
      {data.policies.map((p) => {
        const c = JSON.parse(String(p.content_json)) as Policy,
          a = data.assets.find((a) => a.id === p.asset_id),
          usage = c.books.some(
            (b) => b.policy.method === "units_of_production",
          );
        return (
          <Card
            key={String(p.id)}
            appearance="standard"
            as="section"
            className="ui-panel"
          >
            <h2>
              <RecordLink
                href={recordCanonicalHref(base, {
                  kind: "finance_asset_policy",
                  id: String(p.id),
                })}
              >
                {String(a?.asset_number ?? p.asset_id)} · {String(p.status)}
              </RecordLink>
            </h2>
            {c.books.map((b) => (
              <p key={b.chartId}>
                {String(
                  data.books.find((x) => x.id === b.chartId)?.name ?? b.chartId,
                )}{" "}
                · {b.policy.method.replaceAll("_", " ")}
              </p>
            ))}
            {p.basis_json && (
              <details>
                <summary>Retained carrying basis</summary>
                {(() => {
                  const basis = JSON.parse(String(p.basis_json)) as {
                    ordinal: number;
                    throughDate: string;
                    books: Array<{
                      chartId: string;
                      accumulated: string;
                      units: number;
                    }>;
                  };
                  return (
                    <>
                      <p>
                        {basis.ordinal} completed asset months · through{" "}
                        {basis.throughDate}
                      </p>
                      {basis.books.map((b) => (
                        <p key={b.chartId}>
                          {String(
                            data.books.find((x) => x.id === b.chartId)?.name ??
                              b.chartId,
                          )}{" "}
                          · accumulated depreciation {b.accumulated} · retained
                          usage {b.units}
                        </p>
                      ))}
                      {a && (
                        <RecordLink href={canonicalUiHref(`${base}/finance/assets/${a.id}`)}>
                          Original acquisition and depreciation history
                        </RecordLink>
                      )}
                    </>
                  );
                })()}
              </details>
            )}
            <FinanceExpansionDecision
              base={base}
              canWrite={canWrite&&canSettle}
              row={p}
              root="asset-policies"
              approveAction="approve"
              canApprove={canApprove&&canSettle}
              userId={userId}
              busy={busy}
              run={run}
            />
            {enabled && canWrite && canSettle && p.status === "active" && (
              <>
                <FormDisclosure error={error}
                  appearance="standard"
                  drawerTitle="Prepare monthly depreciation"
                >
                  <summary>Prepare monthly depreciation</summary>
                  <FormLayout data-finance-draft={draftScope}
                    className="ui-form-grid"
                    onSubmit={async (e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget);
                      await run(
                        ...financeDraftCommand(
                          `asset-policies/${p.id}/runs`,
                          "asset_depreciation",
                          p.id,
                          f,
                          {
                            ...Object.fromEntries(f),
                            expectedVersion: p.version,
                            units: Number(f.get("units") ?? 0),
                          },
                        ),
                      );
                    }}
                  >
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
                    <FinancePrepareOn />
                    <Field appearance="standard">
                      Asset service month
                      <TextInput
                        appearance="standard"
                        name="depreciationMonth"
                        type="month"
                        required
                        defaultValue={
                          a
                            ? monthlyDate(
                                String(a.in_service_date),
                                Number(a.completed_depreciation_months ?? 0),
                              ).slice(0, 7)
                            : undefined
                        }
                      />
                      <span className="muted">
                        For a correction to a closed month, retain the service
                        month and select a current open accounting date.
                      </span>
                    </Field>
                    <Field appearance="standard">
                      Accounting date
                      <TextInput
                        appearance="standard"
                        name="entryDate"
                        type="date"
                        required
                      />
                    </Field>
                    {usage && (
                      <>
                        <Field appearance="standard">
                          Usage this month
                          <TextInput
                            appearance="standard"
                            name="units"
                            type="number"
                            min={0}
                            required
                          />
                        </Field>
                        <Field appearance="standard">
                          Reviewed reading evidence, including zero usage
                          <TextArea
                            appearance="standard"
                            name="usageEvidence"
                            required
                            minLength={3}
                          />
                        </Field>
                      </>
                    )}
                    <Field appearance="standard">
                      Preparation reason
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
                      Prepare depreciation draft
                    </Button>
                  </FormLayout>
                </FormDisclosure>
                {canPrepare&&<PolicyDisclosure initial={p} data={data} busy={busy} error={error} onSave={i=>run("asset-policies",i)}/>}
              </>
            )}
          </Card>
        );
      })}
      <h2>Retained depreciation and correction evidence</h2>
      {data.runs.map((r) => {
        const result = JSON.parse(String(r.result_json)) as {
            books: Array<{
              chartId: string;
              amount: string;
              accumulated: string;
            }>;
          },
          journals = JSON.parse(String(r.journal_ids_json)) as Array<{
            id: string;
          }>,
          reversals = JSON.parse(String(r.reversal_ids_json)) as string[];
        return (
          <Card
            key={String(r.id)}
            appearance="standard"
            as="section"
            className="ui-panel"
          >
            <b>
              <RecordLink
                href={recordCanonicalHref(base, {
                  kind: "finance_asset_policy_run",
                  id: String(r.id),
                })}
              >
                {String(r.entry_date)} · month {String(r.ordinal)} ·{" "}
                {String(r.status)}
              </RecordLink>
            </b>
            {result.books.map((b) => (
              <p key={b.chartId}>
                {String(
                  data.books.find((x) => x.id === b.chartId)?.name ?? b.chartId,
                )}{" "}
                · depreciation {b.amount} · accumulated {b.accumulated}
              </p>
            ))}
            {journals.map((j) => (
              <RecordLink key={j.id} href={canonicalUiHref(`${base}/finance/journals/${j.id}`)}>
                Posting journal
              </RecordLink>
            ))}
            {reversals.map((id) => (
              <RecordLink key={id} href={canonicalUiHref(`${base}/finance/journals/${id}`)}>
                Retained reversal
              </RecordLink>
            ))}
            <FinanceExpansionDecision
              base={base}
              canWrite={canWrite&&canSettle}
              row={r}
              root="asset-policy-runs"
              approveAction="post"
              canApprove={canApprove&&canSettle}
              userId={userId}
              busy={busy}
              run={run}
              periods={data.periods}
            />
          </Card>
        );
      })}
    </>
  );
}
