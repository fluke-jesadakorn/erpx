import {learningMetricSource} from "@erpx/learning-server/adapters/metric-purchases";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { inputSchema, type LearningEvidenceRow, parseCursor } from '@erpx/platform-runtime-server/public/executive/learning-evidence-contract';
import { nativeLearningEvidenceSearch } from "./native-lane-31/client";

/* Native evidence search is owned by the mounted axum port (native_lane_31).
 * The owner renders the predicates, applies Company/branch/department scope,
 * projects the bounded rows and validates the reply. The two former TypeScript
 * SQL readers are retired; only the purchase/history authority gate remains on
 * the caller side. No SQL fallback survives. */
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<LearningEvidenceRow[]> {
            await learningMetricSource(db, actor, input.metric);
            return nativeLearningEvidenceSearch(
                actor,
                {
                    metric: input.metric,
                    query: input.query,
                    filters: input.filters,
                    limit: input.limit,
                },
                cursor,
            );
        } };
}
