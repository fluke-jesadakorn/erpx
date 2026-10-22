import {collaborationNotificationSnapshotSources as operationsSnapshotSources} from '@erpx/server-operations/public/collaboration-notification-snapshot-sources';
const operationsSources=operationsSnapshotSources();
import "server-only";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function HandoverCollaborationFacts(database: SqlDatabase) {
    return { readHandoverViewVersions(companyId: string, ids: string[]) {
            return database.prepare(`SELECT t.id,t.version ${operationsSources.operations_handover_facts_staff_tasks_t_from}
    WHERE t.company_id=? AND t.id IN (${ids.map(() => "?").join(",")})
    AND NOT EXISTS(SELECT 1 FROM social_task_origins o WHERE o.task_id=t.id)`)
                .bind(companyId, ...ids).all<{
                id: string;
                version: number;
            }>();
        },
        readPublishHandoverExisting(value: string, companyId: string, userId: string) {
            return database.prepare(`SELECT body,participants_json,sources_json,published_post_id FROM social_post_drafts
    WHERE id=? AND company_id=? AND author_id=?`).bind(value, companyId, userId)
                .first<{
                body: string;
                participants_json: string;
                sources_json: string;
                published_post_id: string | null;
            }>();
        },
        readHandoverAcknowledgementRecipients(postId: string, author_id: string) {
            return database.prepare(`SELECT user_id FROM social_post_participants WHERE post_id=? AND user_id<>?`)
                .bind(postId, author_id).all<{
                user_id: string;
            }>();
        },
        readHandoverAcknowledgementAcknowledgements(postId: string, version: number, ids: string[]) {
            return database.prepare(`SELECT user_id FROM social_handover_acknowledgements
    WHERE post_id=? AND post_version=? AND user_id IN (${ids.map(() => "?").join(",")})`)
                .bind(postId, version, ...ids).all<{
                user_id: string;
            }>();
        },
        guard7(key: string, value: "handover-recipient", postId: string, userId: string, postId2: string, userId2: string, version: number) {
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM social_post_participants WHERE post_id=? AND user_id=?)
        AND NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements WHERE post_id=? AND user_id=? AND post_version=?)`, [postId, userId, postId2, userId2, version]);
        },
        stageAcknowledgeHandoverEffect(postId: string, userId: string, version: number, now: number, key: string) {
            return database.prepare(`INSERT INTO social_handover_acknowledgements(post_id,user_id,post_version,acknowledged_at,command_id)
        VALUES(?,?,?,?,?)`).bind(postId, userId, version, now, key);
        } };
}
