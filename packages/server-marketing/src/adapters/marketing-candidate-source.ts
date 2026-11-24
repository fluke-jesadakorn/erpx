import 'server-only';
/** Canonical owner-local fixed source for the unenforced/legacy Marketing
 * candidate read. The analytics legacy report and the metric fallback share
 * this one admitted projection, so no duplicate TS SQL literal remains.
 * Callers supply no SQL and no relation name. */
export const marketingCandidateSourceSql = "SELECT * FROM marketing_lead_candidates";
