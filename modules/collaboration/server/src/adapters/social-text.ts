import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeSocialTextStatements } from './native-social-text';
import type { Mention } from '@erpx/collaboration-server/public/rules/social-text';
import { captureJobAuthority } from '@erpx/identity-access-server/public/iam-jobs';
import { getRecord, searchReferences } from '@erpx/records-server/public/records';
import { runtimeEnv } from '../../../../../platform/runtime/server/src/public/runtime';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import {sourceAccessSql} from '@erpx/collaboration-server/public/social-source-policy';
import {socialPostTopicExists,socialCommentTopicExists} from './read-tail-sql';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { createSocialTextApplication, type SocialTextRepository } from '@erpx/collaboration-server/application/social-text';
import { collaborationMentionPrincipals } from '@erpx/identity-access-server/public/collaboration-mention-principals';
import { collaborationTextSources as owner0 } from '@erpx/identity-access-server/public/collaboration-text-sources';
import { collaborationTextSources as owner1 } from '@erpx/agent-server/public/collaboration-text-sources';
async function inward<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
function application(db: SqlDatabase, actor: WorkContext) {
    const repository: SocialTextRepository<SqlStatement> = { async tagSuggestions(value: string, companyId: string, userId: string, now: number, now2: number) {
            return db.prepare(`SELECT DISTINCT t.tag FROM social_topics t WHERE lower(t.tag) LIKE ? AND EXISTS(SELECT 1 FROM social_posts p WHERE p.company_id=? AND p.state='published' AND ${sourceAccessSql()} AND (${socialPostTopicExists('t.tag')} OR ${socialCommentTopicExists('t.tag')})) ORDER BY t.tag LIMIT 12`).bind(value, companyId, userId, now, now2).all<{
                tag: string;
            }>();
        }, ...nativeSocialTextStatements(db, actor),
     ...owner0(db), ...owner1(db) };
    return createSocialTextApplication({ repository, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, mentionPrincipals: async (ids) => collaborationMentionPrincipals((await runtimeEnv()).CONTROL_DB, ids), getRecord: (...args) => inward(() => getRecord(...args)), searchReferences: (...args) => inward(() => searchReferences(...args)), captureJobAuthority: (...args) => inward(() => captureJobAuthority(db, ...args)) });
}
export function mentionPeople(db: SqlDatabase, actor: Pick<WorkContext, "companyId">) { return exposed(() => application(db, actor as WorkContext).mentionPeople(actor)); }
export function textSuggestions(db: SqlDatabase, actor: WorkContext, trigger: string, q: string) { return exposed(() => application(db, actor).textSuggestions(actor, trigger, q)); }
export function validateMentions(db: SqlDatabase, actor: WorkContext, body: string, input: Mention[] = []) { return exposed(() => application(db, actor).validateMentions(actor, body, input)); }
export function textStatements(db: SqlDatabase, actor: WorkContext, postId: string, ownerType: "post" | "comment", ownerId: string, body: string, mentions: Mention[]) { return exposed(() => application(db, actor).textStatements(actor, postId, ownerType, ownerId, body, mentions)); }
