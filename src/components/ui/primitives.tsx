import type { ReactNode } from "react";
import { CircleDashed, ArrowUpRight } from "lucide-react";
import styles from "./ui.module.css";
export function PageHeader({
  eyebrow,
  title,
  description,
  action,
}: {
  eyebrow?: string;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <header className={styles.pageHeader}>
      <div>
        {eyebrow && <div className="eyebrow">{eyebrow}</div>}
        <h1>{title}</h1>
        {description && <p className="lede">{description}</p>}
      </div>
      {action && <div className={styles.headerAction}>{action}</div>}
    </header>
  );
}
export function StatusBadge({ status }: { status: string }) {
  const tone =
    /reject|cancel|blocked|error|failed|inactive|suspend|revok|retired/i.test(
      status,
    )
      ? "danger"
      : /draft|waiting|review|pending|hold/i.test(status)
        ? "warning"
        : /approv|complete|active|posted|released|published|resolved|ready|live|accepted/i.test(
              status,
            )
          ? "success"
          : "info";
  return <span className={`badge ${tone}`}>{status.replaceAll("_", " ")}</span>;
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
    <div className={styles.empty}>
      <span className={styles.emptyIcon}>
        <CircleDashed size={24} />
      </span>
      <h2>{title}</h2>
      <p>{description}</p>
      {action}
    </div>
  );
}
export function PreviewBanner() {
  return (
    <div className={styles.preview} data-source="sample">
      <span className="badge warning">
        <CircleDashed size={12} />
        Preview · Sample data
      </span>
      <span>
        Explore the experience. Business actions are not available yet.
      </span>
    </div>
  );
}
export function LiveBanner() {
  return (
    <div className={styles.preview} data-source="live">
      <span className="badge success">
        <CircleDashed size={12} />
        Live · Persisted data
      </span>
      <span>
        Records are loaded from the scoped local database and keep their workflow history.
      </span>
    </div>
  );
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
    <article className={styles.metric}>
      <div>
        <span>{label}</span>
        {icon ?? <ArrowUpRight size={15} />}
      </div>
      <strong>{value}</strong>
      {detail && <small>{detail}</small>}
    </article>
  );
}
export function Skeleton() {
  return (
    <div className={styles.skeleton} aria-label="Loading" role="status">
      <div />
      <div />
      <div />
      <div />
    </div>
  );
}
