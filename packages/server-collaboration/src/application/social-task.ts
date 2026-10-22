import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { AiProviderSnapshot } from '@erpx/server-agent/public/ai-providers';
import type { TaskSource } from '@erpx/server-collaboration/domain/social-task';
import type { PostRow } from '@erpx/server-collaboration/domain/social-source';
export interface SocialTaskPorts<Effect> {
    clock: Clock;
    repository: {
        taskOrigin(taskId: string): Promise<{
            post_id: string;
        } | null>;
        aiProvider(id: string, user: string): Promise<{
            provider_json: string | null;
        } | null>;
        guardAiDraft(key: string, id: string, user: string, post: string, hash: string, version: number, now: number): Effect;
        bindTaskOrigin(post: string, task: string, version: number, hash: string, draft: string | null, user: string, now: number): Effect;
    };
    sources: {
        read(actor: Pick<StaffActor, 'userId' | 'companyId'>, id: string): Promise<PostRow>;
        snapshot(actor: Pick<StaffActor, 'userId' | 'companyId'>, id: string, comments: string[]): Promise<{
            post: PostRow;
            hash: string;
        }>;
        guard(key: string, post: PostRow, user: string, step: string): Effect;
    };
    providerGuard(key: string, snapshot: AiProviderSnapshot): Effect;
}
export function createSocialTaskApplication<Effect>(ports: SocialTaskPorts<Effect>) {
    async function taskSourceStatements(actor: StaffActor, key: string, taskId: string, users: string[], source?: TaskSource, projectId?: string | null) {
        const existing = await ports.repository.taskOrigin(taskId);
        if (!source && !existing)
            return [];
        if (projectId)
            throw new BusinessRuleError(422, "POST_TASK_PROJECT", "Post tasks use named participants. Remove the project before saving.");
        const post = source
            ? (await ports.sources.snapshot(actor, source.postId, source.commentIds)).post
            : await ports.sources.read(actor, existing!.post_id);
        const statements: Effect[] = [];
        if (source) {
            const snapshot = await ports.sources.snapshot(actor, source.postId, source.commentIds);
            if (snapshot.hash !== source.sourceHash ||
                post.version !== source.sourceVersion)
                throw new BusinessRuleError(409, "SOURCE_CHANGED", "The conversation changed. Review it and prepare the task again.");
            if (source.aiDraftId) {
                const ai = await ports.repository.aiProvider(source.aiDraftId, actor.userId);
                if (ai?.provider_json)
                    statements.push(ports.providerGuard(key, JSON.parse(ai.provider_json) as AiProviderSnapshot));
            }
            if (source.aiDraftId)
                statements.push(ports.repository.guardAiDraft(key, source.aiDraftId, actor.userId, post.id, source.sourceHash, post.version, ports.clock.now()));
            statements.push(ports.repository.bindTaskOrigin(post.id, taskId, post.version, source.sourceHash, source.aiDraftId, actor.userId, ports.clock.now()));
        }
        for (const [i, user] of [...new Set([actor.userId, ...users])].entries()) {
            await ports.sources.read({ userId: user, companyId: actor.companyId }, post.id);
            statements.unshift(ports.sources.guard(key, post, user, `source-participant-${i}`));
        }
        return statements;
    }
    return { taskSourceStatements };
}
