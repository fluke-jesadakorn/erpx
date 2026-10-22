"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Table as UiTable } from "@erpx/ui/components/foundation";

import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { useState } from "react";
import s from "@erpx/feature-legal/components/legal.module.css";
import { TableFrame } from "@erpx/ui/components/foundation";
type Row = Record<string, string | number | null>;
type Reports = {
  asOf: number;
  registers: Row[];
  upcoming: Row[];
  workflows: { category: string; rows: Row[] }[];
  costs: Row[];
  access: Row[];
  jobs: Row[];
  invitations: Row[];
};
const names: Record<string, string> = {
  kind: "Register",
  state: "Status",
  count: "Items",
  overdue: "Overdue",
  sla_breached: "SLA breached",
  title: "Title",
  due_date: "Due date",
  owner_id: "Owner",
  currency: "Currency",
  budget_minor: "Budget (minor units)",
  fees_minor: "Recorded fees (minor units)",
  classification: "Classification",
  members: "Members",
  external_shares: "Live external shares",
  attempts: "Attempts",
  error: "Failure reason",
  created_at: "Queued at",
};
function Table({ rows, open }: { rows: Row[]; open: (id: string) => Promise<void> }) {
  if (!rows.length) return <p className={s.small}>No matching records.</p>;
  const columns = Object.keys(rows[0]).filter((k) => !["id", "item_id"].includes(k));
  return (
    <TableFrame appearance="standard" label="Report records">
      <UiTable className={s.table}>
        <thead>
          <tr>
            {columns.map((k) => (
              <th key={k}>{names[k] ?? k.replaceAll("_", " ")}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={String(r.id ?? i)}>
              {columns.map((k) => (
                <td key={k}>
                  {k === "title" && r.id ? (
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
                      className={s.link}
                      onClick={() => void open(String(r.id))}
                    >
                      {r[k]}
                    </Button>
                  ) : k === "created_at" ? (
                    <TimeText appearance="standard" value={Number(r[k])} />
                  ) : (
                    String(r[k] ?? "—")
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </UiTable>
    </TableFrame>
  );
}
export function LegalReports({
  call,
  open,
}: {
  call: (path: string) => Promise<unknown>;
  open: (id: string) => Promise<void>;
}) {
  const [data, setData] = useState<Reports | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  return (
    <div className={s.toolContent}>
      <Button appearance="standard"
        type="submit"
        variant="ghost"
        className={s.button}
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          try {
            setData((await call("/reports")) as Reports);
            setError("");
          } catch (e) {
            setError((e as Error).message);
          } finally {
            setBusy(false);
          }
        }}
      >
        Refresh reports
      </Button>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data && (
        <>
          <p className={s.small}>
            As of <TimeText appearance="standard" value={data.asOf} />. Results follow your current record permissions.
          </p>
          {(
            [
              ["Register status and SLA", data.registers],
              ["Due within 90 days", data.upcoming],
              ["Outside counsel budget and fees", data.costs],
              ["Access review", data.access],
              ["Document processing", data.jobs],
              ["External invitations", data.invitations],
            ] as [string, Row[]][]
          ).map(([title, rows]) => (
            <section key={title}>
              <h3>{title}</h3>
              <Table rows={rows} open={open} />
            </section>
          ))}
          <h3>Workflow status</h3>
          {data.workflows
            .filter((g) => g.rows.length)
            .map((g) => (
              <section key={g.category}>
                <h4>{g.category.replaceAll("_", " ")}</h4>
                <Table rows={g.rows} open={open} />
              </section>
            ))}
        </>
      )}
    </div>
  );
}
