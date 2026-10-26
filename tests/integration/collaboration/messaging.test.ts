import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import {
  expenseFixture,
  expenseDb as db,
  key,
} from "../../support/helpers/expense-fixture";
import {createMessagingRoom,createMessagingTeam,createMessagingChannel,manageMessaging,messagingHistory,messagingOverview,messagingPeople,messagingFile,readMessaging,sendMessagingMessage,messageSchema} from '@erpx/server-collaboration/public/messaging';
import {messagingRoom} from '@erpx/server-collaboration/public/messaging-read';
import {attachmentContent,createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,scanAttachmentNow,withdrawAttachmentLink} from '@erpx/server-files/public/attachments/service';
import type {FileStore} from '@erpx/server-runtime/public/runtime/types';
import { uploadMessagingFile,discardMessagingFile } from "@erpx/server-collaboration/public/messaging-files";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: db, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("@erpx/server-files/public/attachments/scanner", () => ({
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
vi.mock("@erpx/server-files/public/attachments/preview", async () => {
  const actual = await vi.importActual<
    typeof import("@erpx/server-files/public/attachments/preview")
  >("@erpx/server-files/public/attachments/preview");
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
import {packageGrantStore,readCompanyPackageGrants,packageGrantRevision} from '@erpx/server-commercial/public/package-grants';
import {withOwnerPackageObligations,ownerPackageObligations} from '@erpx/server-commercial/public/owner-package-obligations';
afterEach(()=>vi.unstubAllEnvs());
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


async function messagesGrant(packageId:'work'|'work-messages',state:'active'|'draining'='active'){
 const previous=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId===packageId);return packageGrantStore(db,{authorize:async()=>({userId:'isolated-messages-provider'}),obligations:async()=>{throw Error('MESSAGES_TEST_CANNOT_FABRICATE_WITHDRAWAL_ZERO');}}).apply({expectedVersion:previous?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(previous?.version??0)+1,reason:'Actual original Messages owner fixture',evidence:'test:messages-owned-boundary'}});
}
it('purchasedMessages keeps the independent account picker and original module-empty conversations but requires actual Messages purchase for intake and current history',async()=>{
 const old=await direct();await send(old.id,'Retained original conversation');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');expect(f.claimant.modules).toEqual([]);expect((await messagingPeople(db,f.claimant)).map(row=>row.id)).toContain(f.head.userId);expect(await messagingOverview(db,f.claimant)).toMatchObject({teams:[],rooms:[],purchase:{newWork:false,history:false}});await expect(messagingHistory(db,f.claimant,old.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(direct()).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await messagesGrant('work');await messagesGrant('work-messages');expect((await messagingHistory(db,f.claimant,old.id)).messages.map(row=>row.body)).toEqual(['Retained original conversation']);const messageKey=key(),input=messageSchema.parse({body:'Original exact Messages submission'}),result=await sendMessagingMessage(db,f.claimant,old.id,input,messageKey);await messagesGrant('work-messages','draining');await expect(send(old.id,'New draining communication')).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await expect(createMessagingTeam(db,f.claimant,{name:'New draining team',members:[]},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});expect(await sendMessagingMessage(db,f.claimant,old.id,input,messageKey)).toEqual(result);await readMessaging(db,f.claimant,old.id,2);expect((await messagingHistory(db,f.claimant,old.id)).messages.map(row=>row.body)).toContain(input.body);await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();await expect(sendMessagingMessage(db,f.claimant,old.id,input,messageKey)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
});
it('purchasedMessages allows only original membership reductions while draining and keeps inactive provisioning out of read-only overview',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const group=await createMessagingRoom(db,f.claimant,{kind:'group',name:'Retained group',members:[f.head.userId,f.owner.userId]},key());await messagesGrant('work-messages','draining');const before=await db.prepare('SELECT count(*) n FROM messaging_rooms WHERE company_id=?').bind(f.company).first('n');await messagingOverview(db,f.claimant);expect(await db.prepare('SELECT count(*) n FROM messaging_rooms WHERE company_id=?').bind(f.company).first('n')).toBe(before);await expect(manageMessaging(db,f.claimant,group.id,false,{name:'Retained group',members:[f.head.userId,f.owner.userId,f.reviewer.userId],expectedVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await manageMessaging(db,f.claimant,group.id,false,{name:'Retained group',members:[f.head.userId],expectedVersion:1},key());await expect(messagingHistory(db,f.owner,group.id)).rejects.toMatchObject({code:'CHAT_NOT_FOUND'});expect((await messagingOverview(db,f.claimant)).purchase).toMatchObject({newWork:false,settleExisting:true,history:true});
});
it('purchasedMessages rolls complete messages back after deferred original source or current actor changes before unchanged retry',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct();for(const kind of ['source','iam'] as const){const command=key(),body=messageSchema.parse({body:`Original private deferred ${kind} proof`}),fault=`message_final_${key().replaceAll('-','')}`,effect=kind==='source'?`DELETE FROM messaging_room_members WHERE room_id='${room.id}' AND user_id='${f.claimant.userId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);try{await expect(sendMessagingMessage(db,f.claimant,room.id,body,command)).rejects.toMatchObject({code:kind==='source'?'MESSAGING_SOURCE_CHANGED':'QUEUED_AUTHORITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}expect(await db.prepare('SELECT 1 FROM messaging_messages WHERE id=?').bind(`message-${command}`).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first('revoked_at')).toBeNull();const saved=await sendMessagingMessage(db,f.claimant,room.id,body,command);expect(await sendMessagingMessage(db,f.claimant,room.id,body,command)).toEqual(saved);}
});


it('purchasedMessages uploads and binds actual private CSV bytes in the original owner unit and retains exact upload and send history while draining',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),input={id,name:'original.csv',type:'text/csv' as const},upload=()=>uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,input,new Request('http://localhost/upload',{method:'PUT',body:'a,b\n1,2'})),original=await upload(),messageKey=key(),body=messageSchema.parse({body:'Original captured CSV delivery',files:[id]}),message=await sendMessagingMessage(db,f.claimant,room.id,body,messageKey);await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(original.sourceId).run();await messagesGrant('work-messages','draining');expect(await upload()).toEqual(original);expect(await sendMessagingMessage(db,f.claimant,room.id,body,messageKey)).toEqual(message);await expect(uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,{...input,id:key()},new Request('http://localhost/upload',{method:'PUT',body:'a,b\n3,4'}))).rejects.toMatchObject({code:'PACKAGE_DRAINING'});const retained=await attachmentContent(db,env.H001_FILES,f.head,original.attachmentLinkId,'original',{compatibilityAdapter:true});expect(await retained.object.text()).toBe('a,b\n1,2');expect(await new Response(retained.object.body).text()).toBe('a,b\n1,2');expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(original.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});
});
it('purchasedMessages buffers private bytes through actual deferred access-audit source and IAM changes with no escaped stream then preserves exact retained bytes',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),original=await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,{id,name:'private.csv',type:'text/csv'},new Request('http://localhost/upload',{method:'PUT',body:'private,csv\nexact,bytes'}));await send(room.id,'Original private attachment',f.claimant,key(),[id]);let consumed=0;const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args),get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;}},baseline=await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(original.attachmentLinkId).first('n');for(const kind of ['source','iam'] as const){const fault=`message_read_${key().replaceAll('-','')}`,effect=kind==='source'?`DELETE FROM messaging_room_members WHERE room_id='${room.id}' AND user_id='${f.head.userId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.head.assignmentId}'`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${original.attachmentLinkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);try{await expect(attachmentContent(db,files,f.head,original.attachmentLinkId,'original',{compatibilityAdapter:true})).rejects.toMatchObject({code:kind==='source'?'CHAT_NOT_FOUND':'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(original.attachmentLinkId).first('n')).toBe(baseline);expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.head.assignmentId).first('revoked_at')).toBeNull();}
 expect(consumed).toBe(2);expect(await (await attachmentContent(db,files,f.head,original.attachmentLinkId,'original',{compatibilityAdapter:true})).object.text()).toBe('private,csv\nexact,bytes');expect(consumed).toBe(3);await expect(db.transaction(tx=>attachmentContent(tx,files,f.head,original.attachmentLinkId,'original',{compatibilityAdapter:true}))).rejects.toMatchObject({code:'MESSAGING_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(3);
});

it('purchasedMessages waits for a committed real Files metadata writer before its first byte snapshot without hidden serialization retries',async()=>{
 const {committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),original=await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,{id,name:'leased.csv',type:'text/csv'},new Request('http://localhost/upload',{method:'PUT',body:'source,lease\nexact,bytes'}));let fetched=0;const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args),get:async(...args)=>{fetched++;return env.H001_FILES.get(...args);}};
 const measured=await measureReads(()=>committedSourceVisibility(db,tx=>tx.prepare("UPDATE attachment_links SET state='withdrawn',withdrawn_by=created_by,withdrawn_at=1,withdrawal_reason='Actual committed private metadata writer' WHERE id=? AND state='draft'").bind(original.attachmentLinkId).run(),async()=>{try{await attachmentContent(db,files,f.claimant,original.attachmentLinkId,'original',{compatibilityAdapter:true});return null;}catch(error){return error;}}));expect(measured.result).toMatchObject({code:'ATTACHMENT_LINK_NOT_FOUND'});expect(fetched).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(original.attachmentLinkId).first('state')).toBe('withdrawn');
});
it('purchasedMessages removes only newly written unreferenced upload objects after late original owner authority rolls the complete upload back',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),command=`chat-upload-${id}`,fault=`message_upload_${key().replaceAll('-','')}`,written:string[]=[],files:FileStore={put:async(...args)=>{const result=await env.H001_FILES.put(...args);if(result)written.push(result.key);return result;},get:(...args)=>env.H001_FILES.get(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);try{await expect(uploadMessagingFile(db,files,f.claimant,room.id,{id,name:'owned-rollback.csv',type:'text/csv'},new Request('http://localhost/upload',{method:'PUT',body:'owned,rollback\nno,orphan'}))).rejects.toMatchObject({code:'CONFLICT'});}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}expect(written.length).toBeGreaterThan(0);expect(await db.prepare('SELECT 1 FROM messaging_files WHERE id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();for(const objectKey of written)expect(await env.H001_FILES.head(objectKey)).toBeNull();expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first()).toEqual({revoked_at:null,version:1});const input={id,name:'owned-rollback.csv',type:'text/csv' as const},retry=await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,input,new Request('http://localhost/upload',{method:'PUT',body:'owned,rollback\nno,orphan'}));expect(await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,input,new Request('http://localhost/upload',{method:'PUT',body:'owned,rollback\nno,orphan'}))).toEqual(retry);
});
it('purchasedMessages settles an actual unsent attachment by original discard before normal Commercial inactive without manufacturing a message or deleting retained bytes',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),input={id,name:'unsent-owned.csv',type:'text/csv' as const},bytes='unsent,evidence\nretained,bytes',uploaded=await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,input,new Request('http://localhost/upload',{method:'PUT',body:bytes})),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(uploaded.sourceId).first('object_key'));await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(uploaded.sourceId).run();await messagesGrant('work-messages','draining');
 const scope={companyId:f.company,packageId:'work-messages',packageVersion:1},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-messages-provider'}),obligations:(tx,company,packageId)=>ownerPackageObligations(db,tx,company,packageId)}),request=async()=>{const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==='work-messages')!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Unsent attachment explicitly discarded, retain original evidence',evidence:'test:message-file-normal-withdrawal'}}]};};
 const blocked=await request();await expect(withOwnerPackageObligations(db,scope,tx=>store.batch.previewIn(tx,blocked))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(discardMessagingFile(db,f.head,id,{reason:'Another member cannot discard'},key())).rejects.toMatchObject({code:'MESSAGE_FILE_ALREADY_BOUND'});
 const command=key(),body={reason:'Discard this original unsent attachment and preserve its history'},result=await discardMessagingFile(db,f.claimant,id,body,command);expect(result).toEqual({id,state:'withdrawn'});expect(await db.prepare('SELECT message_id FROM messaging_files WHERE id=?').bind(id).first('message_id')).toBeNull();expect(await db.prepare('SELECT 1 FROM messaging_messages WHERE room_id=?').bind(room.id).first()).toBeNull();
 const next=await request(),preview=await withOwnerPackageObligations(db,scope,tx=>store.batch.previewIn(tx,next));expect(await withOwnerPackageObligations(db,scope,tx=>store.batch.applyIn(tx,next,async()=>{}))).toEqual(preview);expect(await discardMessagingFile(db,f.claimant,id,body,command)).toEqual(result);expect(await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,input,new Request('http://localhost/upload',{method:'PUT',body:bytes}))).toEqual(uploaded);expect(await (await env.H001_FILES.get(objectKey))?.text()).toBe(bytes);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(uploaded.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([['work','active'],['work-messages','inactive']]);await expect(uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,{...input,id:key()},new Request('http://localhost/upload',{method:'PUT',body:bytes}))).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();await expect(discardMessagingFile(db,f.claimant,id,body,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
});
it('purchasedMessages retains a preexisting create-only object when the complete upload fails without proof that this owner created its bytes',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),command=`chat-upload-${id}`,fault=`message_prior_object_${key().replaceAll('-','')}`,existing:string[]=[],files:FileStore={put:async(...args)=>{const result=await env.H001_FILES.put(...args);if(result)existing.push(result.key);return null;},get:(...args)=>env.H001_FILES.get(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);try{await expect(uploadMessagingFile(db,files,f.claimant,room.id,{id,name:'prior-object.csv',type:'text/csv'},new Request('http://localhost/upload',{method:'PUT',body:'preexisting,bytes\nretain,unknown'}))).rejects.toMatchObject({code:'CONFLICT'});}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}expect(existing.length).toBeGreaterThan(0);for(const objectKey of existing)expect(await (await env.H001_FILES.get(objectKey))?.text()).toBe('preexisting,bytes\nretain,unknown');expect(await db.prepare('SELECT 1 FROM messaging_files WHERE id=?').bind(id).first()).toBeNull();
});
it('purchasedMessages retains only the original real denied-read audit and rolls even that audit back after deferred current account revocation',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),uploaded=await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,{id,name:'denied-audit.csv',type:'text/csv'},new Request('http://localhost/upload',{method:'PUT',body:'actual,denied\nno,bytes'}));await send(room.id,'Original bound private evidence',f.claimant,key(),[id]);const fault=`message_denied_audit_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${uploaded.attachmentLinkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);try{await expect(attachmentContent(db,env.H001_FILES,f.owner,uploaded.attachmentLinkId,'original',{compatibilityAdapter:true})).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}expect(await db.prepare('SELECT 1 FROM attachment_access_events WHERE link_id=?').bind(uploaded.attachmentLinkId).first()).toBeNull();expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:null,version:1});await expect(attachmentContent(db,env.H001_FILES,f.owner,uploaded.attachmentLinkId,'original',{compatibilityAdapter:true})).rejects.toMatchObject({status:404});expect((await db.prepare('SELECT company_id,attachment_id,link_id,actor_id,variant,outcome,reason FROM attachment_access_events WHERE link_id=?').bind(uploaded.attachmentLinkId).all()).results).toEqual([{company_id:f.company,attachment_id:uploaded.sourceId,link_id:uploaded.attachmentLinkId,actor_id:f.owner.userId,variant:'original',outcome:'denied',reason:'CHAT_NOT_FOUND'}]);vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','disabled');await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.owner.assignmentId).run();const invalidFault=`message_invalid_denial_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${invalidFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${invalidFault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${uploaded.attachmentLinkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${invalidFault}()`);try{await expect(attachmentContent(db,env.H001_FILES,f.owner,uploaded.attachmentLinkId,'original',{compatibilityAdapter:true})).rejects.toMatchObject({code:'MESSAGING_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${invalidFault} ON attachment_access_events;DROP FUNCTION ${invalidFault}()`);}expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(uploaded.attachmentLinkId).first('n'))).toBe(1);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:1,version:2});await expect(attachmentContent(db,env.H001_FILES,f.owner,uploaded.attachmentLinkId,'original',{compatibilityAdapter:true})).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(uploaded.attachmentLinkId).first('n'))).toBe(2);
});
it('purchasedMessages rolls back the entire actual unsent withdrawal after deferred conversation or IAM changes and retries the exact original key',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const room=await direct(),id=key(),uploaded=await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,{id,name:'deferred-discard.csv',type:'text/csv'},new Request('http://localhost/upload',{method:'PUT',body:'deferred,withdrawal\nretain,evidence'})),body={reason:'Explicitly discard this actual unsent message evidence'},command=key();await messagesGrant('work-messages','draining');
 for(const kind of ['source','iam'] as const){const fault=`message_discard_${kind}_${key().replaceAll('-','')}`,effect=kind==='source'?`UPDATE messaging_rooms SET name='Unexpected deferred source' WHERE id='${room.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${uploaded.attachmentLinkId}' AND NEW.action='link.withdrawn') EXECUTE FUNCTION ${fault}()`);try{await expect(discardMessagingFile(db,f.claimant,id,body,command)).rejects.toMatchObject({status:409});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_events;DROP FUNCTION ${fault}()`);}expect(await db.prepare('SELECT state,withdrawn_by,withdrawal_reason FROM attachment_links WHERE id=?').bind(uploaded.attachmentLinkId).first()).toEqual({state:'draft',withdrawn_by:null,withdrawal_reason:null});expect(await db.prepare("SELECT 1 FROM attachment_events WHERE link_id=? AND action='link.withdrawn'").bind(uploaded.attachmentLinkId).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first()).toEqual({revoked_at:null,version:1});expect(await db.prepare('SELECT name FROM messaging_rooms WHERE id=?').bind(room.id).first('name')).not.toBe('Unexpected deferred source');}
 await expect(db.transaction(tx=>discardMessagingFile(tx,f.claimant,id,body,command))).rejects.toMatchObject({code:'MESSAGING_TRANSACTION_REQUIRED'});expect(await discardMessagingFile(db,f.claimant,id,body,command)).toEqual({id,state:'withdrawn'});expect(await discardMessagingFile(db,f.claimant,id,body,command)).toEqual({id,state:'withdrawn'});
});
it('keeps the new unsent discard fully atomic with the independent purchase switch disabled',async()=>{
 const room=await direct(),id=key(),uploaded=await uploadMessagingFile(db,env.H001_FILES,f.claimant,room.id,{id,name:'disabled-discard.csv',type:'text/csv'},new Request('http://localhost/upload',{method:'PUT',body:'disabled,atomic\noriginal,evidence'}));await expect(discardMessagingFile(db,f.claimant,id,{reason:'Actual disabled-mode discard'},'bad')).rejects.toMatchObject({status:422});expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(uploaded.attachmentLinkId).first('state')).toBe('draft');expect(await db.prepare("SELECT 1 FROM attachment_events WHERE link_id=? AND action='link.withdrawn'").bind(uploaded.attachmentLinkId).first()).toBeNull();const command=key(),body={reason:'Actual disabled-mode discard'};expect(await discardMessagingFile(db,f.claimant,id,body,command)).toEqual({id,state:'withdrawn'});expect(await discardMessagingFile(db,f.claimant,id,body,command)).toEqual({id,state:'withdrawn'});
});
it('purchasedMessages observes actual generic Files message intake before any native messaging_files row and closes only its original explicit withdrawal',async()=>{
 await messagesGrant('work');await messagesGrant('work-messages');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');const room=await direct(),set=await createAttachmentSet(db,f.claimant,{targetType:'messaging_room',targetId:room.id,componentKind:'message',componentRef:key(),purposeKey:'message_attachment',expectedTargetVersion:1},key()),bytes=new TextEncoder().encode('generic,message\nactual,evidence'),reserved=await reserveAttachmentFile(db,f.claimant,set.id,{filename:'generic-message.csv',mediaType:'text/csv',bytes:bytes.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,f.claimant,reserved.uploadIntentId,bytes.buffer);expect(await scanAttachmentNow(db,env.H001_FILES,uploaded.sourceId)).toBe('available');expect(await db.prepare('SELECT 1 FROM messaging_files WHERE attachment_link_id=?').bind(reserved.attachmentLinkId).first()).toBeNull();await messagesGrant('work-messages','draining');const scope={companyId:f.company,packageId:'work-messages',packageVersion:1},outstanding=()=>withOwnerPackageObligations(db,scope,tx=>ownerPackageObligations(db,tx,f.company,'work-messages'));expect(await outstanding()).toBe(1);await withdrawAttachmentLink(db,f.claimant,reserved.attachmentLinkId,'Explicitly discard this original generic message evidence',key());expect(await outstanding()).toBe(0);expect(await db.prepare('SELECT 1 FROM messaging_files WHERE attachment_link_id=?').bind(reserved.attachmentLinkId).first()).toBeNull();expect(await db.prepare('SELECT source_state FROM attachments WHERE id=?').bind(uploaded.sourceId).first('source_state')).toBe('available');
});


it('Messaging preserves only real unchanged quarantine and missing-object denials and rolls a late denial-source effect back without returning bytes',async()=>{
 const {enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2');enableAttachmentV2ForTest();await messagesGrant('work');await messagesGrant('work-messages');
 for(const mode of ['disabled','enforced'] as const){vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT',mode);const room=await direct(),set=await createAttachmentSet(db,f.claimant,{targetType:'messaging_room',targetId:room.id,componentKind:'message',componentRef:key(),purposeKey:'message_attachment',expectedTargetVersion:1},key()),body=new TextEncoder().encode('private,retained\nno,escape'),reserved=await reserveAttachmentFile(db,f.claimant,set.id,{filename:'original-no-byte.csv',mediaType:'text/csv',bytes:body.byteLength},key()),uploaded={...await completeAttachmentUpload(db,env.H001_FILES,f.claimant,reserved.uploadIntentId,body.buffer),attachmentLinkId:reserved.attachmentLinkId};let opened=0,missing=false;const files:FileStore={get:async(...args)=>{opened++;return missing?null:env.H001_FILES.get(...args);},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
  await expect(attachmentContent(db,files,f.claimant,uploaded.attachmentLinkId,'original')).rejects.toMatchObject({status:423,code:'ATTACHMENT_NOT_AVAILABLE'});expect(opened).toBe(0);
  expect(await scanAttachmentNow(db,env.H001_FILES,uploaded.sourceId)).toBe('available');missing=true;await expect(attachmentContent(db,files,f.claimant,uploaded.attachmentLinkId,'original')).rejects.toMatchObject({status:503,code:'FILE_UNAVAILABLE'});expect(opened).toBe(1);expect((await db.prepare('SELECT outcome,reason FROM attachment_access_events WHERE link_id=? ORDER BY reason').bind(uploaded.attachmentLinkId).all()).results).toEqual([{outcome:'denied',reason:'ATTACHMENT_NOT_AVAILABLE'},{outcome:'denied',reason:'FILE_UNAVAILABLE'}]);
  const trigger=`messaging_no_byte_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE messaging_rooms SET name='Unexpected denial source change' WHERE id='${room.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${uploaded.attachmentLinkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,files,f.claimant,uploaded.attachmentLinkId,'original')).rejects.toMatchObject({code:'MESSAGING_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(uploaded.attachmentLinkId).first('n'))).toBe(2);expect(await db.prepare('SELECT name FROM messaging_rooms WHERE id=?').bind(room.id).first('name')).not.toBe('Unexpected denial source change');missing=false;expect(await(await attachmentContent(db,files,f.claimant,uploaded.attachmentLinkId,'original')).object.text()).toBe('private,retained\nno,escape');
 }
});
