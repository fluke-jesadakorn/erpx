import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {messagingActiveAccountSql} from '@erpx/identity-access-server/public/collaboration-messaging-authorization';
import {messagingRoomOrganizationSql} from '@erpx/organization-server/public/collaboration-messaging-projections';
import {messagingMemberExists,messagingTeamMemberGuard} from './read-tail-sql';
/** Closed native room alias. This is the original account/member/private/Org
 * policy, shared by Messages and its Files boundary without a broad barrel. */
export function messagingRoomAccessSql(){return `${messagingActiveAccountSql('r')} AND ((r.organization_node_id IS NULL AND ((r.kind IN ('direct','group') AND ${messagingMemberExists('messaging_room_members','room_id','r.id')}) OR (r.kind='channel' AND ${messagingTeamMemberGuard('t','r.team_id','?')} AND (NOT r.private OR ${messagingMemberExists('messaging_room_members','room_id','r.id')})))) OR (r.organization_node_id IS NOT NULL AND ${messagingRoomOrganizationSql('r','?')}))`;}
export const messagingRoomAccessArgs=(actor:WorkContext)=>[actor.userId,actor.userId,actor.userId,actor.userId,actor.userId];
