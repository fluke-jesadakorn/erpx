import { Button, Field, Select, Table, TextInput } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import type {
  crmAnalytics,
  marketingAnalytics,
  analyticsOptions,
  AnalyticsFilters,
} from "@/server/crm-marketing-report";
import styles from "./analytics-report.module.css";
import { Card, FilterBar, TableFrame } from "@/components/ui/foundation";

type CRM = Awaited<ReturnType<typeof crmAnalytics>>;
type Marketing = Awaited<ReturnType<typeof marketingAnalytics>>;
type Options = Awaited<ReturnType<typeof analyticsOptions>>;
function value(v: number | null, unit: string) {
  if (v === null) return "No data";
  if (unit === "ratio") return `${(v * 100).toFixed(1)}%`;
  if (unit.startsWith("minor"))
    return `${(v / 100).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })} THB`;
  return v.toLocaleString();
}
export function AnalyticsReport({
  kind,
  base,
  report,
  options,
  workspaceHref,
}: {
  kind: "crm" | "marketing";
  base: string;
  report: CRM | Marketing;
  options: Options;
  workspaceHref?: string;
}) {
  const f = report.filters;
  const select = (
    name: keyof AnalyticsFilters,
    label: string,
    rows: Array<Record<string, unknown>>,
  ) => (
    <Field appearance="standard">
      {label}
      <Select appearance="standard" name={name} defaultValue={f[name] ?? ""}>
        <option value="">All</option>
        {rows.map((r) => (
          <option key={String(r.id)} value={String(r.id)}>
            {String(r.name)}
          </option>
        ))}
      </Select>
    </Field>
  );
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{kind === "crm" ? "CRM analytics" : "Marketing analytics"}</h2>
      <FilterBar className="ui-filter-bar" method="get" action={workspaceHref ?? `${base}/${kind}`}>
        {workspaceHref && <input type="hidden" name="view" value="reports" />}
        <Field appearance="standard">
          From
          <TextInput appearance="standard" name="from" type="date" defaultValue={f.from} required />
        </Field>
        <Field appearance="standard">
          To
          <TextInput appearance="standard" name="to" type="date" defaultValue={f.to} required />
        </Field>
        {select("branchId", "Company / Branch", options.branches)}
        {select("propertyId", "Property", options.properties)}
        {select("campaignId", "Campaign", options.campaigns)}
        {select("ownerId", "Owner", options.owners)}
        <Button appearance="standard" type="submit" variant="primary">
          Apply filters
        </Button>
      </FilterBar>
      <p>
        Selected dates: {f.from}–{f.to} (Asia/Bangkok). Counts use source records and each metric
        states its formula and unit.
      </p>
      <div className={styles.metrics}>
        {report.metrics.map((m) => (
          <article className={styles.metric} key={m.id}>
            <h3>{m.label}</h3>
            <strong>{value(m.value, m.unit)}</strong>
            <details>
              <summary>Formula and source</summary>
              <p>{m.formula}</p>
              <p>
                Unit: {m.unit} · Source: {m.source} · Dates: {m.from}–{m.to} ({m.timezone})
              </p>
            </details>
            {m.drilldown.length > 0 && (
              <details>
                <summary>Source records ({m.drilldown.length})</summary>
                {m.drilldown.slice(0, 100).map((id) => (
                  <div key={id}>
                    <Link href={`${base}/${kind}/${id}`}>{id}</Link>
                  </div>
                ))}
              </details>
            )}
          </article>
        ))}
      </div>
      {kind === "crm" && "transitions" in report && (
        <>
          <h3>Stage changes during this period</h3>
          <Definition item={report.definitions[0]} />
          {report.transitions.length ? (
            <ul>
              {report.transitions.map((t) => (
                <li key={t.action}>
                  {t.action}: {t.count} <SourceLinks base={base} kind="crm" ids={t.leadIds} />
                </li>
              ))}
            </ul>
          ) : (
            <p>No stage changes.</p>
          )}
          <h3>Loss reasons</h3>
          <Definition item={report.definitions[1]} />
          {report.lossReasons.length ? (
            <ul>
              {report.lossReasons.map((x) => (
                <li key={x.reason}>
                  {x.reason}: {x.count} <SourceLinks base={base} kind="crm" ids={x.leadIds} />
                </li>
              ))}
            </ul>
          ) : (
            <p>No lost opportunities.</p>
          )}
          <h3>Confirmed outcomes</h3>
          <Definition item={report.definitions[2]} />
          {report.outcomes.length ? (
            <ul>
              {report.outcomes.map((x) => (
                <li key={x.leadId}>
                  <Link href={`${base}/crm/${x.leadId}`}>
                    {x.source} · {x.sourceId}
                  </Link>{" "}
                  · Current source state: {x.currentSourceState}
                </li>
              ))}
            </ul>
          ) : (
            <p>No confirmed outcomes.</p>
          )}
        </>
      )}
      {kind === "marketing" && "firstTouch" in report && (
        <>
          <p>
            First touch and last touch are separate attribution views; their totals must not be
            added together. ROI uses posted net revenue and an explicitly linked posted campaign
            cost; missing cost or campaign evidence shows No data.
          </p>
          <h3>First touch</h3>
          <Definition item={report.definitions[0]} />
          <Attribution rows={report.firstTouch} base={base} />
          <h3>Last touch</h3>
          <Definition item={report.definitions[1]} />
          <Attribution rows={report.lastTouch} base={base} />
          <p>Handoffs without campaign evidence: {report.unattributed}</p>
          <Definition item={report.definitions[2]} />
          <SourceLinks base={base} kind="marketing" ids={report.unattributedCandidateIds} />
        </>
      )}
    </Card>
  );
}
function Definition({
  item,
}: {
  item: {
    unit: string;
    formula: string;
    source: string;
    from: string;
    to: string;
    timezone: string;
  };
}) {
  return (
    <details>
      <summary>Formula and source</summary>
      <p>{item.formula}</p>
      <p>
        Unit: {item.unit} · Source: {item.source} · Dates: {item.from}–{item.to} ({item.timezone})
      </p>
    </details>
  );
}
function SourceLinks({
  base,
  kind,
  ids,
}: {
  base: string;
  kind: "crm" | "marketing";
  ids: string[];
}) {
  return ids.length ? (
    <details>
      <summary>Source records ({ids.length})</summary>
      {ids.slice(0, 100).map((id) => (
        <div key={id}>
          <Link href={`${base}/${kind}/${id}`}>{id}</Link>
        </div>
      ))}
    </details>
  ) : null;
}
function Attribution({ rows, base }: { rows: Marketing["firstTouch"]; base: string }) {
  return rows.length ? (
    <TableFrame appearance="standard" label="Records"><Table className="business-table">
      <thead>
        <tr>
          <th>Campaign ID</th>
          <th>Source</th>
          <th>Leads</th>
          <th>Wins</th>
          <th>Posted revenue</th>
          <th>Posted cost</th>
          <th>ROI</th>
          <th>Records</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={`${r.campaignId ?? "none"}-${i}`}>
            <td>{r.campaignId ?? "No data"}</td>
            <td>{r.source}</td>
            <td>{r.leads}</td>
            <td>{r.wins}</td>
            <td>{value(r.recognizedMinor, "minor THB")}</td>
            <td>{r.costMinor === null ? "No data" : value(r.costMinor, "minor THB")}</td>
            <td>{r.roi === null ? "No data" : `${(r.roi * 100).toFixed(1)}%`}</td>
            <td>
              <SourceLinks base={base} kind="marketing" ids={r.candidateIds} />
            </td>
          </tr>
        ))}
      </tbody>
    </Table></TableFrame>
  ) : (
    <p>No attributed handoffs in this period.</p>
  );
}
