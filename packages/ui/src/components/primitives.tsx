import type { ReactNode } from "react";
import { CircleDashed, ArrowUpRight } from "lucide-react";

import type { UiAppearance } from "@erpx/ui/components/appearance";
import { Badge } from "@erpx/ui/components/patterns";

export function PageHeader({
  eyebrow,
  title,
  description,
  action,
  appearance = "legacy",
}: {
  appearance?: UiAppearance;
  eyebrow?: string;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <header className={`${appearance === "standard" ? "ui-page-header" : "mb-6"} flex flex-col items-start justify-between gap-4 sm:flex-row sm:items-center sm:gap-6`}>
      <div className="min-w-0">
        {eyebrow && <div className="eyebrow">{eyebrow}</div>}
        <h1>{title}</h1>
        {description && <p className="lede max-w-[72ch]">{description}</p>}
      </div>
      {action && <div className="flex w-full items-center gap-2 sm:w-auto">{action}</div>}
    </header>
  );
}
export function StatusBadge({ status, appearance = "legacy" }: { status: string; appearance?: UiAppearance }) {
  const tone = /reject|cancel|blocked|error|failed|inactive|suspend|revok|retired/i.test(status)
    ? "danger"
    : /draft|waiting|review|pending|hold/i.test(status)
      ? "warning"
      : /approv|complete|active|posted|released|published|resolved|ready|live|accepted/i.test(
            status,
          )
        ? "success"
        : "info";
  if (appearance === "standard") return <Badge tone={tone}>{status.replaceAll("_", " ")}</Badge>;
  return (
    <span data-slot="badge" className={`badge ${tone}`}>
      {status.replaceAll("_", " ")}
    </span>
  );
}
export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="px-6 py-10 text-center text-[var(--ui-muted)]">
      <span className="mx-auto mb-4 grid size-12 place-items-center rounded-[var(--ui-radius-md)] bg-[var(--ui-brand-soft)] text-[var(--ui-brand-strong)]">
        <CircleDashed size={24} aria-hidden="true" />
      </span>
      <h2 className="text-[length:var(--ui-type-card)]">{title}</h2>
      <p className="mx-auto mt-2 mb-4 max-w-90">{description}</p>
      {action}
    </div>
  );
}
function DataBanner({ live }: { live: boolean }) {
  return (
    <div
      className="mb-6 flex flex-wrap items-center gap-3 rounded-[var(--ui-radius-md)] border border-[var(--ui-line)] bg-[var(--ui-card)] px-4 py-3 text-xs text-[var(--ui-muted)]"
      data-source={live ? "live" : "sample"}
    >
      <span className={`badge ${live ? "success" : "warning"}`}>
        <CircleDashed size={12} aria-hidden="true" />
        {live ? "Live · Persisted data" : "Preview · Sample data"}
      </span>
      <span>
        {live
          ? "Records are loaded from the scoped local database and keep their workflow history."
          : "Explore the experience. Business actions are not available yet."}
      </span>
    </div>
  );
}
export function PreviewBanner() {
  return <DataBanner live={false} />;
}
export function LiveBanner() {
  return <DataBanner live />;
}
export function Metric({
  label,
  value,
  detail,
  icon,
}: {
  label: string;
  value: ReactNode;
  detail?: string;
  icon?: ReactNode;
}) {
  return (
    <article className="min-w-0 bg-[var(--ui-card)] p-4 sm:px-6 sm:py-5">
      <div className="flex items-center justify-between gap-3 text-[length:var(--ui-type-meta)] text-[var(--ui-muted)]">
        <span>{label}</span>
        {icon ?? <ArrowUpRight size={15} aria-hidden="true" />}
      </div>
      <strong className="mt-3 mb-1 block text-[32px] font-bold tracking-tight text-[var(--ui-ink-strong)] tabular-nums">
        {value}
      </strong>
      {detail && (
        <small className="text-[length:var(--ui-type-meta)] text-[var(--ui-muted)]">{detail}</small>
      )}
    </article>
  );
}
export function Skeleton() {
  return (
    <div className="grid gap-4 p-8" aria-label="Loading" role="status">
      {[0, 1, 2, 3].map((index) => (
        <div
          key={index}
          className={
            index === 0
              ? "h-8 w-[45%] animate-pulse rounded-[var(--ui-radius-md)] bg-[var(--ui-card-2)] motion-reduce:animate-none"
              : "h-15 animate-pulse rounded-[var(--ui-radius-md)] bg-[var(--ui-card-2)] motion-reduce:animate-none"
          }
        />
      ))}
    </div>
  );
}
