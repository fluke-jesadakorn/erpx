import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import {
  expenseFixture,
  expenseDb as db,
  key,
} from "./helpers/expense-fixture";
import {
  createMessagingRoom,
  createMessagingTeam,
  createMessagingChannel,
  manageMessaging,
  messagingHistory,
  messagingOverview,
  messagingPeople,
  messagingFile,
  messagingRoom,
  readMessaging,
  sendMessagingMessage,
  messageSchema,
} from "../src/server/messaging";
import { uploadMessagingFile } from "../src/server/messaging-files";

vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: db, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("../src/server/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({
    enabled: true,
    ready: true,
    engine: "ClamAV messaging test",
    definitionUpdatedAt: Date.now(),
    error: null,
  }),
  scanAttachmentBytes: async () => ({
    clean: true,
    signature: null,
    engine: "ClamAV messaging test",
    definitionUpdatedAt: Date.now(),
  }),
}));
vi.mock("../src/server/attachments/preview", async () => {
  const actual = await vi.importActual<
    typeof import("../src/server/attachments/preview")
  >("../src/server/attachments/preview");
  return {
    ...actual,
    attachmentPreviewReadiness: async () => ({
      ready: true,
      heifConvert: "/test/heif-convert",
      pdftoppm: "/test/pdftoppm",
      error: null,
    }),
    sanitizedAttachmentPreview: async () => null,
  };
});
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  for (const actor of [f.claimant, f.head, f.owner, f.reviewer])
    await env.CONTROL_DB.prepare(
      'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,1,1)',
    )
      .bind(actor.userId, actor.name, `${actor.userId}@example.invalid`)
      .run();
});
const send = (
  room: string,
  body: string,
  actor = f.claimant,
  id = key(),
  files: string[] = [],
) =>
  sendMessagingMessage(
    db,
    actor,
    room,
    messageSchema.parse({ body, files }),
    id,
  );
const direct = () =>
  createMessagingRoom(
    db,
    f.claimant,
    { kind: "direct", name: "", members: [f.head.userId] },
    key(),
  );

it("uses real company accounts, reuses direct pairs, persists messages and tracks reading", async () => {
  expect((await messagingPeople(db, f.claimant)).map((p) => p.id)).toContain(
    f.head.userId,
  );
  const [a, b] = await Promise.all([
    direct(),
    createMessagingRoom(
      db,
      f.head,
      { kind: "direct", name: "", members: [f.claimant.userId] },
      key(),
    ),
  ]);
  expect(a.id).toBe(b.id);
  const id = key();
  await send(a.id, "Hello", f.claimant, id);
  await send(a.id, "Hello", f.claimant, id);
  await expect(send(a.id, "Changed", f.claimant, id)).rejects.toMatchObject({
    status: 409,
  });
  let history = await messagingHistory(db, f.head, a.id);
  expect(history.messages.map((m) => m.body)).toEqual(["Hello"]);
  expect((await messagingOverview(db, f.head)).rooms.find(r=>r.id===a.id)?.unread).toBe(1);
  await readMessaging(db, f.head, a.id, 1);
  await readMessaging(db, f.head, a.id, 0);
  expect((await messagingOverview(db, f.head)).rooms.find(r=>r.id===a.id)?.unread).toBe(0);
  await send(a.id, "Hi back", f.head);
  history = await messagingHistory(db, f.claimant, a.id, undefined, 1);
  expect(history.messages[0].body).toBe("Hi back");
  await expect(messagingHistory(db, f.owner, a.id)).rejects.toMatchObject({
    status: 404,
  });
  await expect(
    messagingHistory(db, { ...f.head, companyId: "another-company" }, a.id),
  ).rejects.toMatchObject({ status: 404 });
});
it("paginates without gaps and orders concurrent sends", async () => {
  const { id } = await direct();
  for (let i = 0; i < 54; i++) await send(id, `Message ${i}`);
  await Promise.all([send(id, "Concurrent A"), send(id, "Concurrent B")]);
  const latest = await messagingHistory(db, f.head, id);
  expect(latest.hasMore).toBe(true);
  expect(latest.messages).toHaveLength(50);
  const earlier = await messagingHistory(
    db,
    f.head,
    id,
    latest.messages[0].seq,
  );
  expect(earlier.messages).toHaveLength(6);
  expect(
    new Set([...earlier.messages, ...latest.messages].map((m) => m.seq)).size,
  ).toBe(56);
});
it("manages groups with owner and version checks; revoked users lose access", async () => {
  const { id } = await createMessagingRoom(
    db,
    f.claimant,
    { kind: "group", name: "Project", members: [f.head.userId] },
    key(),
  );
  await send(id, "History");
  await expect(
    manageMessaging(
      db,
      f.head,
      id,
      false,
      { name: "Hijack", members: [], expectedVersion: 1 },
      key(),
    ),
  ).rejects.toMatchObject({ status: 403 });
  await manageMessaging(
    db,
    f.claimant,
    id,
    false,
    { name: "Project", members: [f.reviewer.userId], expectedVersion: 1 },
    key(),
  );
  expect((await messagingHistory(db, f.reviewer, id)).messages[0].body).toBe(
    "History",
  );
  await expect(messagingHistory(db, f.head, id)).rejects.toMatchObject({
    status: 404,
  });
  await expect(
    manageMessaging(
      db,
      f.claimant,
      id,
      false,
      { name: "Stale", members: [], expectedVersion: 1 },
      key(),
    ),
  ).rejects.toMatchObject({ status: 409 });
  await db
    .prepare("UPDATE assignments SET revoked_at=1 WHERE user_id=?")
    .bind(f.reviewer.userId)
    .run();
  await expect(messagingRoom(db, f.reviewer, id)).rejects.toMatchObject({
    status: 404,
  });
});
it("creates General and isolates private channels; team removal also clears private membership", async () => {
  const t = await createMessagingTeam(
    db,
    f.claimant,
    { name: "Engineering", members: [f.head.userId, f.reviewer.userId] },
    key(),
  );
  expect((await messagingHistory(db, f.head, t.roomId)).room.name).toBe(
    "General",
  );
  const channel = await createMessagingChannel(
    db,
    f.claimant,
    t.id,
    { name: "Planning", private: true, members: [f.head.userId] },
    key(),
  );
  await send(channel.id, "Private");
  await expect(
    messagingHistory(db, f.reviewer, channel.id),
  ).rejects.toMatchObject({ status: 404 });
  expect(
    (await messagingOverview(db, f.reviewer)).rooms.map((r) => r.id),
  ).not.toContain(channel.id);
  await expect(
    createMessagingChannel(
      db,
      f.head,
      t.id,
      { name: "No", private: false, members: [] },
      key(),
    ),
  ).rejects.toMatchObject({ status: 403 });
  await manageMessaging(
    db,
    f.claimant,
    t.id,
    true,
    { name: "Engineering", members: [f.reviewer.userId], expectedVersion: 1 },
    key(),
  );
  await expect(messagingHistory(db, f.head, channel.id)).rejects.toMatchObject({
    status: 404,
  });
  await expect(messagingHistory(db, f.head, t.roomId)).rejects.toMatchObject({
    status: 404,
  });
  await manageMessaging(
    db,
    f.claimant,
    t.id,
    true,
    {
      name: "Engineering",
      members: [f.reviewer.userId, f.head.userId],
      expectedVersion: 2,
    },
    key(),
  );
  await expect(messagingHistory(db, f.head, channel.id)).rejects.toMatchObject({
    status: 404,
  });
  await manageMessaging(
    db,
    f.claimant,
    channel.id,
    false,
    { name: "Planning", members: [f.head.userId], expectedVersion: 1 },
    key(),
  );
  expect(
    (await messagingHistory(db, f.head, channel.id)).messages[0].body,
  ).toBe("Private");
});
it("keeps draft files private, binds attachments once, and revokes downloads with membership", async () => {
  const previousGate = process.env.ERPX_ATTACHMENTS_V2;
  const previousReadOnly = process.env.ERPX_ATTACHMENTS_READ_ONLY;
  process.env.ERPX_ATTACHMENTS_V2 = "false";
  delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  const t = await createMessagingTeam(
    db,
    f.claimant,
    { name: "Files", members: [f.head.userId] },
    key(),
  );
  const id = key(),
    input = { id, name: "notes.csv", type: "text/csv" as const };
  const upload = () =>
    uploadMessagingFile(
      db,
      env.H001_FILES,
      f.claimant,
      t.roomId,
      input,
      new Request("http://localhost/upload", {
        method: "PUT",
        body: "a,b\n1,2",
      }),
    );
  const completed = await upload();
  expect(completed).toMatchObject({ state: "available" });
  expect(
    await db
      .prepare("SELECT source_state FROM attachments WHERE id=?")
      .bind(completed.sourceId)
      .first("source_state"),
  ).toBe("available");
  await upload();
  await expect(messagingFile(db, f.head, id)).rejects.toMatchObject({
    status: 404,
  });
  await expect(
    send(t.roomId, "Stolen", f.head, key(), [id]),
  ).rejects.toMatchObject({ status: 409 });
  await send(t.roomId, "", f.claimant, key(), [id]);
  expect(
    (await messagingHistory(db, f.head, t.roomId)).messages[0].files[0].id,
  ).toBe(id);
  const file = await messagingFile(db, f.head, id);
  expect(await (await env.H001_FILES.get(file.object_key))?.text()).toBe(
    "a,b\n1,2",
  );
  await expect(
    send(t.roomId, "Again", f.claimant, key(), [id]),
  ).rejects.toMatchObject({ status: 409 });
  await manageMessaging(
    db,
    f.claimant,
    t.id,
    true,
    { name: "Files", members: [], expectedVersion: 1 },
    key(),
  );
  await expect(messagingFile(db, f.head, id)).rejects.toMatchObject({
    status: 404,
  });
  await expect(
    uploadMessagingFile(
      db,
      env.H001_FILES,
      f.head,
      t.roomId,
      { ...input, id: key() },
      new Request("http://localhost/upload", { method: "PUT", body: "a,b" }),
    ),
  ).rejects.toMatchObject({ status: 404 });
  if (previousGate === undefined) delete process.env.ERPX_ATTACHMENTS_V2;
  else process.env.ERPX_ATTACHMENTS_V2 = previousGate;
  if (previousReadOnly === undefined)
    delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  else process.env.ERPX_ATTACHMENTS_READ_ONLY = previousReadOnly;
});

it("blocks message file intake without mutating rows in rollback mode", async () => {
  const previousGate = process.env.ERPX_ATTACHMENTS_V2;
  const previousReadOnly = process.env.ERPX_ATTACHMENTS_READ_ONLY;
  const { id } = await direct();
  const beforeFiles = Number(
    await db.prepare("SELECT count(*) FROM messaging_files").first("count"),
  );
  const beforeSources = Number(
    await db.prepare("SELECT count(*) FROM attachments").first("count"),
  );
  process.env.ERPX_ATTACHMENTS_V2 = "false";
  process.env.ERPX_ATTACHMENTS_READ_ONLY = "true";
  try {
    await expect(
      uploadMessagingFile(
        db,
        env.H001_FILES,
        f.claimant,
        id,
        { id: key(), name: "blocked.csv", type: "text/csv" },
        new Request("http://localhost/upload", {
          method: "PUT",
          body: "a,b\n1,2",
        }),
      ),
    ).rejects.toMatchObject({ status: 503, code: "ATTACHMENTS_READ_ONLY" });
    expect(
      Number(await db.prepare("SELECT count(*) FROM messaging_files").first("count")),
    ).toBe(beforeFiles);
    expect(
      Number(await db.prepare("SELECT count(*) FROM attachments").first("count")),
    ).toBe(beforeSources);
  } finally {
    if (previousGate === undefined) delete process.env.ERPX_ATTACHMENTS_V2;
    else process.env.ERPX_ATTACHMENTS_V2 = previousGate;
    if (previousReadOnly === undefined)
      delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
    else process.env.ERPX_ATTACHMENTS_READ_ONLY = previousReadOnly;
  }
});
it("rejects invalid content, oversize files, nonexistent and other-company recipients", async () => {
  const { id } = await direct();
  expect(() => messageSchema.parse({ body: " " })).toThrow();
  expect(() => messageSchema.parse({ body: "x".repeat(5001) })).toThrow();
  expect(() =>
    messageSchema.parse({ body: "", files: Array.from({ length: 10 }, key) }),
  ).not.toThrow();
  expect(() =>
    messageSchema.parse({ body: "", files: Array.from({ length: 11 }, key) }),
  ).toThrow();
  await expect(
    uploadMessagingFile(
      db,
      env.H001_FILES,
      f.claimant,
      id,
      { id: key(), name: "fake.png", type: "image/png" },
      new Request("http://localhost/upload", {
        method: "PUT",
        body: "not a png",
      }),
    ),
  ).rejects.toMatchObject({ status: 422 });
  await expect(
    uploadMessagingFile(
      db,
      env.H001_FILES,
      f.claimant,
      id,
      { id: key(), name: "big.csv", type: "text/csv" },
      new Request("http://localhost/upload", {
        method: "PUT",
        headers: { "content-length": String(11 * 1024 * 1024) },
        body: "abc",
      }),
    ),
  ).rejects.toMatchObject({ status: 422 });
  await expect(
    createMessagingRoom(
      db,
      f.claimant,
      { kind: "direct", name: "", members: ["nonexistent"] },
      key(),
    ),
  ).rejects.toMatchObject({ status: 422 });
  await db
    .prepare("UPDATE assignments SET company_id=? WHERE user_id=?")
    .bind("other", f.reviewer.userId)
    .run();
  await expect(
    createMessagingRoom(
      db,
      f.claimant,
      { kind: "direct", name: "", members: [f.reviewer.userId] },
      key(),
    ),
  ).rejects.toMatchObject({ status: 409 });
});
