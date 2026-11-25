import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import type {HandoverAcknowledgement} from '@erpx/server-operations/public/handover';
import {workSocialReadScope} from '@erpx/server-collaboration/public/work-social-read-policy';
import {sourceAccessSql} from '@erpx/server-collaboration/public/social-source-policy';
import {jsonbTextIds,socialPostRecipientExists,socialHandoverAckJoin} from './read-tail-sql';
/** Batch projection of exactly the native recipient/acknowledgement rows used
 * by Operations. Current source visibility remains in this one SELECT. */
export async function readableHandoverReceipts(database:SqlDatabase,actor:StaffActor,ids:readonly string[]):Promise<HandoverAcknowledgement[]>{
 if(!ids.length)return [];const scope=await workSocialReadScope(database,actor),now=Date.now();
 return (await database.prepare(`${scope.prefix}SELECT p.id "postId",p.version "postVersion",
 ${socialPostRecipientExists('recipient.user_id=?')} "canAcknowledge",
 EXISTS(${socialHandoverAckJoin('1',' AND ack.user_id=? AND recipient.user_id<>p.author_id')}) acknowledged,
 (SELECT count(*)::int FROM social_post_participants recipient WHERE recipient.post_id=p.id AND recipient.user_id<>p.author_id) "recipientCount",
 (${socialHandoverAckJoin('count(*)::int',' AND recipient.user_id<>p.author_id')}) "acknowledgedCount"
 FROM social_posts p WHERE p.id IN${jsonbTextIds} AND p.company_id=? AND p.state='published' AND p.purpose='handover' AND p.audience_kind='participants' AND ${sourceAccessSql('p','?::text')} AND ${scope.where}`).bind(...scope.args,actor.userId,actor.userId,JSON.stringify(ids),actor.companyId,actor.userId,now,now,actor.assignmentId).all<HandoverAcknowledgement>()).results;
}
