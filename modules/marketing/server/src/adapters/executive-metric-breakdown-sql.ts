import "server-only";
import { marketingSelectHeadSql } from "./select-head-sql";

/** Closed-variant owner-local executive metric breakdown projection. The
 * candidate and engagement executive readers share one fixed projection frame;
 * a caller supplies only the purchased source, label and scope predicate and
 * never raw query text. Each admitted variant emits its original statement byte
 * for byte, so no duplicate breakdown literal survives in the owner. */
export function marketingMetricBreakdownSql(
  variant: "candidates" | "engagements",
  purchased: string,
  label: string,
  scope: string,
): string {
  const source =
    variant === "candidates"
      ? `(${purchased}) c WHERE ${scope}`
      : `marketing_engagement_events e JOIN (${purchased}) c ON c.id=e.candidate_id WHERE ${scope} AND e.created_at>=? AND e.created_at<?`;
  return marketingSelectHeadSql(
    `${label} label,count(*)::text value`,
    ` FROM ${source} GROUP BY 1 ORDER BY 1`,
  );
}
