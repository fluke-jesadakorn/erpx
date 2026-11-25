/**
 * Runtime-owned canonical SQL contract for the closed read-tail families and
 * the report guards.
 *
 * The seven projection shapes and the two guards live in
 * `read-tail-sources.json`, the same asset the owner crate binds with
 * `include_str!`. This module only substitutes a caller's closed column list
 * and verbatim suffix; it authors no statement text. No relation name or
 * predicate is accepted from an untrusted caller. Control-plane lock and
 * migration statements are owned by the native control-plane planner.
 */
import contract from "./read-tail-sources.json" with { type: "json" };

type Sources = Readonly<{
  holdingBindings: string;
  executiveGenerationJobs: string;
  runtimeHealth: string;
  executiveExternalTokens: string;
  executiveReportShares: string;
  executiveReportRevisions: string;
  executiveReports: string;
  reportLock: string;
  reportVersionGuard: string;
}>;

const sources = contract as Sources;

/** Substitute the closed projection and verbatim tail without regex replacer
 *  expansion, so `$1` binds survive byte-for-byte. */
const render = (shape: string, columns: string, suffix: string) =>
  shape.replace("{columns}", () => columns).replace("{suffix}", () => suffix);

/** Canonical `holding_bindings` projection. */
export const holdingBindingsSql = (columns: string, suffix = "") =>
  render(sources.holdingBindings, columns, suffix);

/** Canonical `executive_generation_jobs` projection. */
export const executiveGenerationJobsSql = (columns: string, suffix = "") =>
  render(sources.executiveGenerationJobs, columns, suffix);

/** Canonical `runtime_health` projection. */
export const runtimeHealthSql = (columns: string, suffix = "") =>
  render(sources.runtimeHealth, columns, suffix);

/** Canonical `executive_external_tokens` projection. */
export const executiveExternalTokensSql = (columns: string, suffix = "") =>
  render(sources.executiveExternalTokens, columns, suffix);

/** Canonical `executive_report_shares` projection. */
export const executiveReportSharesSql = (columns: string, suffix = "") =>
  render(sources.executiveReportShares, columns, suffix);

/** Canonical `executive_report_revisions` projection. */
export const executiveReportRevisionsSql = (columns: string, suffix = "") =>
  render(sources.executiveReportRevisions, columns, suffix);

/** Canonical `executive_reports` projection. */
export const executiveReportsSql = (columns: string, suffix = "") =>
  render(sources.executiveReports, columns, suffix);

/** Single-row report lock guard executed by the caller unit. */
export const reportLockSql = sources.reportLock;

/** Report ownership/version guard executed by the workflow assertion. */
export const reportVersionGuardSql = sources.reportVersionGuard;
