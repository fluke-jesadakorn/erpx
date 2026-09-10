import Link from "next/link";
import {
  ArrowUpRight,
  Clock3,
  CheckCircle2,
  MessageSquare,
  Search,
  Link2,
  Activity,
} from "lucide-react";
import {
  type ModuleDefinition,
  type ModuleSection,
  moduleHref,
  previewRecordHref,
  liveDestination,
  capabilitySupport,
} from "@/data/module-registry";
import { getModuleSamples, type PreviewRecord } from "@/data/preview/samples";
import {
  PreviewBanner,
  Metric,
  StatusBadge,
  EmptyState,
} from "@/components/ui/primitives";
import { ModuleIcon } from "@/components/workplace/module-icon";
import { ModuleTabs } from "./module-tabs";
import styles from "./modules.module.css";
import type { WorkspaceRecord } from "@/server/records";
export type WorkspaceQuery = { q?: string; status?: string; sort?: string };
type DisplayRecord = Omit<PreviewRecord, "source"> & {
  source: "sample" | "live";
};
function phase(status: string) {
  if (/inactive|suspended|revoked|blocked|expired|unavailable/i.test(status))
    return "In progress";
  return /posted|approved|accepted|closed|complete|active|available|released|resolved|published|ready|received/i.test(
    status,
  )
    ? "Complete"
    : /pending|review|wait|hold|blocked/i.test(status)
      ? "In progress"
      : "To do";
}
export function ModuleWorkspace({
  workspaceBase,
  module,
  section,
  query,
  liveRecords,
  canCreate = false,
}: {
  workspaceBase: string;
  module: ModuleDefinition;
  section: ModuleSection;
  query: WorkspaceQuery;
  liveRecords?: WorkspaceRecord[];
  canCreate?: boolean;
}) {
  const samples = getModuleSamples(module.id, section.id)!;
  const liveRows: DisplayRecord[] = (liveRecords ?? []).map((record) => ({
    id: record.id,
    title: record.title,
    description: record.description,
    status: record.status,
    fields: record.fields.map((field) => ({
      ...field,
      options: field.options ?? [],
    })),
    source: "live",
  }));
  const isLive = liveRecords !== undefined;
  const sourceRows: DisplayRecord[] = isLive
    ? liveRows
    : samples.records.map((record) => ({
        ...record,
        source: "sample" as const,
      }));
  const statuses = [...new Set(sourceRows.map((record) => record.status))];
  const rows = sourceRows
    .filter(
      (record) =>
        (!query.q ||
          `${record.title} ${record.id} ${record.description} ${record.fields.map((field) => String(field.value)).join(" ")}`
            .toLowerCase()
            .includes(query.q.toLowerCase())) &&
        (!query.status ||
          query.status === "all" ||
          record.status === query.status),
    )
    .sort((a, b) =>
      query.sort === "name"
        ? a.title.localeCompare(b.title)
        : query.sort === "status"
          ? a.status.localeCompare(b.status)
          : a.id.localeCompare(b.id),
    );
  const recordHref = (record: DisplayRecord, view = "details") =>
    isLive
      ? `${workspaceBase}/records/${module.id}/${encodeURIComponent(record.id)}`
      : `${previewRecordHref(workspaceBase, module.id, record.id)}?${new URLSearchParams({ section: section.id, view, ...(query.q ? { q: query.q } : {}), ...(query.status ? { status: query.status } : {}), ...(query.sort ? { sort: query.sort } : {}) })}`;
  const complete = rows.filter(
    (row) => phase(row.status) === "Complete",
  ).length;
  const live = liveDestination(module.id, section.id);
  const support = capabilitySupport(module.id, section.id);
  return (
    <main
      className="content"
      data-source={isLive ? "live" : "sample"}
      data-module={module.id}
      data-section={section.id}
      data-workspace-kind={section.kind}
    >
      <div className={styles.moduleIdentity}>
        <span className={styles.moduleIcon}>
          <ModuleIcon id={module.id} size={18} />
        </span>
        <strong>{module.name}</strong>
        <span>/ {module.group}</span>
        <Link href={`${workspaceBase}/modules`}>
          All modules{" "}
          <ArrowUpRight size={12} style={{ verticalAlign: "middle" }} />
        </Link>
      </div>
      <ModuleTabs
        workspaceBase={workspaceBase}
        module={module}
        active={section.id}
      />
      {!isLive && <PreviewBanner />}
      <header className={styles.workspaceHead}>
        <div>
          <h1>{section.label}</h1>
          <p className="lede">{section.description}.</p>
        </div>
        <div className={styles.workspaceActions}>
          {live && (
            <Link className="button" href={`${workspaceBase}/${live}`}>
              Open live {live === "work" ? "requests" : "updates"}{" "}
              <ArrowUpRight size={14} />
            </Link>
          )}
          {canCreate && (
            <Link
              className="button"
              href={`${workspaceBase}/records/${module.id}/new?section=${encodeURIComponent(section.id)}`}
            >
              New record
            </Link>
          )}
        </div>
      </header>
      <details className={styles.summary}>
        <summary>Summary</summary>
        <section
          className={styles.metrics}
          aria-label={`${isLive ? "Persisted" : "Sample"} record summary`}
        >
          <Metric
            label={
              section.kind === "directory"
                ? "Profiles in this view"
                : "Records in this view"
            }
            value={rows.length}
            detail={isLive ? "In this view" : "Sample records only"}
          />
          <Metric
            label="Needs attention"
            value={rows.length - complete}
            detail="Open or in progress"
          />
          <Metric
            label="Completed / active"
            value={complete}
            detail={isLive ? "Open or in progress" : "Based on sample status"}
          />
        </section>
      </details>
      <form
        className={styles.filters}
        action={moduleHref(workspaceBase, module, section.id)}
      >
        <div className={styles.search}>
          <Search size={15} />
          <input
            type="search"
            className="input"
            name="q"
            aria-label={`Search ${isLive ? "" : "sample"} records`}
            key={query.q ?? ""}
            defaultValue={query.q}
            placeholder={`Search ${section.label.toLowerCase()}…`}
          />
        </div>
        <details className={styles.filterDetails}>
          <summary>Filters</summary>
          <div>
            <select
              className="input"
              name="status"
              aria-label={`${isLive ? "Persisted" : "Sample"} status`}
              defaultValue={query.status ?? "all"}
              key={query.status ?? "all"}
            >
              <option value="all">All statuses</option>
              {statuses.map((status) => (
                <option key={status}>{status}</option>
              ))}
            </select>
            <select
              className="input"
              name="sort"
              aria-label={`Sort ${isLive ? "" : "sample"} records`}
              defaultValue={query.sort ?? "reference"}
              key={query.sort ?? "reference"}
            >
              <option value="reference">Reference</option>
              <option value="name">Name A–Z</option>
              <option value="status">Status</option>
            </select>
          </div>
        </details>
        <button className="button secondary" type="submit">
          Apply
        </button>
        {(query.q ||
          (query.status && query.status !== "all") ||
          query.sort) && (
          <Link
            className="button ghost"
            href={moduleHref(workspaceBase, module, section.id)}
          >
            Reset
          </Link>
        )}
      </form>
      <div className={styles.workspaceContent}>
        <section className={styles.primary}>
          <header className={styles.primaryHead}>
            <h2>{section.label} workspace</h2>
            <span>
              {rows.length} {isLive ? "records" : "sample records"}
            </span>
          </header>
          {rows.length === 0 ? (
            <EmptyState
              title={`No matching ${isLive ? "" : "sample"} records`}
              description={
                isLive
                  ? "Clear the filters or create a new record."
                  : "Clear the filters to explore the full preview."
              }
            />
          ) : (
            <WorkspaceBody
              kind={section.kind}
              rows={rows}
              module={module}
              section={section}
              href={recordHref}
              isLive={isLive}
            />
          )}
          {!isLive && (
            <p className={styles.disabledNote}>
              Preview only. {section.label} actions are unavailable until this
              capability is connected.
            </p>
          )}
        </section>
        <details className={styles.side}>
          <summary>About this workspace</summary>
          {live && (
            <section>
              <h2>Available in your workspace</h2>
              {support.actions.map((action) => (
                <p key={action.label}>
                  <strong>{action.label}</strong>
                  <br />
                  {action.availability === "live"
                    ? "Live · Subject to your active role"
                    : "Preview · Not available yet"}
                </p>
              ))}
              <Link
                className={styles.sideLink}
                href={`${workspaceBase}/${live}`}
              >
                Open live {live === "work" ? "work" : "updates"}
                <ArrowUpRight size={14} />
              </Link>
            </section>
          )}
          <section>
            <h2>Connected work</h2>
            {isLive ? (
              <p>Open a record to see related work and review history.</p>
            ) : samples.relations.length ? (
              samples.relations.map((relation) => (
                <Link
                  key={`${relation.moduleId}/${relation.recordId}`}
                  className={styles.sideLink}
                  href={previewRecordHref(
                    workspaceBase,
                    relation.moduleId,
                    relation.recordId,
                  )}
                >
                  <Link2 size={14} />
                  <span>
                    <strong>{relation.title}</strong>
                    <small>
                      Sample ·{" "}
                      {relation.label.replace(/([A-Z])/g, " $1").toLowerCase()}
                    </small>
                  </span>
                </Link>
              ))
            ) : (
              <p>
                Related records will appear here as this workflow connects to
                other teams.
              </p>
            )}
          </section>
        </details>
      </div>
    </main>
  );
}
function WorkspaceBody({
  kind,
  rows,
  module,
  section,
  href,
  isLive,
}: {
  kind: ModuleSection["kind"];
  rows: DisplayRecord[];
  module: ModuleDefinition;
  section: ModuleSection;
  href: (record: DisplayRecord, view?: string) => string;
  isLive: boolean;
}) {
  if (kind === "board")
    return (
      <div className={styles.board}>
        {["To do", "In progress", "Complete"].map((lane) => (
          <section key={lane} className={styles.boardLane}>
            <div className={styles.laneHeading}>
              {lane}
              <span>
                {rows.filter((row) => phase(row.status) === lane).length}
              </span>
            </div>
            {rows
              .filter((row) => phase(row.status) === lane)
              .map((row) => (
                <Link
                  key={row.id}
                  href={href(row)}
                  className={styles.boardCard}
                >
                  <span className={styles.recordId}>{row.id}</span>
                  <strong>{row.title}</strong>
                  <p>{row.description}</p>
                  <StatusBadge status={row.status} />
                </Link>
              ))}
          </section>
        ))}
      </div>
    );
  if (kind === "analytics") {
    const measure = rows[0].fields.find((field) => field.type === "number");
    const values = rows.map((row) =>
      Number(
        row.fields.find((field) => field.key === measure?.key)?.value ?? 0,
      ),
    );
    const maximum = Math.max(...values, 1);
    return (
      <div className={styles.chart}>
        <div className={styles.chartSummary}>
          <strong>
            {measure
              ? new Intl.NumberFormat("en").format(
                  values.reduce((total, value) => total + value, 0),
                )
              : rows.length}
          </strong>
          <p>
            {measure ? measure.label : "Sample records"}
            <br />
            {measure
              ? `Total across filtered ${isLive ? "" : "sample"} records`
              : "Distribution by workflow stage"}
          </p>
        </div>
        {measure
          ? rows.map((row, index) => (
              <div className={styles.chartRow} key={row.id}>
                <span>{row.title}</span>
                <div
                  className={styles.chartTrack}
                  role="img"
                  aria-label={`${row.title}: ${values[index]} ${measure.label}`}
                >
                  <div
                    className={styles.chartFill}
                    style={{ width: `${(values[index] / maximum) * 100}%` }}
                  />
                </div>
                <span>{new Intl.NumberFormat("en").format(values[index])}</span>
              </div>
            ))
          : ["To do", "In progress", "Complete"].map((label) => {
              const count = rows.filter(
                (row) => phase(row.status) === label,
              ).length;
              return (
                <div className={styles.chartRow} key={label}>
                  <span>{label}</span>
                  <div
                    className={styles.chartTrack}
                    role="img"
                    aria-label={`${label}: ${count} of ${rows.length}`}
                  >
                    <div
                      className={styles.chartFill}
                      style={{ width: `${(count / rows.length) * 100}%` }}
                    />
                  </div>
                  <span>{count}</span>
                </div>
              );
            })}
        <div className="section-head">
          <h2>Source records</h2>
          <span className="small">
            {isLive ? "Persisted local data" : "Sample data"}
          </span>
        </div>
        {rows.map((row) => (
          <Link key={row.id} href={href(row)} className="row">
            <span className="row-main">
              <strong className="row-title">{row.title}</strong>
              <div className="small">{row.id}</div>
            </span>
            <StatusBadge status={row.status} />
            <ArrowUpRight size={14} />
          </Link>
        ))}
      </div>
    );
  }
  if (kind === "timeline")
    return (
      <div className={styles.timeline}>
        {rows.map((row, index) => (
          <article key={row.id} className={styles.timelineItem}>
            <span className={styles.timelineIcon}>
              <Clock3 size={14} />
            </span>
            <div>
              <span className="eyebrow">
                {isLive ? "Persisted event" : "Sample event"}{" "}
                {String(index + 1).padStart(2, "0")}
              </span>
              <h3 style={{ marginTop: 6 }}>{row.title}</h3>
              <p>
                {row.description} · {row.status}
              </p>
              <Link href={href(row)}>
                View {row.id}{" "}
                <ArrowUpRight size={12} style={{ verticalAlign: "middle" }} />
              </Link>
            </div>
          </article>
        ))}
      </div>
    );
  if (kind === "library")
    return (
      <div className={styles.library}>
        {rows.map((row) => (
          <Link key={row.id} href={href(row)} className={styles.libraryCard}>
            <div className={styles.libraryVisual}>
              <ModuleIcon id={module.id} size={36} />
            </div>
            <div>
              <h3>{row.title}</h3>
              <p>{row.description}</p>
              <StatusBadge status={row.status} />
            </div>
          </Link>
        ))}
      </div>
    );
  if (kind === "directory")
    return (
      <div>
        {rows.map((row) => (
          <Link key={row.id} href={href(row)} className={styles.directoryRow}>
            <span className="avatar">
              {row.title
                .split(" ")
                .map((part) => part[0])
                .join("")
                .slice(0, 2)}
            </span>
            <div>
              <h3>{row.title}</h3>
              <p>{row.description}</p>
              <span className={styles.recordId}>{row.id}</span>
            </div>
            <StatusBadge status={row.status} />
            <ArrowUpRight size={14} />
          </Link>
        ))}
      </div>
    );
  if (kind === "health")
    return (
      <div>
        {rows.map((row) => (
          <Link href={href(row)} key={row.id} className={styles.healthRow}>
            {phase(row.status) === "Complete" ? (
              <CheckCircle2 size={19} />
            ) : (
              <Activity size={19} />
            )}
            <div>
              <h3>{row.title}</h3>
              <p>{row.description}</p>
              <span className={styles.recordId}>
                {row.id} · {isLive ? "Persisted operation" : "Sample operation"}
              </span>
            </div>
            <StatusBadge status={row.status} />
          </Link>
        ))}
      </div>
    );
  if (kind === "conversation")
    return (
      <div className={styles.conversation}>
        <div className="small">
          <MessageSquare
            size={15}
            style={{ verticalAlign: "middle", marginRight: 6 }}
          />
          {isLive
            ? "Persisted record conversations"
            : "Sample record conversations"}
        </div>
        {rows.map((row) => (
          <article className={styles.message} key={row.id}>
            <strong>{row.title}</strong>
            <p>{row.description}</p>
            <Link
              href={href(row)}
              className="small"
              style={{ color: "var(--brand)" }}
            >
              Open conversation context · {row.id} ↗
            </Link>
          </article>
        ))}
        <label className="field">
          Reply
          <input
            disabled
            placeholder="Replies are unavailable in this preview"
          />
        </label>
      </div>
    );
  const eligibleFields = rows[0].fields.filter(
    (field) => !["textarea", "checkbox"].includes(field.type),
  );
  const fields = (
    kind === "ledger"
      ? [
          ...eligibleFields.filter((field) => field.type === "number"),
          ...eligibleFields.filter((field) => field.type !== "number"),
        ]
      : eligibleFields
  ).slice(0, kind === "ledger" ? 3 : 2);
  return (
    <div
      className={styles.table}
      tabIndex={0}
      role="region"
      aria-label={`${module.name} ${isLive ? "" : "sample"} records`}
    >
      <table>
        <thead>
          <tr>
            <th>
              {kind === "ledger" ? "Reference & narrative" : section.label}
            </th>
            <th>Status</th>
            {fields.map((field) => (
              <th key={field.key}>{field.label}</th>
            ))}
            <th>
              <span className="sr-only">Open record</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.id}>
              <td>
                <Link href={href(row)}>
                  <strong>{row.title}</strong>
                  <small>
                    {row.id} · {row.description}
                  </small>
                </Link>
              </td>
              <td>
                <StatusBadge status={row.status} />
              </td>
              {fields.map((field) => (
                <td key={field.key}>
                  {field.type === "number"
                    ? new Intl.NumberFormat("en").format(
                        Number(
                          row.fields.find((item) => item.key === field.key)
                            ?.value ?? 0,
                        ),
                      )
                    : String(
                        row.fields.find((item) => item.key === field.key)
                          ?.value ?? "—",
                      )}
                </td>
              ))}
              <td>
                <Link href={href(row)} aria-label={`Open ${row.id}`}>
                  <ArrowUpRight size={15} />
                </Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
