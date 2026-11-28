import "server-only";
import { betaPolicyProjection } from "./identity-snapshot-sources";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted.
 *
 * The beta_policy projection is manage_erpx-owned fixed SQL also served by the
 * IAM statement snapshot. Both public surfaces reuse one declaration so the
 * two callers cannot drift into separate query text. The spaced column list
 * is normalized to the compact form consumed by the joins. */
export const hospitalityOwnerProjectionSources = {
  beta_policy: betaPolicyProjection.replaceAll(", ", ","),
} as const;
