import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { socialActivityEvents, type SocialActivityEvent } from "@/server/modules/collaboration/public/social-activity";
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchSocialActivityEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "collaboration-thread-activity";
            cursor?: string | undefined;
        }, cursor: {
            at: number;
            id: string;
        } | null): Promise<{
            rows: SocialActivityEvent[];
        }> {
            const source = socialActivityEvents(actor);
            const conditions = ["e.at>=?", "e.at<?"];
            const values: unknown[] = [...source.values, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',e.id,e.source_id,e.source_type,e.title,e.activity_actor,e.thread_id)))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(e.at,e.id)<(?,?)");
                values.push(cursor.at, cursor.id);
            }
            const rows = (await db.prepare(`${source.sql} WHERE ${conditions.join(" AND ")} ORDER BY e.at DESC,e.id DESC LIMIT ?`)
                .bind(...values, input.limit + 1).all<SocialActivityEvent>()).results;
            return { rows };
        }, async factsReadSocialActivityEvidence(actor: WorkContext, recordId: string, filters: {
            from: string;
            to: string;
            currency: string;
        }): Promise<{
            row: SocialActivityEvent | null;
        }> {
            const source = socialActivityEvents(actor);
            const row = await db.prepare(`${source.sql} WHERE e.id=? AND e.at>=? AND e.at<? LIMIT 1`)
                .bind(...source.values, recordId, Date.parse(filters.from), Date.parse(filters.to) + 86400000).first<SocialActivityEvent>();
            return { row };
        } };
}
