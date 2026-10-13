"use client";
import {FinanceEvidenceDocuments} from "./finance-evidence-documents";
import {EmbeddedRecord} from "@/components/records/record-context";
import {RecordLink} from "@/components/records/record-link";
import {recordCanonicalHref} from "@/lib/records/record-drawer";
import { useContext,useState } from "react";
import { useRouter } from "next/navigation";
import { financeCommandFetch as workFetch } from "@/lib/finance/finance-command-fetch";
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
type Data = {
  definitions: Row[];
  values: Row[];
  rules: Row[];
  mappings: Row[];
  charts: Row[];
  accounts: Row[];
  periods: Row[];
};
export function FinanceBookControls({
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
      );
      const body = await r.json();
      if (!r.ok)
        throw new Error(body.error?.message ?? "Finance action failed");
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Finance action failed");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card appearance="standard" as="section" className="ui-panel">
      <h2>Books and reporting dimensions</h2>
      <p className="muted">
        Map the secondary book, check its opening balances, then have another
        Finance approver enable it. Select the Company dimensions used by your
        reports.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert">
          {error}
        </InlineNotice>
      )}
      {canWrite && (
        (!recordOnly && <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Add a reporting dimension"
        >
          <summary>Add dimension</summary>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void run("dimensions", {
                code: f.get("code"),
                name: f.get("name"),
                required: f.get("required") === "on",
                values: String(f.get("values"))
                  .split(/\r?\n/)
                  .filter((s) => s.trim())
                  .map((s) => {
                    const [code, ...name] = s.split(",");
                    return {
                      code: code.trim(),
                      name: name.join(",").trim() || code.trim(),
                    };
                  }),
              });
            }}
          >
            <Field appearance="standard">
              Code
              <TextInput
                appearance="standard"
                name="code"
                placeholder="project"
                required
                pattern="[a-z][a-z0-9_]{0,39}"
              />
            </Field>
            <Field appearance="standard">
              Name
              <TextInput
                appearance="standard"
                name="name"
                placeholder="Project"
                required
              />
            </Field>
            <Field appearance="standard">
              Values: code, name
              <TextArea
                appearance="standard"
                name="values"
                required
                placeholder={"alpha, Alpha project\nbeta, Beta project"}
              />
            </Field>
            <label>
              <input type="checkbox" name="required" /> Require on new postings
            </label>
            <Button
              appearance="standard"
              clientManaged
              type="submit"
              disabled={busy}
            >
              Save dimension
            </Button>
          </FormLayout>
        </FormDisclosure>)
      )}
      <ul>
        {data.definitions.map((d) => (
          <li key={String(d.id)}>
            <b>{String(d.name)}</b> ·{" "}
            {Number(d.required) ? "Required" : "Optional"} ·{" "}
            {data.values
              .filter((v) => v.dimension_id === d.id)
              .map((v) => String(v.name))
              .join(", ")}
          </li>
        ))}
      </ul>
      {canWrite && (
        (!recordOnly && <FormDisclosure error={error}
          appearance="standard"
          drawerTitle="Prepare a secondary book"
        >
          <summary>Prepare book mapping</summary>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                primaryChartId = String(f.get("primaryChartId")),
                secondaryChartId = String(f.get("secondaryChartId"));
              const pairs = String(f.get("mappings"))
                .split(/\r?\n/)
                .filter((s) => s.trim())
                .map((s) => s.split(",").map((s) => s.trim()));
              const mappings = pairs.map(([a, b]) => ({
                primaryAccountId: String(
                  data.accounts.find(
                    (r) => r.chart_id === primaryChartId && r.code === a,
                  )?.id ?? "",
                ),
                secondaryAccountId: String(
                  data.accounts.find(
                    (r) => r.chart_id === secondaryChartId && r.code === b,
                  )?.id ?? "",
                ),
              }));
              void run("book-mappings", {
                primaryChartId,
                secondaryChartId,
                effectiveDate: f.get("effectiveDate"),
                openingPeriodId: f.get("openingPeriodId"),
                reason: f.get("reason"),
                mappings,
              });
            }}
          >
            {["primaryChartId", "secondaryChartId"].map((name, i) => (
              <Field appearance="standard" key={name}>
                {i ? "Secondary book" : "Primary book"}
                <Select appearance="standard" name={name} required>
                  <option value="">Select book</option>
                  {data.charts.map((c) => (
                    <option key={String(c.id)} value={String(c.id)}>
                      {String(c.name)} · {String(c.base_currency)}
                    </option>
                  ))}
                </Select>
              </Field>
            ))}
            <Field appearance="standard">
              Opening and effective date
              <TextInput
                appearance="standard"
                name="effectiveDate"
                type="date"
                required
              />
            </Field>
            <Field appearance="standard">
              Open period
              <Select appearance="standard" name="openingPeriodId" required>
                {data.periods.map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Account mapping: primary code, secondary code
              <TextArea
                appearance="standard"
                name="mappings"
                rows={8}
                required
                placeholder={"1000, 1000\n2000, 2000"}
              />
            </Field>
            <Field appearance="standard">
              Opening and policy evidence
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
              Prepare for review
            </Button>
          </FormLayout>
        </FormDisclosure>)
      )}
      <TableFrame appearance="standard" label="Book rules">
        <Table>
          <thead>
            <tr>
              <th>Books</th>
              <th>Effective</th>
              <th>Status</th>
              <th>Opening / review</th>
            </tr>
          </thead>
          <tbody>
            {data.rules.map((r) => (
              <tr key={String(r.id)}>
                <td>
                  <RecordLink href={recordCanonicalHref(base,{kind:"finance_book",id:String(r.id)})}>{String(r.primary_name)} → {String(r.secondary_name)}</RecordLink>
                </td>
                <td>{String(r.effective_date)}</td>
                <td>{String(r.status)}</td>
                <td>
                  <details>
                    <summary>Opening balances and mappings</summary>
                    <ul>
                      {(
                        JSON.parse(String(r.opening_json)) as {
                          accountId: string;
                          minor: number;
                        }[]
                      ).map((l, i) => (
                        <li key={i}>
                          {String(
                            data.accounts.find((a) => a.id === l.accountId)
                              ?.name ?? l.accountId,
                          )}{" "}
                          · {(l.minor / 100).toFixed(2)}
                        </li>
                      ))}
                    </ul>
                    <ul>
                      {data.mappings
                        .filter((m) => m.rule_id === r.id)
                        .map((m) => (
                          <li key={String(m.primary_account_id)}>
                            {String(
                              data.accounts.find(
                                (a) => a.id === m.primary_account_id,
                              )?.code,
                            )}{" "}
                            →{" "}
                            {String(
                              data.accounts.find(
                                (a) => a.id === m.secondary_account_id,
                              )?.code,
                            )}
                          </li>
                        ))}
                    </ul>
                    <p>{String(r.reason)}</p>
                  </details>
                  <FinanceEvidenceDocuments base={base} row={r} targetType="finance_book_rule" canWrite={canWrite}/>
                  {canApprove &&
                    r.status === "draft" &&
                    r.prepared_by !== userId && (
                      <FormLayout
                        onSubmit={(e) => {
                          e.preventDefault();
                          const f = new FormData(e.currentTarget);
                          void run(`book-mappings/${r.id}/${f.get("action")}`, {
                            expectedVersion: Number(r.version),
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
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableFrame>
    </Card>
  );
}
