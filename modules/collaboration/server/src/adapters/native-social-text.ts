import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';
import { sourceAccessSql } from './social-source-policy';

/** Native-owned social text notice/binding statements. */
export function nativeSocialTextStatements(db: SqlDatabase, actor: WorkContext) {
  return {
    prepareMention(ownerType: 'post' | 'comment', ownerId: string, kind: 'record' | 'person' | 'agent', targetId: string, label: string, userId: string, now: number): Promise<SqlStatement> {
      return nativeNotificationStatement(db, actor, {
        companyId: actor.companyId, userId, recordType: ownerType, recordId: ownerId,
        recordVersion: 1, message: label, kind: 'social_mention', target_kind: kind, target_id: targetId,
      }, `mention-${ownerId}-${targetId}`, now);
    },
    prepareMentionNotice(value: string, targetId: string, now: number, postId: string, targetId2: string, now2: number, now3: number): Promise<SqlStatement> {
      return nativeNotificationStatement(db, actor, {
        companyId: actor.companyId, userId: targetId, recordType: 'post', recordId: postId,
        recordVersion: 1, message: '', kind: 'social_mention_notice', target_id: targetId, source_access_sql: sourceAccessSql(),
      }, value, now);
    },
    prepareAssistantReply(value: string, postId: string, ownerType: 'post' | 'comment', ownerId: string, companyId: string, userId: string, value2: string, name: string, now: number, now2: number): Promise<SqlStatement> {
      return nativeNotificationStatement(db, actor, {
        companyId, userId, recordType: ownerType, recordId: postId, recordVersion: 1,
        message: value2, kind: 'social_agent_reply', identifier: name,
      }, value, now);
    },
    prepareTopic(tag: string, userId: string, now: number): Promise<SqlStatement> {
      return nativeNotificationStatement(db, actor, {
        companyId: actor.companyId, userId, recordType: '', recordId: '', recordVersion: 1,
        message: tag, kind: 'social_topic',
      }, `topic-${tag}`, now);
    },
    prepareTopicBinding(ownerId: string, tag: string, ownerType: string): Promise<SqlStatement> {
      return nativeNotificationStatement(db, actor, {
        companyId: actor.companyId, userId: '', recordType: ownerType, recordId: ownerId,
        recordVersion: 1, message: tag, kind: 'social_topic_binding',
      }, `topic-binding-${ownerId}-${tag}`, Date.now());
    },
  };
}
