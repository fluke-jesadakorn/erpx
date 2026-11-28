import {collaborationNotificationSnapshotSources as operationsSnapshotSources} from '@erpx/operations-server/public/collaboration-notification-snapshot-sources';
const operationsSources=operationsSnapshotSources();
import "server-only";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {nativeOwnerWriteSystemUnscoped} from "@erpx/collaboration-server/adapters/native-owner-write";
import {socialPostDraftById} from './read-tail-sql';
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeSystemNotificationStatement } from './native-system-notification';
import { nativeInboxSqlFragment } from './native-inbox-statements';
/** Owner-planned read: no SQL text from the caller, executed on its own unit. */
const ownerRead = (db: SqlDatabase, company: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, company,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
export function HandoverCollaborationFacts(database: SqlDatabase) {
    return { async readHandoverViewVersions(companyId: string, ids: string[]) {
            const originAbsent = await nativeInboxSqlFragment(companyId, 'social_handover_task_origin_absent');
            return database.prepare(`SELECT t.id,t.version ${operationsSources.operations_handover_facts_staff_tasks_t_from}
    WHERE t.company_id=? AND t.id IN (${ids.map(() => "?").join(",")})
    ${originAbsent}`)
                .bind(companyId, ...ids).all<{
                id: string;
                version: number;
            }>();
        },
        readPublishHandoverExisting(value: string, companyId: string, userId: string) {
            return database.prepare(socialPostDraftById('body,participants_json,sources_json,published_post_id','\n    ',' AND author_id=?')).bind(value, companyId, userId)
                .first<{
                body: string;
                participants_json: string;
                sources_json: string;
                published_post_id: string | null;
            }>();
        },
        async readHandoverAcknowledgementRecipients(postId: string, author_id: string) {
            return (await ownerRead(database, 'system', 'social_handover_recipients', [postId, author_id])).all<{
                user_id: string;
            }>();
        },
        async readHandoverAcknowledgementAcknowledgements(postId: string, version: number, ids: string[]) {
            return (await ownerRead(database, 'system', 'social_handover_acknowledgements', [postId, version, ...ids])).all<{
                user_id: string;
            }>();
        },
        guard7(key: string, value: "handover-recipient", postId: string, userId: string, postId2: string, userId2: string, version: number) {
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM social_post_participants WHERE post_id=? AND user_id=?)
        AND NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements WHERE post_id=? AND user_id=? AND post_version=?)`, [postId, userId, postId2, userId2, version]);
        },
        stageAcknowledgeHandoverEffect(postId: string, userId: string, version: number, now: number, key: string) {
            return nativeOwnerWriteSystemUnscoped(database, 'handover_ack_insert', [postId, userId, version, now, key], `handover-ack:${key}`, now);
        } };
}
