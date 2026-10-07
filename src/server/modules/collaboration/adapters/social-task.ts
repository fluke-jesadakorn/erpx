import type { SqlDatabase, SqlStatement } from '../../runtime/public/runtime/types';
import { ApiError } from '../../runtime/public/http';
import { BusinessRuleError } from '../../shared';
import { assertion } from '../../workflow/public/commands';
import { aiSnapshotGuard } from '../../agent/public/ai-providers';
import { readablePost, sourceSnapshot, sourceGuard } from '../public/social-source';
import type { StaffActor } from '../../operations/public/staff-policy';
import { createSocialTaskApplication } from '../application/social-task';
import type { TaskSource } from '../domain/social-task';
export { taskSourceSchema } from '../domain/social-task';
export type { TaskSource } from '../domain/social-task';
export async function taskSourceStatements(db: SqlDatabase, actor: StaffActor, key: string, taskId: string, users: string[], source?: TaskSource, projectId?: string | null) {
    const app = createSocialTaskApplication<SqlStatement>({ clock: { now: () => Date.now() }, repository: {
            taskOrigin: taskId => db.prepare('SELECT post_id FROM social_task_origins WHERE task_id=?').bind(taskId).first<{
                post_id: string;
            }>(),
            aiProvider: (id, user) => db.prepare('SELECT provider_json FROM social_ai_drafts WHERE id=? AND actor_id=?').bind(id, user).first<{
                provider_json: string | null;
            }>(),
            guardAiDraft: (key, id, user, post, hash, version, now) => assertion(db, key, 'ai-draft', "EXISTS(SELECT 1 FROM social_ai_drafts WHERE id=? AND actor_id=? AND post_id=? AND source_hash=? AND source_version=? AND state='ready' AND kind='task' AND expires_at>?)", [id, user, post, hash, version, now]),
            bindTaskOrigin: (post, task, version, hash, draft, user, now) => db.prepare('INSERT INTO social_task_origins(post_id,task_id,source_version,source_hash,ai_draft_id,created_by,created_at) VALUES(?,?,?,?,?,?,?)').bind(post, task, version, hash, draft, user, now)
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
