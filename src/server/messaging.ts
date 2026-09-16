import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { runtimeEnv } from "./runtime";
import { ApiError } from "./http";
import { assertion, executeCommand } from "./commands";
import { ensureOrganizationChannels, organizationMemberSql } from "./organization-channels";
import type {
  MessagingPerson,
  MessagingRoom,
  MessagingTeam,
  MessagingMessage,
  MessagingFile,
} from "@/lib/messaging";

const idSchema = z.string().min(1).max(160);
const membersSchema = z
  .array(idSchema)
  .max(100)
  .transform((v) => [...new Set(v)].sort());
export const roomSchema = z.object({
  kind: z.enum(["direct", "group"]),
  name: z.string().trim().max(120).default(""),
  members: membersSchema,
});
export const teamSchema = z.object({
  name: z.string().trim().min(1).max(120),
  members: membersSchema,
});
export const channelSchema = teamSchema.extend({ private: z.boolean() });
export const manageSchema = teamSchema.extend({
  expectedVersion: z.number().int().positive(),
});
export const messageSchema = z
  .object({
    body: z.string().trim().max(5000),
    files: z.array(z.string().uuid()).max(5).default([]),
  })
  .refine(
    (v) => v.body.length > 0 || v.files.length > 0,
    "Write a message or attach a file.",
  )
  .refine(
    (v) => new Set(v.files).size === v.files.length,
    "Duplicate attachments.",
  );

const active = `EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=r.company_id AND a.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;
const access = `${active} AND ((r.organization_node_id IS NULL AND ((r.kind IN ('direct','group') AND EXISTS(SELECT 1 FROM messaging_room_members m WHERE m.room_id=r.id AND m.user_id=?)) OR (r.kind='channel' AND EXISTS(SELECT 1 FROM messaging_team_members t WHERE t.team_id=r.team_id AND t.user_id=?) AND (NOT r.private OR EXISTS(SELECT 1 FROM messaging_room_members m WHERE m.room_id=r.id AND m.user_id=?))))) OR (r.organization_node_id IS NOT NULL AND EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=r.organization_node_id AND n.company_id=r.company_id AND n.status='active' AND ${organizationMemberSql("n","?")})))`;
const accessArgs = (actor: WorkContext) => [
  actor.userId,
  actor.userId,
  actor.userId,
  actor.userId,
  actor.userId,
];
export function messagingGuard(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  roomId: string,
) {
  return assertion(
    db,
    key,
    "messaging-access",
    `EXISTS(SELECT 1 FROM messaging_rooms r WHERE r.id=? AND r.company_id=? AND ${access})`,
    [roomId, actor.companyId, ...accessArgs(actor)],
  );
}
function missing(): never {
  throw new ApiError(
    404,
    "CHAT_NOT_FOUND",
    "This conversation is unavailable.",
  );
}
export async function messagingRoom(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
) {
  const room = await db
    .prepare(
      `SELECT r.* FROM messaging_rooms r WHERE r.id=? AND r.company_id=? AND ${access}`,
    )
    .bind(id, actor.companyId, ...accessArgs(actor))
    .first<MessagingRoom>();
  return room ?? missing();
}
async function team(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  owner = false,
) {
  const row = await db
    .prepare(
      `SELECT t.* FROM messaging_teams t WHERE t.id=? AND t.company_id=? AND ((NOT t.organization_managed AND EXISTS(SELECT 1 FROM messaging_team_members m WHERE m.team_id=t.id AND m.user_id=?)) OR (t.organization_managed AND EXISTS(SELECT 1 FROM organization_nodes n WHERE n.company_id=t.company_id AND n.kind='company' AND n.status='active' AND ${organizationMemberSql("n","?")})))`,
    )
    .bind(id, actor.companyId, actor.userId, actor.userId)
    .first<MessagingTeam>();
  if (!row) missing();
  if (owner && (row.organization_managed || row.owner_id !== actor.userId))
    throw new ApiError(
      403,
      "OWNER_REQUIRED",
      "Only the owner can manage this team.",
    );
  return row;
}
async function accountNames(ids: string[]): Promise<MessagingPerson[]> {
  if (!ids.length) return [];
  const env = await runtimeEnv();
  return (
    await env.CONTROL_DB.prepare(
      `SELECT id,name,email FROM "user" WHERE id IN (${ids.map(() => "?").join(",")}) ORDER BY name,id`,
    )
      .bind(...ids)
      .all<MessagingPerson>()
  ).results;
}
export async function messagingPeople(
  db: SqlDatabase,
  actor: WorkContext,
  query = "",
  teamId?: string,
) {
  if (teamId) await team(db, actor, teamId);
  const ids = (
    await db
      .prepare(
        `SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? ${teamId ? "AND EXISTS(SELECT 1 FROM messaging_team_members tm WHERE tm.team_id=? AND tm.user_id=a.user_id)" : ""}`,
      )
      .bind(
        actor.companyId,
        Date.now(),
        Date.now(),
        ...(teamId ? [teamId] : []),
      )
      .all<{ user_id: string }>()
  ).results.map((r) => r.user_id);
  const people = await accountNames(ids);
  return people
    .filter((p) =>
      `${p.name} ${p.email}`.toLowerCase().includes(query.toLowerCase()),
    )
    .slice(0, 100);
}
async function memberGuards(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  ids: string[],
  teamId?: string,
) {
  if ((await accountNames(ids)).length !== ids.length)
    throw new ApiError(
      422,
      "MEMBER_UNAVAILABLE",
      "Select active user accounts.",
    );
  return ids.map((id, i) =>
    assertion(
      db,
      key,
      `member-${i}`,
      `EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)${teamId ? " AND EXISTS(SELECT 1 FROM messaging_team_members WHERE team_id=? AND user_id=?)" : ""}`,
      [actor.companyId, id, ...(teamId ? [teamId, id] : [])],
    ),
  );
}
function insertMembers(
  db: SqlDatabase,
  table: "messaging_room_members" | "messaging_team_members",
  id: string,
  members: string[],
) {
  const column = table === "messaging_room_members" ? "room_id" : "team_id";
  return members.map((user) =>
    db
      .prepare(
        `INSERT INTO ${table}(${column},user_id) VALUES(?,?) ON CONFLICT DO NOTHING`,
      )
      .bind(id, user),
  );
}
export async function createMessagingRoom(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof roomSchema>,
  key: string,
) {
  const members = [...new Set([actor.userId, ...input.members])].sort();
  if (input.kind === "direct" && members.length !== 2)
    throw new ApiError(422, "RECIPIENT_REQUIRED", "Select one other person.");
  if (input.kind === "group" && (!input.name || members.length < 2))
    throw new ApiError(
      422,
      "GROUP_REQUIRED",
      "Name the group and select at least one person.",
    );
  const directKey = input.kind === "direct" ? JSON.stringify(members) : null;
  // Stable room IDs and a unique pair key resolve simultaneous first conversations.
  const id = directKey
    ? `dm-${await digest(actor.companyId + ":" + directKey)}`
    : `room-${key}`;
  const steps = await memberGuards(db, actor, key, members);
  return executeCommand(
    db,
    actor,
    key,
    "messaging.create",
    input,
    id,
    1,
    { id },
    [
      ...steps,
      db
        .prepare(
          "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,direct_key,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          id,
          actor.companyId,
          input.kind,
          input.name,
          actor.userId,
          directKey,
          Date.now(),
        ),
      ...insertMembers(db, "messaging_room_members", id, members),
    ],
  );
}
async function digest(value: string) {
  return Buffer.from(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value)),
  ).toString("hex");
}
export async function createMessagingTeam(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof teamSchema>,
  key: string,
) {
  const id = `team-${key}`,
    roomId = `general-${key}`,
    members = [...new Set([actor.userId, ...input.members])];
  return executeCommand(
    db,
    actor,
    key,
    "messaging.team-create",
    input,
    id,
    1,
    { id, roomId },
    [
      ...(await memberGuards(db, actor, key, members)),
      db
        .prepare(
          "INSERT INTO messaging_teams(id,company_id,name,owner_id,created_at) VALUES(?,?,?,?,?)",
        )
        .bind(id, actor.companyId, input.name, actor.userId, Date.now()),
      ...insertMembers(db, "messaging_team_members", id, members),
      db
        .prepare(
          "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,created_at) VALUES(?,?,'channel','General',?,?,?)",
        )
        .bind(roomId, actor.companyId, actor.userId, id, Date.now()),
    ],
  );
}
export async function createMessagingChannel(
  db: SqlDatabase,
  actor: WorkContext,
  teamId: string,
  input: z.infer<typeof channelSchema>,
  key: string,
) {
  await team(db, actor, teamId, true);
  const id = `channel-${key}`,
    members = [...new Set([actor.userId, ...input.members])];
  return executeCommand(
    db,
    actor,
    key,
    "messaging.channel-create",
    input,
    id,
    1,
    { id },
    [
      assertion(
        db,
        key,
        "team-owner",
        "EXISTS(SELECT 1 FROM messaging_teams WHERE id=? AND company_id=? AND owner_id=?)",
        [teamId, actor.companyId, actor.userId],
      ),
      ...(await memberGuards(db, actor, key, members, teamId)),
      db
        .prepare(
          "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,private,created_at) VALUES(?,?,'channel',?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          input.name,
          actor.userId,
          teamId,
          input.private,
          Date.now(),
        ),
      ...insertMembers(
        db,
        "messaging_room_members",
        id,
        input.private ? members : [],
      ),
    ],
  );
}
export async function messagingMembers(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  isTeam: boolean,
) {
  const row = isTeam
    ? await team(db, actor, id)
    : await messagingRoom(db, actor, id);
  if ((row as MessagingRoom).organization_node_id || (row as MessagingTeam).organization_managed) {
    const ids=(await db.prepare(`SELECT DISTINCT p.user_id FROM hr_people p JOIN organization_nodes n ON n.company_id=p.company_id WHERE p.company_id=? AND p.user_id IS NOT NULL AND n.status='active' AND ${isTeam?"n.kind='company'":"n.id=?"} AND ${organizationMemberSql("n","p.user_id")}`).bind(actor.companyId,...(isTeam?[]:[(row as MessagingRoom).organization_node_id])).all<{user_id:string}>()).results.map(p=>p.user_id);
    return {...row,members:await accountNames(ids)};
  }
  const channelTeam =
    !isTeam &&
    (row as MessagingRoom).kind === "channel" &&
    !(row as MessagingRoom).private
      ? (row as MessagingRoom).team_id
      : null;
  const table =
      isTeam || channelTeam
        ? "messaging_team_members"
        : "messaging_room_members",
    column = isTeam || channelTeam ? "team_id" : "room_id";
  const ids = (
    await db
      .prepare(`SELECT user_id FROM ${table} WHERE ${column}=?`)
      .bind(channelTeam || id)
      .all<{ user_id: string }>()
  ).results.map((r) => r.user_id);
  return { ...row, members: await accountNames(ids) };
}
export async function manageMessaging(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  isTeam: boolean,
  input: z.infer<typeof manageSchema>,
  key: string,
) {
  const row = isTeam
    ? await team(db, actor, id, true)
    : await messagingRoom(db, actor, id);
  if (
    row.owner_id !== actor.userId ||
    (!isTeam && (row as MessagingRoom).kind === "direct")
  )
    throw new ApiError(
      403,
      "OWNER_REQUIRED",
      "Only the owner can manage this conversation.",
    );
  const room = row as MessagingRoom;
  const members = [...new Set([row.owner_id, ...input.members])];
  const table = isTeam ? "messaging_teams" : "messaging_rooms",
    memberTable = isTeam ? "messaging_team_members" : "messaging_room_members",
    column = isTeam ? "team_id" : "room_id";
  const steps: SqlStatement[] = [
    ...(await memberGuards(
      db,
      actor,
      key,
      members,
      !isTeam && room.kind === "channel" ? room.team_id! : undefined,
    )),
    assertion(
      db,
      key,
      "owner-version",
      `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND owner_id=? AND version=?)`,
      [id, actor.companyId, actor.userId, input.expectedVersion],
    ),
    db
      .prepare(`UPDATE ${table} SET name=?,version=version+1 WHERE id=?`)
      .bind(input.name, id),
  ];
  if (isTeam || room.kind === "group" || room.private) {
    steps.push(
      db
        .prepare(
          `DELETE FROM ${memberTable} WHERE ${column}=? AND user_id NOT IN (${members.map(() => "?").join(",")})`,
        )
        .bind(id, ...members),
      ...insertMembers(db, memberTable, id, members),
    );
  }
  if (isTeam)
    steps.push(
      db
        .prepare(
          `DELETE FROM messaging_room_members WHERE room_id IN (SELECT id FROM messaging_rooms WHERE team_id=?) AND user_id NOT IN (${members.map(() => "?").join(",")})`,
        )
        .bind(id, ...members),
    );
  if (!isTeam) steps.unshift(messagingGuard(db, actor, key, id));
  return executeCommand(
    db,
    actor,
    key,
    "messaging.manage",
    { ...input, isTeam },
    id,
    input.expectedVersion + 1,
    { id },
    steps,
  );
}
export async function messagingOverview(db: SqlDatabase, actor: WorkContext) {
  await ensureOrganizationChannels(db,actor.companyId);
  const teams = (
    await db
      .prepare(
        `SELECT t.* FROM messaging_teams t WHERE t.company_id=? AND ((NOT t.organization_managed AND EXISTS(SELECT 1 FROM messaging_team_members m WHERE m.team_id=t.id AND m.user_id=?)) OR (t.organization_managed AND EXISTS(SELECT 1 FROM organization_nodes n WHERE n.company_id=t.company_id AND n.kind='company' AND n.status='active' AND ${organizationMemberSql("n","?")}))) ORDER BY t.name,t.id`,
      )
      .bind(actor.companyId, actor.userId, actor.userId)
      .all<MessagingTeam>()
  ).results;
  const rooms = (
    await db
      .prepare(
        `SELECT r.*,
    (SELECT count(*) FROM messaging_messages msg WHERE msg.room_id=r.id AND msg.sender_id<>? AND msg.seq>COALESCE((SELECT rd.seq FROM messaging_reads rd WHERE rd.room_id=r.id AND rd.user_id=?),0)) unread,
    (SELECT CASE WHEN msg.body='' THEN 'Attachment' ELSE left(msg.body,160) END FROM messaging_messages msg WHERE msg.room_id=r.id ORDER BY msg.seq DESC LIMIT 1) preview,
    COALESCE((SELECT msg.created_at FROM messaging_messages msg WHERE msg.room_id=r.id ORDER BY msg.seq DESC LIMIT 1),r.created_at) updated_at
    FROM messaging_rooms r WHERE r.company_id=? AND ${access} ORDER BY updated_at DESC,r.id`,
      )
      .bind(actor.userId, actor.userId, actor.companyId, ...accessArgs(actor))
      .all<MessagingRoom>()
  ).results;
  const directRooms = rooms.filter((r) => r.kind === "direct");
  if (directRooms.length) {
    const members = (
      await db
        .prepare(
          `SELECT room_id,user_id FROM messaging_room_members WHERE room_id IN (${directRooms.map(() => "?").join(",")}) AND user_id<>?`,
        )
        .bind(...directRooms.map((r) => r.id), actor.userId)
        .all<{ room_id: string; user_id: string }>()
    ).results;
    const people = await accountNames([
      ...new Set(members.map((m) => m.user_id)),
    ]);
    for (const room of directRooms)
      room.name =
        people.find(
          (p) => p.id === members.find((m) => m.room_id === room.id)?.user_id,
        )?.name ?? "Former member";
  }
  return { teams, rooms };
}
export async function messagingHistory(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  before?: number,
  after?: number,
) {
  const room = await messagingRoom(db, actor, id);
  const rows = (
    await db
      .prepare(
        `SELECT msg.* FROM messaging_messages msg JOIN messaging_rooms r ON r.id=msg.room_id WHERE r.id=? AND r.company_id=? AND ${access} ${before !== undefined ? "AND msg.seq<?" : after !== undefined ? "AND msg.seq>?" : ""} ORDER BY msg.seq ${after !== undefined ? "ASC" : "DESC"} LIMIT 51`,
      )
      .bind(
        id,
        actor.companyId,
        ...accessArgs(actor),
        ...(before !== undefined
          ? [before]
          : after !== undefined
            ? [after]
            : []),
      )
      .all<MessagingMessage>()
  ).results;
  const hasMore = rows.length > 50,
    messages = rows.slice(0, 50);
  if (after === undefined) messages.reverse();
  for (const msg of messages) msg.files = [];
  if (messages.length) {
    const files = (
      await db
        .prepare(
          `SELECT f.id,f.original_name,f.media_type,f.bytes,f.message_id FROM messaging_files f JOIN messaging_rooms r ON r.id=f.room_id WHERE r.company_id=? AND ${access} AND f.message_id IN (${messages.map(() => "?").join(",")})`,
        )
        .bind(
          actor.companyId,
          ...accessArgs(actor),
          ...messages.map((m) => m.id),
        )
        .all<MessagingFile & { message_id: string }>()
    ).results;
    for (const msg of messages)
      msg.files = files.filter((f) => f.message_id === msg.id);
  }
  return { room, messages, hasMore };
}
export async function sendMessagingMessage(
  db: SqlDatabase,
  actor: WorkContext,
  roomId: string,
  input: z.infer<typeof messageSchema>,
  key: string,
) {
  await messagingRoom(db, actor, roomId);
  const id = `message-${key}`;
  const steps = [messagingGuard(db, actor, key, roomId)];
  input.files.forEach((file, i) =>
    steps.push(
      assertion(
        db,
        key,
        `file-${i}`,
        "EXISTS(SELECT 1 FROM messaging_files WHERE id=? AND room_id=? AND created_by=? AND message_id IS NULL)",
        [file, roomId, actor.userId],
      ),
    ),
  );
  steps.push(
    db
      .prepare("UPDATE messaging_rooms SET last_seq=last_seq+1 WHERE id=?")
      .bind(roomId),
    db
      .prepare(
        "INSERT INTO messaging_messages(id,room_id,seq,sender_id,sender_name,body,created_at) SELECT ?,id,last_seq,?,?,?,? FROM messaging_rooms WHERE id=?",
      )
      .bind(id, actor.userId, actor.name, input.body, Date.now(), roomId),
  );
  for (const file of input.files)
    steps.push(
      db
        .prepare("UPDATE messaging_files SET message_id=? WHERE id=?")
        .bind(id, file),
    );
  return executeCommand(
    db,
    actor,
    key,
    "messaging.send",
    input,
    roomId,
    1,
    { id },
    steps,
  );
}
export async function readMessaging(
  db: SqlDatabase,
  actor: WorkContext,
  roomId: string,
  seq: number,
) {
  await messagingRoom(db, actor, roomId);
  await db
    .prepare(
      `INSERT INTO messaging_reads(room_id,user_id,seq) SELECT r.id,?,LEAST(?,r.last_seq) FROM messaging_rooms r WHERE r.id=? AND r.company_id=? AND ${access} ON CONFLICT(room_id,user_id) DO UPDATE SET seq=GREATEST(messaging_reads.seq,excluded.seq)`,
    )
    .bind(actor.userId, seq, roomId, actor.companyId, ...accessArgs(actor))
    .run();
  return { ok: true };
}
export async function messagingFile(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
) {
  const file = await db
    .prepare(
      `SELECT f.* FROM messaging_files f JOIN messaging_rooms r ON r.id=f.room_id WHERE f.id=? AND r.company_id=? AND ${access} AND (f.message_id IS NOT NULL OR f.created_by=?)`,
    )
    .bind(id, actor.companyId, ...accessArgs(actor), actor.userId)
    .first<MessagingFile & { object_key: string; room_id: string }>();
  return file ?? missing();
}
