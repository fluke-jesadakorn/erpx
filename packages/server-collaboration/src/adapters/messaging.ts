import{nativeMessagingReadRepository,messagingRoom}from '@erpx/server-collaboration/adapters/messaging-read';
export{messagingRoom};
import { messagingCurrentAccountSql, activeMessagingPeople } from '@erpx/server-identity_access/public/collaboration-messaging-authorization';
import { messagingManagedTeamSql } from '@erpx/server-organization/public/collaboration-messaging-projections';
import 'server-only';
import { z } from 'zod';
import { roomSchema, teamSchema, channelSchema, manageSchema, messageSchema } from '@erpx/server-collaboration/domain/messaging';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { MessagingRoom, MessagingTeam, MessagingMessage, MessagingFile } from "@erpx/feature-collaboration/lib/messaging";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import { prepareAttachmentBinding } from '@erpx/server-files/public/attachments/service';
import { ensureOrganizationChannels } from '@erpx/server-organization/public/organization-channels';
import { collaborationMessagingSources as organization } from '@erpx/server-organization/public/collaboration-messaging-sources';
import { messagingAccountNames } from '@erpx/server-identity_access/public/collaboration-messaging-accounts';
import { runtimeEnv } from '../../../../packages/server-runtime/src/public/runtime';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { ApiError } from '@erpx/server-runtime/public/http';
import { createMessagingApplication, type MessagingRepository } from '@erpx/server-collaboration/application/messaging';
import { nativeOwnerWrite } from '@erpx/server-collaboration/adapters/native-owner-write';
import {messagingPurchaseControls,requireMessagingCurrentAccount,requireMessagingPurchase,withMessagingPurchasedCommand} from '@erpx/server-collaboration/adapters/messaging-purchases';
import {messagingMemberIds,messagingOwnerGuard,messagingTeamAccessSql,messagingRoomByIdSql,messagingTeamMemberGuard} from './read-tail-sql';
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
import {messagingRoomAccessSql} from '@erpx/server-collaboration/adapters/messaging-source-policy';
const access=messagingRoomAccessSql();
function application(db: SqlDatabase, actor: WorkContext, allowProvision=true) {
    const write = (kind: string, values: unknown[], key: string, fragments?: { target?: 'room' | 'team'; predicate?: string }) =>
        nativeOwnerWrite(db, actor, kind, values, key, Date.now(), fragments);
    const repository: MessagingRepository<SqlStatement> = { guardAccess(key: string, value: "messaging-access", roomId: string, companyId: string, value2: string[]) {
            return assertion(db, key, value, `EXISTS(${messagingRoomByIdSql('1',access)})`, [roomId, companyId, ...value2]);
        }, room(...args){return nativeMessagingReadRepository(db).room(...args);}, async team(id: string, companyId: string, userId: string, userId2: string) {
            return db
                .prepare(messagingTeamAccessSql('t.id=? AND ',messagingManagedTeamSql("t", "?"),''))
                .bind(id, companyId, userId, userId2)
                .first<MessagingTeam>();
        }, async activePeople(companyId: string, now: number, now2: number, value: string[], teamId: string | undefined) { return activeMessagingPeople(db, companyId, now, now2, value, teamId); }, guardMember(key: string, value: string, companyId: string, id: string, value2: string[], teamId: string | undefined) {
            return assertion(db, key, value, `${messagingCurrentAccountSql}${teamId ? ` AND ${messagingTeamMemberGuard('','?','?')}` : ''}`, [companyId, id, ...value2]);
        }, prepareRoom(id: string, companyId: string, kind: "direct" | "group", name: string, userId: string, directKey: string | null, now: number) {
            return write('msg_room_insert', [id, companyId, kind, name, userId, directKey, now], `msg-room:${id}`);
        }, prepareTeam(id: string, companyId: string, name: string, userId: string, now: number) {
            return write('msg_team_insert', [id, companyId, name, userId, now], `msg-team:${id}`);
        }, prepareGeneral(roomId: string, companyId: string, userId: string, id: string, now: number) {
            return write('msg_general_insert', [roomId, companyId, userId, id, now], `msg-general:${roomId}`);
        }, guardTeamOwner(key: string, value: "team-owner", teamId: string, companyId: string, userId: string) {
            return assertion(db, key, value, messagingOwnerGuard("messaging_teams", ""), [teamId, companyId, userId]);
        }, prepareChannel(id: string, companyId: string, name: string, userId: string, teamId: string, privateRoom: boolean, now: number) {
            return write('msg_channel_insert', [id, companyId, name, userId, teamId, privateRoom, now], `msg-channel:${id}`);
        }, async members(value: string, isTeam: boolean, channelTeam: string | null) {
            return db
                .prepare(messagingMemberIds(isTeam || channelTeam ? 'messaging_team_members' : 'messaging_room_members', isTeam || channelTeam ? 'team_id' : 'room_id'))
                .bind(value)
                .all<{
                user_id: string;
            }>();
        }, guardOwnerVersion(key: string, value: "owner-version", id: string, companyId: string, userId: string, expectedVersion: number, isTeam: boolean) {
            return assertion(db, key, value, messagingOwnerGuard(isTeam ? "messaging_teams" : "messaging_rooms", " AND version=?"), [id, companyId, userId, expectedVersion]);
        }, rename(name: string, id: string, isTeam: boolean) {
            return write('msg_rename', [name, id], `msg-rename:${id}`, { target: isTeam ? 'team' : 'room' });
        }, removeMembers(id: string, members: string[], isTeam: boolean) {
            return write('msg_remove_members', [id, ...members], `msg-remove-members:${id}:${members.length}`, { target: isTeam ? 'team' : 'room' });
        }, removeChannelMembers(id: string, members: string[]) {
            return write('msg_remove_channel_members', [id, ...members], `msg-remove-channel-members:${id}:${members.length}`);
        }, async teams(companyId: string, userId: string, userId2: string) {
            return db
                .prepare(messagingTeamAccessSql('',messagingManagedTeamSql("t", "?"),' ORDER BY t.name,t.id'))
                .bind(companyId, userId, userId2)
                .all<MessagingTeam>();
        }, async rooms(userId: string, userId2: string, companyId: string, value: string[]) {
            return db
                .prepare(`SELECT r.*,
    (SELECT count(*) FROM messaging_messages msg WHERE msg.room_id=r.id AND msg.sender_id<>? AND msg.seq>COALESCE((SELECT rd.seq FROM messaging_reads rd WHERE rd.room_id=r.id AND rd.user_id=?),0)) unread,
    (SELECT CASE WHEN msg.body='' THEN 'Attachment' ELSE left(msg.body,160) END FROM messaging_messages msg WHERE msg.room_id=r.id ORDER BY msg.seq DESC LIMIT 1) preview,
    COALESCE((SELECT msg.created_at FROM messaging_messages msg WHERE msg.room_id=r.id ORDER BY msg.seq DESC LIMIT 1),r.created_at) updated_at
    FROM messaging_rooms r WHERE r.company_id=? AND ${access} ORDER BY updated_at DESC,r.id`)
                .bind(userId, userId2, companyId, ...value)
                .all<MessagingRoom>();
        }, async directMembers(value: string[], userId: string) {
            return db
                .prepare(`SELECT room_id,user_id FROM messaging_room_members WHERE room_id IN (${value.map(() => "?").join(",")}) AND user_id<>?`)
                .bind(...value, userId)
                .all<{
                room_id: string;
                user_id: string;
            }>();
        }, async history(id: string, companyId: string, value: string[], value2: number[], before: number | undefined, after: number | undefined) {
            return db
                .prepare(`SELECT msg.* FROM messaging_messages msg JOIN messaging_rooms r ON r.id=msg.room_id WHERE r.id=? AND r.company_id=? AND ${access} ${before !== undefined ? "AND msg.seq<?" : after !== undefined ? "AND msg.seq>?" : ""} ORDER BY msg.seq ${after !== undefined ? "ASC" : "DESC"} LIMIT 51`)
                .bind(id, companyId, ...value, ...value2)
                .all<MessagingMessage>();
        }, async files(companyId: string, value: string[], value2: string[]) {
            return db
                .prepare(`SELECT f.id,f.original_name,f.media_type,f.bytes,f.message_id FROM messaging_files f JOIN messaging_rooms r ON r.id=f.room_id WHERE r.company_id=? AND ${access} AND f.message_id IN (${value2.map(() => "?").join(",")})`)
                .bind(companyId, ...value, ...value2)
                .all<MessagingFile & {
                message_id: string;
            }>();
        }, async unboundFiles(roomId: string, userId: string, files: string[]) {
            return db
                .prepare(`SELECT id,attachment_set_id FROM messaging_files
           WHERE room_id=? AND created_by=? AND message_id IS NULL
             AND id IN (${files.map(() => "?").join(",")})`)
                .bind(roomId, userId, ...files)
                .all<{
                id: string;
                attachment_set_id: string | null;
            }>();
        }, guardFile(key: string, value: string, file: string, roomId: string, userId: string) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM messaging_files WHERE id=? AND room_id=? AND created_by=? AND message_id IS NULL)", [file, roomId, userId]);
        }, advanceRoom(roomId: string, hasBinding: boolean) {
            return write(hasBinding ? "msg_advance_room_versioned" : "msg_advance_room", [roomId], `room-advance:${roomId}`);
        }, prepareMessage(id: string, userId: string, name: string, body: string, now: number, roomId: string) {
            return write('msg_message_insert', [id, userId, name, body, now, roomId], `msg-message:${id}`);
        }, bindFile(id: string, file: string) {
            return write('msg_file_bind_message', [id, file], `msg-file-bind:${id}`);
        }, async read(userId: string, seq: number, roomId: string, companyId: string, value: string[]) {
            return write('msg_read_upsert', [userId, seq, roomId, companyId, ...value], `msg-read:${roomId}:${userId}`, { predicate: access });
        }, async download(id: string, companyId: string, value: string[], userId: string) {
            return db
                .prepare(`SELECT f.* FROM messaging_files f JOIN messaging_rooms r ON r.id=f.room_id WHERE f.id=? AND r.company_id=? AND ${access} AND (f.message_id IS NOT NULL OR f.created_by=?)`)
                .bind(id, companyId, ...value, userId)
                .first<MessagingFile & {
                object_key: string;
                room_id: string;
                attachment_set_id: string | null;
                attachment_link_id: string | null;
                attachment_source_id: string | null;
            }>();
        }, ...organization(db) };
    return createMessagingApplication({ repository, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, clock: { now: () => Date.now() }, digest: sha256, accountNames: async (ids) => messagingAccountNames((await runtimeEnv()).CONTROL_DB, ids), ensureOrganizationChannels: company => allowProvision?ensureOrganizationChannels(db, company):Promise.resolve(), memberBinding(kind, id, user) { return write('msg_member_bind', [id, user], `msg-member:${kind}:${id}:${user}`, { target: kind }); }, prepareAttachmentBinding: (...args) => inward(() => prepareAttachmentBinding(db, ...args)) });
}
export function messagingGuard(db: SqlDatabase, actor: WorkContext, key: string, roomId: string) { return application(db, actor).messagingGuard(actor, key, roomId); }

/** This original active-account picker is also Calendar's independent account
 * foundation. Only a requested Messages team brings the add-on dependency. */
export async function messagingPeople(db:SqlDatabase,actor:WorkContext,query='',teamId?:string){await requireMessagingCurrentAccount(db,actor);if(teamId)await requireMessagingPurchase(db,actor,'history');return exposed(()=>application(db, actor).messagingPeople(actor,query,teamId));}
export async function createMessagingRoom(db:SqlDatabase,actor:WorkContext,input:z.infer<typeof roomSchema>,key:string){const members=[...new Set([actor.userId,...input.members])].sort(),id=input.kind==='direct'?`dm-${await sha256(actor.companyId+':'+JSON.stringify(members))}`:`room-${key}`;return withMessagingPurchasedCommand(db,actor,{key,operation:'messaging.create',id,purpose:'new-work'},tx=>exposed(()=>application(tx, actor).createMessagingRoom(actor,input,key)));}
export async function createMessagingTeam(db:SqlDatabase,actor:WorkContext,input:z.infer<typeof teamSchema>,key:string){return withMessagingPurchasedCommand(db,actor,{key,operation:'messaging.team-create',id:`team-${key}`,team:true,purpose:'new-work'},tx=>exposed(()=>application(tx, actor).createMessagingTeam(actor,input,key)));}
export async function createMessagingChannel(db:SqlDatabase,actor:WorkContext,teamId:string,input:z.infer<typeof channelSchema>,key:string){return withMessagingPurchasedCommand(db,actor,{key,operation:'messaging.channel-create',id:`channel-${key}`,purpose:'new-work'},tx=>exposed(()=>application(tx, actor).createMessagingChannel(actor,teamId,input,key)));}
export async function messagingMembers(db:SqlDatabase,actor:WorkContext,id:string,isTeam:boolean){await requireMessagingPurchase(db,actor,'history');return exposed(()=>application(db, actor).messagingMembers(actor,id,isTeam));}
export async function manageMessaging(db:SqlDatabase,actor:WorkContext,id:string,isTeam:boolean,input:z.infer<typeof manageSchema>,key:string){return withMessagingPurchasedCommand(db,actor,{key,operation:'messaging.manage',id,team:isTeam,purpose:async tx=>{const original=await tx.prepare(`SELECT name FROM ${isTeam?'messaging_teams':'messaging_rooms'} WHERE id=? AND company_id=?`).bind(id,actor.companyId).first<{name:string}>(),members=(await tx.prepare(messagingMemberIds(isTeam?'messaging_team_members':'messaging_room_members',isTeam?'team_id':'room_id')).bind(id).all<{user_id:string}>()).results.map(row=>row.user_id);return original?.name===input.name&&input.members.every(user=>members.includes(user))?'settle-existing':'new-work';}},tx=>exposed(()=>application(tx, actor).manageMessaging(actor,id,isTeam,input,key)));}
export async function messagingOverview(db:SqlDatabase,actor:WorkContext){const purchase=await messagingPurchaseControls(db,actor);if(purchase&&!purchase.history)return {teams:[],rooms:[],purchase};return withMessagingPurchasedCommand(db,actor,{key:`messages-overview:${crypto.randomUUID()}`,operation:'messaging.overview',id:actor.companyId,companyScope:true,purpose:purchase?.newWork===true?'new-work':'history'},async tx=>({...await exposed(()=>application(tx, actor, purchase?.newWork!==false).messagingOverview(actor)),...(purchase?{purchase}:{})}));}
export async function messagingHistory(db:SqlDatabase,actor:WorkContext,id:string,before?:number,after?:number){await requireMessagingPurchase(db,actor,'history');return exposed(()=>application(db, actor).messagingHistory(actor,id,before,after));}
export async function sendMessagingMessage(db:SqlDatabase,actor:WorkContext,roomId:string,input:z.infer<typeof messageSchema>,key:string){return withMessagingPurchasedCommand(db,actor,{key,operation:'messaging.send',id:roomId,privateFiles:input.files.length>0,purpose:'new-work'},tx=>exposed(()=>application(tx, actor).sendMessagingMessage(actor,roomId,input,key)));}
export async function readMessaging(db:SqlDatabase,actor:WorkContext,roomId:string,seq:number){return withMessagingPurchasedCommand(db,actor,{key:`messages-read:${crypto.randomUUID()}`,operation:'messaging.read',id:roomId,purpose:'history'},tx=>exposed(()=>application(tx, actor).readMessaging(actor,roomId,seq)));}
export async function messagingFile(db:SqlDatabase,actor:WorkContext,id:string){await requireMessagingPurchase(db,actor,'history');return exposed(()=>application(db, actor).messagingFile(actor,id));}
export { roomSchema, teamSchema, channelSchema, manageSchema, messageSchema } from '@erpx/server-collaboration/domain/messaging';
