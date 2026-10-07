import 'server-only';
import type { SqlDatabase, SqlStatement } from '../../runtime/public/runtime/types';
import type { WorkContext } from '../../identity_access/public/authorization';
import type { Mention } from '../public/rules/social-text';
import { captureJobAuthority } from '../../identity_access/public/iam-jobs';
import { getRecord, searchReferences } from '../../records/public/records';
import { runtimeEnv } from '../../runtime/public/runtime';
import { ApiError } from '../../runtime/public/http';
import { sourceAccessSql } from '../public/social-source';
import { BusinessRuleError } from '../../shared';
import { createSocialTextApplication, type SocialTextRepository } from '../application/social-text';
import { collaborationMentionPrincipals } from '../../identity_access/public/collaboration-mention-principals';
import { collaborationTextSources as owner0 } from '../../identity_access/public/collaboration-text-sources';
import { collaborationTextSources as owner1 } from '../../agent/public/collaboration-text-sources';
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
function application(db: SqlDatabase) {
    const repository: SocialTextRepository<SqlStatement> = { async tagSuggestions(value: string, companyId: string, userId: string, now: number, now2: number) {
            return db.prepare(`SELECT DISTINCT t.tag FROM social_topics t WHERE lower(t.tag) LIKE ? AND EXISTS(SELECT 1 FROM social_posts p WHERE p.company_id=? AND p.state='published' AND ${sourceAccessSql()} AND (EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=t.tag) OR EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=t.tag))) ORDER BY t.tag LIMIT 12`).bind(value, companyId, userId, now, now2).all<{
                tag: string;
            }>();
        }, prepareMention(ownerType: "post" | "comment", ownerId: string, kind: "record" | "person" | "agent", targetId: string, label: string, userId: string, now: number) { return db.prepare("INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(ownerType, ownerId, kind, targetId, label, userId, now); }, prepareMentionNotice(value: string, targetId: string, now: number, postId: string, targetId2: string, now2: number, now3: number) { return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,p.company_id,?,'post',p.id,p.version,'You were mentioned in a conversation',? FROM social_posts p WHERE p.id=? AND ${sourceAccessSql()}`).bind(value, targetId, now, postId, targetId2, now2, now3); }, prepareAssistantReply(value: string, postId: string, ownerType: "post" | "comment", ownerId: string, companyId: string, userId: string, value2: string, name: string, now: number, now2: number) { return db.prepare("INSERT INTO social_agent_replies(id,post_id,owner_type,owner_id,company_id,actor_id,actor_json,requester_name,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(value, postId, ownerType, ownerId, companyId, userId, value2, name, now, now2); }, prepareTopic(tag: string, userId: string, now: number) { return db.prepare("INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(tag, userId, now); }, prepareTopicBinding(ownerId: string, tag: string, ownerType: string) { return db.prepare(`INSERT INTO ${ownerType === "post" ? "social_post_topics(post_id" : "social_comment_topics(comment_id"},tag) VALUES(?,?) ON CONFLICT DO NOTHING`).bind(ownerId, tag); }, ...owner0(db), ...owner1(db) };
    return createSocialTextApplication({ repository, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, mentionPrincipals: async (ids) => collaborationMentionPrincipals((await runtimeEnv()).CONTROL_DB, ids), getRecord: (...args) => inward(() => getRecord(...args)), searchReferences: (...args) => inward(() => searchReferences(...args)), captureJobAuthority: (...args) => inward(() => captureJobAuthority(db, ...args)) });
}
export function mentionPeople(db: SqlDatabase, actor: Pick<WorkContext, "companyId">) { return exposed(() => application(db).mentionPeople(actor)); }
export function textSuggestions(db: SqlDatabase, actor: WorkContext, trigger: string, q: string) { return exposed(() => application(db).textSuggestions(actor, trigger, q)); }
export function validateMentions(db: SqlDatabase, actor: WorkContext, body: string, input: Mention[] = []) { return exposed(() => application(db).validateMentions(actor, body, input)); }
export function textStatements(db: SqlDatabase, actor: WorkContext, postId: string, ownerType: "post" | "comment", ownerId: string, body: string, mentions: Mention[]) { return exposed(() => application(db).textStatements(actor, postId, ownerType, ownerId, body, mentions)); }
