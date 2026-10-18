"use client";
import { useFinanceActionAllowed } from "./purchase-context";
import { useFinanceDraftScope, captureFinanceForm, acknowledgeFinanceForm } from "./finance-draft-boundary";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink } from "@/components/records/record-link";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import {
  Button,
  Field,
  Select,
  Table,
  TableFrame,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useMemo, useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { Stack, Card, FormLayout } from "@/components/ui/foundation";

export type FinanceChart = {
  id: string | number;
  name: string | number;
  base_currency: string | number;
  accounting_basis?: string | number;
};
export type FinanceAccount = {
  id: string | number;
  code: string | number;
  name: string | number;
  category: string | number;
  currency?: string | number | null;
  control_kind?: string | number | null;
  is_control?: string | number | null;
  status?: string | number | null;
  version?: string | number | null;
};
type CsvRow = {
  code: string;
  name: string;
  category: string;
  currency?: string;
  controlKind?: string;
};
const categories = [
  "asset",
  "liability",
  "equity",
  "revenue",
  "expense",
  "other_income",
  "other_expense",
] as const;
const controls = ["none", "receivable", "payable", "bank"] as const;

function useFinanceSetupCommand(base: string) {
  const allowedNew = useFinanceActionAllowed("new-work"), allowedSettle = useFinanceActionAllowed("settle-existing");
  const [hasAttempt, setHasAttempt] = useState(false);
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const retryKey = useRef<{ fingerprint: string; value: string } | null>(null);
  async function send(
    path: string,
    input: unknown,
    redirect?: string | ((data: { id?: string | number }) => string),
  ) {
    const fingerprint = `${path}:${JSON.stringify(input)}`;
    const settlement = path.endsWith("/status") && (input as {status?:unknown}).status === "inactive";
    if (!(settlement ? allowedSettle : allowedNew) && retryKey.current?.fingerprint !== fingerprint) { setError("This Finance action is unavailable. Your draft is retained."); return null; }
    const completion = captureFinanceForm();
    setPending(true); setError("");
    try {
      const key =
        retryKey.current?.fingerprint === fingerprint
          ? retryKey.current.value
          : crypto.randomUUID();
      retryKey.current = { fingerprint, value: key };
      setHasAttempt(true);
      const response = await workFetch(
        `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/${path}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": key },
          body: JSON.stringify(input),
        },
      );
      const body = (await response.json()) as {
        data?: { id?: string | number };
        error?: { message?: string };
      };
      if (!response.ok || !body.data || typeof body.data !== "object") throw new Error(body.error?.message ?? "Unable to save finance setup.");
      acknowledgeFinanceForm(completion); retryKey.current = null; setHasAttempt(false);
      if (redirect)
        router.push(typeof redirect === "function" ? redirect(body.data ?? {}) : redirect);
      else router.refresh();
      return body.data;
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Unable to save finance setup.");
      return null;
    } finally {
      setPending(false);
    }
  }
  return { pending, error, send, canCreate: allowedNew || hasAttempt, canSettle: allowedSettle || hasAttempt };
}

function parseCsvLine(line: string): { cells: string[]; error?: string } {
  const cells: string[] = [];
  let current = "";
  let quoted = false;
  let afterQuote = false;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quoted) {
      if (character === '"') {
        if (line[index + 1] === '"') {
          current += '"';
          index += 1;
        } else {
          quoted = false;
          afterQuote = true;
        }
      } else current += character;
    } else if (afterQuote) {
      if (character === ",") {
        cells.push(current.trim());
        current = "";
        afterQuote = false;
      } else if (!/\s/.test(character))
        return { cells, error: "Unexpected content after a quoted value." };
    } else if (character === ",") {
      cells.push(current.trim());
      current = "";
    } else if (character === '"' && current.trim() === "") {
      current = "";
      quoted = true;
    } else current += character;
  }
  if (quoted) return { cells, error: "Unclosed quoted value." };
  cells.push(current.trim());
  return { cells };
}

function parseCsv(value: string): { rows: CsvRow[]; errors: string[] } {
  const lines = value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) return { rows: [], errors: [] };
  const headerLine = parseCsvLine(lines.shift()!);
  const header = headerLine.cells.map((cell) => cell.toLowerCase());
  const required = ["code", "name", "category"];
  const errors = [
    ...(headerLine.error ? [`Header: ${headerLine.error}`] : []),
    ...required
      .filter((field) => !header.includes(field))
      .map((field) => `Missing required column: ${field}`),
  ];
  if (errors.length) return { rows: [], errors };
  const rows: CsvRow[] = [];
  lines.forEach((line, index) => {
    const parsedLine = parseCsvLine(line);
    if (parsedLine.error) {
      errors.push(`Row ${index + 2}: ${parsedLine.error}`);
      return;
    }
    const cells = parsedLine.cells;
    if (cells.length !== header.length) {
      errors.push(`Row ${index + 2}: expected ${header.length} column(s), found ${cells.length}.`);
      return;
    }
    const row = Object.fromEntries(header.map((key, column) => [key, cells[column] ?? ""]));
    const category = String(row.category ?? "");
    const controlKind = String(row.controlkind ?? row.control_kind ?? "none");
    if (!row.code || !row.name) errors.push(`Row ${index + 2}: code and name are required.`);
    if (!categories.includes(category as (typeof categories)[number]))
      errors.push(`Row ${index + 2}: invalid category ${category || "(empty)"}.`);
    if (!controls.includes(controlKind as (typeof controls)[number]))
      errors.push(`Row ${index + 2}: invalid controlKind ${controlKind}.`);
    rows.push({
      code: String(row.code ?? ""),
      name: String(row.name ?? ""),
      category,
      currency: row.currency ? String(row.currency).toUpperCase() : undefined,
      controlKind,
    });
  });
  return { rows, errors };
}

export function FinanceChartControls({
  base,
  chart,
  charts,
  accounts,
}: {
  base: string;
  chart?: FinanceChart;
  charts: FinanceChart[];
  accounts: FinanceAccount[];
}) {
  const { pending, error, send, canCreate, canSettle } = useFinanceSetupCommand(base);
  const draftScope = useFinanceDraftScope();
  const [csv, setCsv] = useState(
    `code,name,category,currency,controlKind\n1100,Operating bank,asset,${chart?.base_currency ?? ""},bank\n1200,Trade receivables,asset,${chart?.base_currency ?? ""},receivable`,
  );
  const parsed = useMemo(() => parseCsv(csv), [csv]);
  return (
    <Stack as="section" className="ui-stack" aria-label="Chart of accounts management">
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <div className="ui-section-header">
          <div>
            <h2>Charts of accounts</h2>
            <p className="muted">
              Create a separate reporting boundary for a company or accounting basis. Existing
              charts and posted journals remain unchanged.
            </p>
          </div>
        </div>
        <FormDisclosure error={error} appearance="standard">
          <summary>Create a chart</summary>
          <FormLayout data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void send(
                "charts",
                {
                  name: form.get("name"),
                  baseCurrency: form.get("baseCurrency"),
                  accountingBasis: form.get("accountingBasis"),
                },
                (data) =>
                  data.id
                    ? canonicalUiHref(`${base}/finance/setup?step=chart&chart=${encodeURIComponent(String(data.id))}`)
                    : canonicalUiHref(`${base}/finance/setup?step=chart`),
              );
            }}
          >
            <Field appearance="standard">
              Chart name
              <TextInput appearance="standard" name="name" required maxLength={160} placeholder="Operations ledger" />
            </Field>
            <Field appearance="standard">
              Base currency
              <TextInput appearance="standard"
                name="baseCurrency"
                required
                minLength={3}
                maxLength={3}
                defaultValue={String(chart?.base_currency ?? "")}
                placeholder="Choose a currency code"
                pattern="[A-Z]{3}"
              />
            </Field>
            <Field appearance="standard">
              Accounting basis
              <TextInput appearance="standard"
                name="accountingBasis"
                required
                maxLength={40}
                defaultValue={String(chart?.accounting_basis ?? "")}
                placeholder="Confirmed Company accounting basis"
              />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || !canCreate}>
              {pending ? "Saving…" : "Create chart"}
            </Button>
          </FormLayout>
        </FormDisclosure>
        <ul>
          {charts.map((item) => (
            <li key={String(item.id)}>
              {String(item.name)} · {String(item.base_currency)}
              {item.id === chart?.id ? " · selected" : ""}
            </li>
          ))}
        </ul>
      </Card>
      {chart && (
        <>
          <Card as="section" appearance="standard" className="ui-panel">
            <div className="ui-section-header">
              <div>
                <h2>Accounts · {String(chart.name)}</h2>
                <p className="muted">
                  Control accounts drive receivables, payables, bank reconciliation and opening
                  schedules.
                </p>
              </div>
            </div>
            <FormDisclosure error={error} appearance="standard">
              <summary>Add an account</summary>
              <FormLayout data-finance-draft={draftScope}
                className="ui-form-grid"
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget);
                  void send(`charts/${encodeURIComponent(String(chart.id))}/accounts`, {
                    code: form.get("code"),
                    name: form.get("name"),
                    category: form.get("category"),
                    currency: form.get("currency"),
                    controlKind: form.get("controlKind"),
                    isControl: form.get("controlKind") !== "none",
                  });
                }}
              >
                <Field appearance="standard">
                  Code
                  <TextInput appearance="standard" name="code" required maxLength={40} placeholder="1100" />
                </Field>
                <Field appearance="standard">
                  Name
                  <TextInput appearance="standard" name="name" required maxLength={160} placeholder="Operating bank" />
                </Field>
                <Field appearance="standard">
                  Category
                  <Select appearance="standard" name="category" defaultValue="asset">
                    {categories.map((value) => (
                      <option key={value} value={value}>
                        {value}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Currency
                  <TextInput appearance="standard"
                    name="currency"
                    required
                    minLength={3}
                    maxLength={3}
                    defaultValue={String(chart.base_currency)}
                  />
                </Field>
                <Field appearance="standard">
                  Control role
                  <Select appearance="standard" name="controlKind" defaultValue="none">
                    {controls.map((value) => (
                      <option key={value} value={value}>
                        {value === "none" ? "None" : value}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || !canCreate}>
                  {pending ? "Saving…" : "Add account"}
                </Button>
              </FormLayout>
            </FormDisclosure>
            <TableFrame appearance="standard" label="Records" className="business-table-wrap">
              <Table className="business-table">
                <thead>
                  <tr>
                    <th>Code</th>
                    <th>Name</th>
                    <th>Category</th>
                    <th>Currency</th>
                    <th>Control role</th>
                    <th>Status</th>
                    <th>Change status</th>
                  </tr>
                </thead>
                <tbody>
                  {accounts.map((account) => {
                    const status = String(account.status ?? "active");
                    const nextStatus = status === "active" ? "inactive" : "active";
                    return (
                      <tr key={String(account.id)}>
                        <td><RecordLink href={canonicalUiHref(`${base}/record-details/finance_account/${encodeURIComponent(String(account.id))}`)}>{String(account.code)}</RecordLink></td>
                        <td>{String(account.name)}</td>
                        <td>{String(account.category)}</td>
                        <td>{String(account.currency ?? chart.base_currency)}</td>
                        <td>{String(account.control_kind ?? "none")}</td>
                        <td>{status}</td>
                        <td>
                          <details>
                            <summary>Change status</summary>
                            <form data-finance-draft={draftScope}
                              className="ui-form-actions"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const form = new FormData(event.currentTarget);
                                void send(
                                  `charts/${encodeURIComponent(String(chart.id))}/accounts/${encodeURIComponent(String(account.id))}/status`,
                                  {
                                    status: nextStatus,
                                    expectedVersion: Number(account.version ?? 1),
                                    reason: form.get("reason"),
                                  },
                                );
                              }}
                            >
                              <TextInput appearance="standard"
                                name="reason"
                                required
                                minLength={3}
                                maxLength={2000}
                                placeholder="Reason"
                                aria-label={`Reason for ${String(account.code)}`}
                              />
                              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={pending || !(nextStatus === "active" ? canCreate : canSettle)}>
                                {pending
                                  ? "Saving…"
                                  : nextStatus === "active"
                                    ? "Activate"
                                    : "Deactivate"}
                              </Button>
                            </form>
                          </details>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </Table>
            </TableFrame>
          </Card>
          <FormDisclosure error={error} appearance="standard" className="ui-panel" drawerTitle="Import account list">
            <summary>Import account list</summary>
            <FormLayout data-finance-draft={draftScope} layout="stack" onSubmit={event => {
              event.preventDefault();
              void send("charts/import", {
                chartId: String(chart.id),
                rows: parsed.rows.map(row => ({...row, controlKind: row.controlKind || "none"})),
              });
            }}>
            <div className="ui-section-header">
              <div>
                <p className="muted">
                  Preview row-level validation before an idempotent upsert. Required columns: code,
                  name, category. Optional: currency, controlKind.
                </p>
              </div>
            </div>
            <Field appearance="standard">
              CSV account list
              <TextArea appearance="standard"
                value={csv}
                onChange={(event) => setCsv(event.target.value)}
                rows={7}
                spellCheck={false}
              />
            </Field>
            {parsed.errors.length ? (
              <ul className="error" role="alert">
                {parsed.errors.map((message) => (
                  <li key={message}>{message}</li>
                ))}
              </ul>
            ) : (
              <p className="muted">Preview: {parsed.rows.length} valid row(s) ready to import.</p>
            )}
            <Button appearance="standard"
              clientManaged
              type="submit"
              variant="primary"
              disabled={pending || !canCreate || !parsed.rows.length || parsed.errors.length > 0}
            >
              {pending ? "Importing…" : "Import validated accounts"}
            </Button>
            </FormLayout>
          </FormDisclosure>
          <FormDisclosure error={error} appearance="standard" className="ui-panel">
            <summary>Add accounting period</summary>
            <FormLayout data-finance-draft={draftScope}
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                void send("periods", {
                  name: form.get("name"),
                  startDate: form.get("startDate"),
                  endDate: form.get("endDate"),
                });
              }}
            >
              <Field appearance="standard">
                Period name
                <TextInput appearance="standard" name="name" required maxLength={100} />
              </Field>
              <Field appearance="standard">
                Start date
                <TextInput appearance="standard" name="startDate" type="date" required />
              </Field>
              <Field appearance="standard">
                End date
                <TextInput appearance="standard" name="endDate" type="date" required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || !canCreate}>
                Add period
              </Button>
            </FormLayout>
          </FormDisclosure>
        </>
      )}
    </Stack>
  );
}
