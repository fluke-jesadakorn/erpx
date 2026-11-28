import "server-only";
import { posMetricHistoryAccess } from "@erpx/pos-server/adapters/metric-purchases";
import { z } from "zod";
import { type Filters } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { type PosEvidenceMetric, inputSchema, type EvidenceRow, parseCursor } from '@erpx/platform-runtime-server/public/executive/pos-evidence-contract';
import { nativePosEvidenceRows } from "./native-lane-25/client";

/**
 * Native POS transaction-evidence search (lane 25). The SQL for all three
 * source legs, the cursor predicate, the projection and the fixed response row
 * are owned by `erpx-server-pos::adapters::native_lane_25`; the owner executes
 * them with sqlx behind an axum router. This bridge sends only bounded
 * identities and filters and strictly validates the returned rows. The
 * cross-owner package/assignment purchase gate stays on the caller's unit.
 */
export function executiveEvidenceSources(database: SqlDatabase) {
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<EvidenceRow[]> {
            if (!(await posMetricHistoryAccess(database, actor))[input.metric]) return [];
            const filters: Filters = input.filters;
            return nativePosEvidenceRows(
                {
                    holdingId: actor.holdingId,
                    holdingSlug: actor.holdingSlug,
                    companyId: actor.companyId,
                    branchId: actor.branchId ?? null,
                },
                {
                    metric: input.metric as "pos-sales" | "net-pos-sales" | "pos-payment-mix",
                    from: filters.from,
                    to: filters.to,
                    currency: filters.currency,
                    query: input.query,
                    limit: input.limit,
                    ...(cursor ? { eventDate: cursor.eventDate, id: cursor.id } : {}),
                },
            );
        } };
}
