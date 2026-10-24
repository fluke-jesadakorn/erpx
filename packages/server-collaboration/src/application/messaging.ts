import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { MessagingPerson, MessagingRoom, MessagingTeam, MessagingMessage, MessagingFile } from "@erpx/feature-collaboration/lib/messaging";
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import { roomSchema, teamSchema, channelSchema, manageSchema, messageSchema } from '@erpx/server-collaboration/domain/messaging';
import type { prepareAttachmentBinding } from '@erpx/server-files/public/attachments/service';
const accessArgs = (actor: WorkContext) => [actor.userId, actor.userId, actor.userId, actor.userId, actor.userId];
export interface MessagingRepository<Effect> {
    guardAccess(key: string, value: "messaging-access", roomId: string, companyId: string, value2: string[]): Effect;
    room(id: string, companyId: string, value: string[]): Promise<MessagingRoom | null>;
    team(id: string, companyId: string, userId: string, userId2: string): Promise<MessagingTeam | null>;
    activePeople(companyId: string, now: number, now2: number, value: string[], teamId: string | undefined): Promise<{
        results: {
            user_id: string;
        }[];
    }>;
    guardMember(key: string, value: string, companyId: string, id: string, value2: string[], teamId: string | undefined): Effect;
    prepareRoom(id: string, companyId: string, kind: "direct" | "group", name: string, userId: string, directKey: string | null, now: number): Effect;
    prepareTeam(id: string, companyId: string, name: string, userId: string, now: number): Effect;
    prepareGeneral(roomId: string, companyId: string, userId: string, id: string, now: number): Effect;
    guardTeamOwner(key: string, value: "team-owner", teamId: string, companyId: string, userId: string): Effect;
    prepareChannel(id: string, companyId: string, name: string, userId: string, teamId: string, privateRoom: boolean, now: number): Effect;
    organizationMembers(companyId: string, value: (string | null | undefined)[], isTeam: boolean): Promise<{
        results: {
            user_id: string;
        }[];
    }>;
    members(value: string, isTeam: boolean, channelTeam: string | null): Promise<{
        results: {
            user_id: string;
        }[];
    }>;
    guardOwnerVersion(key: string, value: "owner-version", id: string, companyId: string, userId: string, expectedVersion: number, isTeam: boolean): Effect;
    rename(name: string, id: string, isTeam: boolean): Effect;
    removeMembers(id: string, members: string[], isTeam: boolean): Effect;
    removeChannelMembers(id: string, members: string[]): Effect;
    teams(companyId: string, userId: string, userId2: string): Promise<{
        results: MessagingTeam[];
    }>;
    rooms(userId: string, userId2: string, companyId: string, value: string[]): Promise<{
        results: MessagingRoom[];
    }>;
    directMembers(value: string[], userId: string): Promise<{
        results: {
            room_id: string;
            user_id: string;
        }[];
    }>;
    history(id: string, companyId: string, value: string[], value2: number[], before: number | undefined, after: number | undefined): Promise<{
        results: MessagingMessage[];
    }>;
    files(companyId: string, value: string[], value2: string[]): Promise<{
        results: (MessagingFile & {
            message_id: string;
        })[];
    }>;
    unboundFiles(roomId: string, userId: string, files: string[]): Promise<{
        results: {
            id: string;
            attachment_set_id: string | null;
        }[];
    }>;
    guardFile(key: string, value: string, file: string, roomId: string, userId: string): Effect;
    advanceRoom(roomId: string, hasBinding: boolean): Effect;
    prepareMessage(id: string, userId: string, name: string, body: string, now: number, roomId: string): Effect;
    bindFile(id: string, file: string): Effect;
    read(userId: string, seq: number, roomId: string, companyId: string, value: string[]): Promise<unknown>;
    download(id: string, companyId: string, value: string[], userId: string): Promise<MessagingFile & {
        object_key: string;
        room_id: string;
        attachment_set_id: string | null;
        attachment_link_id: string | null;
        attachment_source_id: string | null;
    } | null>;
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface MessagingPorts<Effect> {
    repository: MessagingRepository<Effect>;
    commands: OwnerCommands<Effect>;
    clock: Clock;
    digest(value: string): Promise<string>;
    accountNames(ids: string[]): Promise<MessagingPerson[]>;
    ensureOrganizationChannels(company: string): Promise<unknown>;
    memberBinding(kind: 'room' | 'team', id: string, user: string): Effect;
    prepareAttachmentBinding(actor: WorkContext, input: Parameters<typeof prepareAttachmentBinding>[2], options: {
        compatibilityAdapter: true;
    }): Promise<{
        guards: Effect[];
        steps: Effect[];
    }>;
}
export function createMessagingApplication<Effect>(ports: MessagingPorts<Effect>) {
const{messagingRoom}=createMessagingRead(ports);
    const r = ports.repository;
    function messagingGuard(actor: WorkContext, key: string, roomId: string) {
        return r.guardAccess(key, "messaging-access", roomId, actor.companyId, accessArgs(actor));
    }
    function missing(): never {
        throw new BusinessRuleError(404, "CHAT_NOT_FOUND", "This conversation is unavailable.");
    }

    async function team(actor: WorkContext, id: string, owner = false) {
        const row = await r.team(id, actor.companyId, actor.userId, actor.userId);
        if (!row)
            missing();
        if (owner && (row.organization_managed || row.owner_id !== actor.userId))
            throw new BusinessRuleError(403, "OWNER_REQUIRED", "Only the owner can manage this team.");
        return row;
    }
    async function messagingPeople(actor: WorkContext, query = "", teamId?: string) {
        if (teamId)
            await team(actor, teamId);
        const ids = (await r.activePeople(actor.companyId, ports.clock.now(), ports.clock.now(), (teamId ? [teamId] : []), teamId)).results.map((r) => r.user_id);
        const people = await ports.accountNames(ids);
        return people
            .filter((p) => `${p.name} ${p.email}`.toLowerCase().includes(query.toLowerCase()))
            .slice(0, 100);
    }
    async function memberGuards(actor: WorkContext, key: string, ids: string[], teamId?: string) {
        if ((await ports.accountNames(ids)).length !== ids.length)
            throw new BusinessRuleError(422, "MEMBER_UNAVAILABLE", "Select active user accounts.");
        return ids.map((id, i) => r.guardMember(key, `member-${i}`, actor.companyId, id, (teamId ? [teamId, id] : []), teamId));
    }
    function insertMembers(kind: 'room' | 'team', id: string, members: string[]) { return members.map(user => ports.memberBinding(kind, id, user)); }
    async function createMessagingRoom(actor: WorkContext, input: z.infer<typeof roomSchema>, key: string) {
        const members = [...new Set([actor.userId, ...input.members])].sort();
        if (input.kind === "direct" && members.length !== 2)
            throw new BusinessRuleError(422, "RECIPIENT_REQUIRED", "Select one other person.");
        if (input.kind === "group" && (!input.name || members.length < 2))
            throw new BusinessRuleError(422, "GROUP_REQUIRED", "Name the group and select at least one person.");
        const directKey = input.kind === "direct" ? JSON.stringify(members) : null;
        // Stable room IDs and a unique pair key resolve simultaneous first conversations.
        const id = directKey
            ? `dm-${await ports.digest(actor.companyId + ":" + directKey)}`
            : `room-${key}`;
        const steps = await memberGuards(actor, key, members);
        return ports.commands.commit(actor, key, "messaging.create", input, id, 1, { id }, [
            ...steps,
            r.prepareRoom(id, actor.companyId, input.kind, input.name, actor.userId, directKey, ports.clock.now()),
            ...insertMembers("room", id, members),
        ]);
    }
    async function createMessagingTeam(actor: WorkContext, input: z.infer<typeof teamSchema>, key: string) {
        const id = `team-${key}`, roomId = `general-${key}`, members = [...new Set([actor.userId, ...input.members])];
        return ports.commands.commit(actor, key, "messaging.team-create", input, id, 1, { id, roomId }, [
            ...(await memberGuards(actor, key, members)),
            r.prepareTeam(id, actor.companyId, input.name, actor.userId, ports.clock.now()),
            ...insertMembers("team", id, members),
            r.prepareGeneral(roomId, actor.companyId, actor.userId, id, ports.clock.now()),
        ]);
    }
    async function createMessagingChannel(actor: WorkContext, teamId: string, input: z.infer<typeof channelSchema>, key: string) {
        await team(actor, teamId, true);
        const id = `channel-${key}`, members = [...new Set([actor.userId, ...input.members])];
        return ports.commands.commit(actor, key, "messaging.channel-create", input, id, 1, { id }, [
            r.guardTeamOwner(key, "team-owner", teamId, actor.companyId, actor.userId),
            ...(await memberGuards(actor, key, members, teamId)),
            r.prepareChannel(id, actor.companyId, input.name, actor.userId, teamId, input.private, ports.clock.now()),
            ...insertMembers("room", id, input.private ? members : []),
        ]);
    }
    async function messagingMembers(actor: WorkContext, id: string, isTeam: boolean) {
        const row = isTeam
            ? await team(actor, id)
            : await messagingRoom(actor, id);
        if ((row as MessagingRoom).organization_node_id || (row as MessagingTeam).organization_managed) {
            const ids = (await r.organizationMembers(actor.companyId, (isTeam ? [] : [(row as MessagingRoom).organization_node_id]), isTeam)).results.map(p => p.user_id);
            return { ...row, members: await ports.accountNames(ids) };
        }
        const channelTeam = !isTeam &&
            (row as MessagingRoom).kind === "channel" &&
            !(row as MessagingRoom).private
            ? (row as MessagingRoom).team_id
            : null;
        const ids = (await r.members(channelTeam || id, isTeam, channelTeam)).results.map((r) => r.user_id);
        return { ...row, members: await ports.accountNames(ids) };
    }
    async function manageMessaging(actor: WorkContext, id: string, isTeam: boolean, input: z.infer<typeof manageSchema>, key: string) {
        const row = isTeam
            ? await team(actor, id, true)
            : await messagingRoom(actor, id);
        if (row.owner_id !== actor.userId ||
            (!isTeam && (row as MessagingRoom).kind === "direct"))
            throw new BusinessRuleError(403, "OWNER_REQUIRED", "Only the owner can manage this conversation.");
        const room = row as MessagingRoom;
        const members = [...new Set([row.owner_id, ...input.members])];
        const steps: Effect[] = [
            ...(await memberGuards(actor, key, members, !isTeam && room.kind === "channel" ? room.team_id! : undefined)),
            r.guardOwnerVersion(key, "owner-version", id, actor.companyId, actor.userId, input.expectedVersion, isTeam),
            r.rename(input.name, id, isTeam),
        ];
        if (isTeam || room.kind === "group" || room.private) {
            steps.push(r.removeMembers(id, members, isTeam), ...insertMembers(isTeam ? "team" : "room", id, members));
        }
        if (isTeam)
            steps.push(r.removeChannelMembers(id, members));
        if (!isTeam)
            steps.unshift(messagingGuard(actor, key, id));
        return ports.commands.commit(actor, key, "messaging.manage", { ...input, isTeam }, id, input.expectedVersion + 1, { id }, steps);
    }
    async function messagingOverview(actor: WorkContext) {
        await ports.ensureOrganizationChannels(actor.companyId);
        const teams = (await r.teams(actor.companyId, actor.userId, actor.userId)).results;
        const rooms = (await r.rooms(actor.userId, actor.userId, actor.companyId, accessArgs(actor))).results;
        const directRooms = rooms.filter((r) => r.kind === "direct");
        if (directRooms.length) {
            const members = (await r.directMembers(directRooms.map((r) => r.id), actor.userId)).results;
            const people = await ports.accountNames([
                ...new Set(members.map((m) => m.user_id)),
            ]);
            for (const room of directRooms)
                room.name =
                    people.find((p) => p.id === members.find((m) => m.room_id === room.id)?.user_id)?.name ?? "Former member";
        }
        return { teams, rooms };
    }
    async function messagingHistory(actor: WorkContext, id: string, before?: number, after?: number) {
        const room = await messagingRoom(actor, id);
        const rows = (await r.history(id, actor.companyId, accessArgs(actor), (before !== undefined
            ? [before]
            : after !== undefined
                ? [after]
                : []), before, after)).results;
        const hasMore = rows.length > 50, messages = rows.slice(0, 50);
        if (after === undefined)
            messages.reverse();
        for (const msg of messages)
            msg.files = [];
        if (messages.length) {
            const files = (await r.files(actor.companyId, accessArgs(actor), messages.map((m) => m.id))).results;
            for (const msg of messages)
                msg.files = files.filter((f) => f.message_id === msg.id);
        }
        return { room, messages, hasMore };
    }
    async function sendMessagingMessage(actor: WorkContext, roomId: string, input: z.infer<typeof messageSchema>, key: string) {
        const room = await messagingRoom(actor, roomId);
        const id = `message-${key}`;
        const operation = "messaging.send";
        const prior = await ports.commands.replay<{
            id: string;
        }>(actor, key, operation, roomId, input);
        if (prior)
            return prior;
        let prepared: {
            guards: Effect[];
            steps: Effect[];
        } | null = null;
        if (input.files.length) {
            const mapped = (await r.unboundFiles(roomId, actor.userId, input.files)).results;
            if (mapped.length !== input.files.length ||
                mapped.some((file) => !file.attachment_set_id))
                throw new BusinessRuleError(409, "ATTACHMENT_SCAN_REQUIRED", "Every message file must use the current quarantine and malware scan lifecycle.");
            prepared = await ports.prepareAttachmentBinding(actor, {
                attachmentSetIds: mapped.map((file) => String(file.attachment_set_id)),
                bindingContract: {
                    targetType: "messaging_room",
                    bindingType: "message_attachment",
                    contexts: mapped.map((file) => ({ purposeKey: "message_attachment", componentKind: "message" as const, componentRef: file.id })),
                },
                targetId: roomId,
                expectedTargetVersion: Number(room.version),
                commandId: key,
                bindingId: id,
            }, { compatibilityAdapter: true });
        }
        const steps = [
            messagingGuard(actor, key, roomId),
            ...(prepared?.guards ?? []),
        ];
        input.files.forEach((file, i) => steps.push(r.guardFile(key, `file-${i}`, file, roomId, actor.userId)));
        steps.push(r.advanceRoom(roomId, Boolean(prepared)), r.prepareMessage(id, actor.userId, actor.name, input.body, ports.clock.now(), roomId));
        for (const file of input.files)
            steps.push(r.bindFile(id, file));
        steps.push(...(prepared?.steps ?? []));
        return ports.commands.commit(actor, key, operation, input, roomId, Number(room.version) + (prepared ? 1 : 0), { id }, steps);
    }
    async function readMessaging(actor: WorkContext, roomId: string, seq: number) {
        await messagingRoom(actor, roomId);
        await r.read(actor.userId, seq, roomId, actor.companyId, accessArgs(actor));
        return { ok: true };
    }
    async function messagingFile(actor: WorkContext, id: string) {
        const file = await r.download(id, actor.companyId, accessArgs(actor), actor.userId);
        return file ?? missing();
    }
    return { messagingGuard, messagingRoom, messagingPeople, createMessagingRoom, createMessagingTeam, createMessagingChannel, messagingMembers, manageMessaging, messagingOverview, messagingHistory, sendMessagingMessage, readMessaging, messagingFile };
}

export type createMessagingReadPorts<Effect>=Pick<MessagingPorts<Effect>,never>&{repository:Pick<MessagingRepository<Effect>,"room">};
export function createMessagingRead<Effect>(ports:createMessagingReadPorts<Effect>){const r=ports.repository;function missing():never{throw new BusinessRuleError(404,'CHAT_NOT_FOUND','This conversation is unavailable.');}
async function messagingRoom(actor: WorkContext, id: string) {
        const room = await r.room(id, actor.companyId, accessArgs(actor));
        return room ?? missing();
    }return{messagingRoom};}
