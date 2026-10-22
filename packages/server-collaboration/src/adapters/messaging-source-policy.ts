import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {messagingActiveAccountSql} from '@erpx/server-identity_access/public/collaboration-messaging-authorization';
import {messagingRoomOrganizationSql} from '@erpx/server-organization/public/collaboration-messaging-projections';
/** Closed native room alias. This is the original account/member/private/Org
 * policy, shared by Messages and its Files boundary without a broad barrel. */
export function messagingRoomAccessSql(){return `${messagingActiveAccountSql('r')} AND ((r.organization_node_id IS NULL AND ((r.kind IN ('direct','group') AND EXISTS(SELECT 1 FROM messaging_room_members m WHERE m.room_id=r.id AND m.user_id=?)) OR (r.kind='channel' AND EXISTS(SELECT 1 FROM messaging_team_members t WHERE t.team_id=r.team_id AND t.user_id=?) AND (NOT r.private OR EXISTS(SELECT 1 FROM messaging_room_members m WHERE m.room_id=r.id AND m.user_id=?))))) OR (r.organization_node_id IS NOT NULL AND ${messagingRoomOrganizationSql('r','?')}))`;}
export const messagingRoomAccessArgs=(actor:WorkContext)=>[actor.userId,actor.userId,actor.userId,actor.userId,actor.userId];
