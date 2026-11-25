import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  nativeWorkspaceChoiceBrands,
  type Lane39HoldingContext,
} from "./native-lane-39/client";

/**
 * Native lane 39 cutover of the Brand workspace-choice publication read.
 *
 * The axum owner now executes the `organization_brand_profiles` query, applies
 * Company scope and projects the response. This adapter is transport only: it
 * keeps the retained signature for call-site compatibility, ignores the caller
 * unit (`database`) and emits no SQL. The caller supplies its WorkContext
 * (holdingId + holdingSlug) so the client selects the exact registered Holding;
 * there is no first-holding default. A missing config, unmatched context, or
 * failing native runtime fails closed with `ApiError`, with no TS fallback.
 */
export async function workspaceChoiceBrands(
  _database: SqlDatabase,
  companyIds: string[],
  actor?: Lane39HoldingContext,
) {
  void _database;
  const choices = await nativeWorkspaceChoiceBrands(companyIds, actor);
  return choices.map((choice) => ({
    scope_kind: choice.scope_kind,
    scope_id: choice.scope_id,
    primary_publication_id: choice.primary_publication_id,
  }));
}
