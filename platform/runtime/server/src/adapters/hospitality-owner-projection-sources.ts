import "server-only";
import { finalRootOwnerProjectionSources } from "@erpx/platform-runtime-server/adapters/final-root-owner-projection-sources";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted.
 *
 * The executive_generation_jobs projection is Runtime-owned fixed SQL also served
 * by the final-root owner source. Both public surfaces reuse one statement so the
 * two callers cannot drift into separate query text. */
export const hospitalityOwnerProjectionSources = {
  executive_generation_jobs: finalRootOwnerProjectionSources.executive_generation_jobs,
} as const;
