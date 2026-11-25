import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { inputSchema, type BrandProfileRow, parseCursor, timeRange } from '@erpx/server-runtime/public/executive/brand-evidence-contract';
import { nativeBrandEvidence } from "./native-lane-39/evidence-client";

/**
 * Native lane 39 cutover of the Brand executive evidence search.
 *
 * The axum owner now builds the exact registered statement, executes it on its
 * Holding pool and projects the rows. This adapter keeps the retained signature
 * for call-site compatibility, ignores the caller unit (`database`) and emits
 * no SQL, no bind array and no plan request. The caller supplies its WorkContext
 * (holdingId + holdingSlug) so the client selects the exact registered Holding;
 * there is no first-holding default. A missing config, unmatched context, or
 * failing native runtime fails closed with `ApiError`, with no TS fallback.
 */
export function executiveEvidenceSources(_database: SqlDatabase) {
  void _database;
  return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<BrandProfileRow[]> {
      const range = input.metric === "brand-profile-updates";
      const bounds = range ? timeRange(input.filters) : undefined;
      return nativeBrandEvidence({
        companyId: actor.companyId,
        ...(actor.branchId ? { branchId: actor.branchId } : {}),
        ...(bounds ? { from: bounds.from, toExclusive: bounds.toExclusive } : {}),
        ...(input.query ? { query: input.query } : {}),
        ...(cursor ? { cursor } : {}),
        // One-row lookahead: the retained application derives `hasMore` and
        // slices the page itself, so the owner must return limit + 1 rows.
        limit: input.limit + 1,
      }, actor);
    } };
}
