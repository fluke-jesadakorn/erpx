import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {messagingActiveAccountSql} from '@erpx/server-identity_access/public/collaboration-messaging-authorization';
import {messagingRoomOrganizationSql} from '@erpx/server-organization/public/collaboration-messaging-projections';
import {messagingRoomMemberExists} from './read-tail-sql';
/** Closed native room alias. This is the original account/member/private/Org
 * policy, shared by Messages and its Files boundary without a broad barrel. */
export function messagingRoomAccessSql(){return `${messagingActiveAccountSql('r')} AND ((r.organization_node_id IS NULL AND ((r.kind IN ('direct','group') AND ${messagingRoomMemberExists}) OR (r.kind='channel' AND EXISTS(SELECT 1 FROM messaging_team_members t WHERE t.team_id=r.team_id AND t.user_id=?) AND (NOT r.private OR ${messagingRoomMemberExists})))) OR (r.organization_node_id IS NOT NULL AND ${messagingRoomOrganizationSql('r','?')}))`;}
export const messagingRoomAccessArgs=(actor:WorkContext)=>[actor.userId,actor.userId,actor.userId,actor.userId,actor.userId];
