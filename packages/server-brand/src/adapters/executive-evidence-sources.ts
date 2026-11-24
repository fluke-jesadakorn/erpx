import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { inputSchema, type BrandProfileRow, parseCursor, timeRange } from '@erpx/server-runtime/public/executive/brand-evidence-contract';
import { BRAND_RD_EVIDENCE_SEARCH, planBrandRead } from "./native-brand-read-ports";
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<BrandProfileRow[]> {
            const range = input.metric === "brand-profile-updates";
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
            if (range) {
                const t = timeRange(input.filters);
                values.push(t.from, t.toExclusive);
            }
            if (input.query) values.push(input.query, input.query, input.query);
            if (cursor) values.push(cursor.branchName, cursor.branchName, cursor.id);
            values.push(input.limit + 1);
            const plan = await planBrandRead(
                BRAND_RD_EVIDENCE_SEARCH,
                values,
                { range, query: !!input.query, cursor: !!cursor },
                actor,
            );
            return (await db.prepare(plan.sql).bind(...plan.values).all<BrandProfileRow>()).results;
        } };
}
