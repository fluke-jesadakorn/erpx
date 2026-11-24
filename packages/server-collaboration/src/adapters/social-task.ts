import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {socialTaskOriginStatement} from "./native-social-task-origin";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import { aiSnapshotGuard } from '@erpx/server-agent/public/ai-providers';
import {readablePost,sourceSnapshot,sourceGuard} from '@erpx/server-collaboration/public/social-source';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import { createSocialTaskApplication } from '@erpx/server-collaboration/application/social-task';
import {socialTaskOriginBy,socialAiDraftById} from './read-tail-sql';
import type { TaskSource } from '@erpx/server-collaboration/domain/social-task';
export { taskSourceSchema } from '@erpx/server-collaboration/domain/social-task';
export type { TaskSource } from '@erpx/server-collaboration/domain/social-task';
export async function taskSourceStatements(db: SqlDatabase, actor: StaffActor, key: string, taskId: string, users: string[], source?: TaskSource, projectId?: string | null) {
    const app = createSocialTaskApplication<SqlStatement>({ clock: { now: () => Date.now() }, repository: {
            taskOrigin: taskId => db.prepare(socialTaskOriginBy('post_id','task_id')).bind(taskId).first<{
                post_id: string;
            }>(),
            aiProvider: (id, user) => db.prepare(socialAiDraftById('provider_json')).bind(id, user).first<{
                provider_json: string | null;
            }>(),
            guardAiDraft: (key, id, user, post, hash, version, now) => assertion(db, key, 'ai-draft', "EXISTS(SELECT 1 FROM social_ai_drafts WHERE id=? AND actor_id=? AND post_id=? AND source_hash=? AND source_version=? AND state='ready' AND kind='task' AND expires_at>?)", [id, user, post, hash, version, now]),
            bindTaskOrigin: (post, task, version, hash, draft, user, now) => socialTaskOriginStatement(db, actor as WorkContext, { key, post, task, version, hash, draft, user, now })
        }, sources: { read: (a, id) => readablePost(db, a, id), snapshot: (a, id, c) => sourceSnapshot(db, a, id, c), guard: (...args) => sourceGuard(db, ...args) }, providerGuard: (key, snapshot) => aiSnapshotGuard(db, key, snapshot) });
    try {
        return await app.taskSourceStatements(actor, key, taskId, users, source, projectId);
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
