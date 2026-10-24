import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { env,applyMigrations } from "../../support/runtime-env";
import { expenseFixture,expenseDb as db,key } from "../../support/helpers/expense-fixture";
import {messagingOverview,messagingHistory,messagingFile,sendMessagingMessage,messageSchema,messagingGuard,manageMessaging} from '@erpx/server-collaboration/public/messaging';
import {messagingRoom} from '@erpx/server-collaboration/public/messaging-read';
import { executeCommand } from "@erpx/server-workflow/public/commands";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:db,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
it("derives company, branch and department channels from placement without owner privilege",async()=>{
  const data=await messagingOverview(db,f.claimant);
  expect(data.rooms.filter(r=>r.organization_node_id).map(r=>r.organization_node_id).sort()).toEqual([f.company,f.branch,f.department].sort());
  await messagingOverview(db,f.owner);
  await expect(messagingRoom(db,f.owner,`org-channel-${f.department}`)).rejects.toMatchObject({status:404});
  await expect(manageMessaging(db,f.claimant,`org-channel-${f.department}`,false,{name:"Hijack",members:[],expectedVersion:1},key())).rejects.toMatchObject({status:403});
});
it("transfers revoke old history and file access immediately and expose new channel history",async()=>{
  await messagingOverview(db,f.claimant);const old=`org-channel-${f.department}`,next=`org-channel-${f.accountsDepartment}`;
  await sendMessagingMessage(db,f.owner,next,messageSchema.parse({body:"Earlier team history"}),key());
  const file=key();await db.prepare("INSERT INTO messaging_files(id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at) VALUES(?,?,?,'note.png','image/png',8,?,'test',1)").bind(file,old,f.claimant.userId,key()).run();
  expect((await messagingFile(db,f.claimant,file)).room_id).toBe(old);
  await db.prepare("UPDATE hr_engagements SET department_id=? WHERE person_id=?").bind(f.accountsDepartment,`person-${f.claimant.userId}`).run();
  await db.prepare("INSERT INTO messaging_room_members(room_id,user_id) VALUES(?,?)").bind(old,f.claimant.userId).run();
  await expect(messagingHistory(db,f.claimant,old)).rejects.toMatchObject({status:404});
  await expect(messagingFile(db,f.claimant,file)).rejects.toMatchObject({status:404});
  expect((await messagingHistory(db,f.claimant,next)).messages[0].body).toBe("Earlier team history");
});
it("rejects a send after departure even with cached membership and retains messages",async()=>{
  await messagingOverview(db,f.claimant);const room=`org-channel-${f.department}`,command=key();
  await sendMessagingMessage(db,f.claimant,room,messageSchema.parse({body:"Recorded before departure"}),key());
  const guard=messagingGuard(db,f.claimant,command,room);
  await db.prepare("UPDATE hr_engagements SET status='ended' WHERE person_id=?").bind(`person-${f.claimant.userId}`).run();
  await expect(executeCommand(db,f.claimant,command,"messaging.test",{},room,1,{id:room},[guard])).rejects.toMatchObject({status:409});
  await expect(messagingHistory(db,f.claimant,room)).rejects.toMatchObject({status:404});
  expect((await messagingHistory(db,f.head,room)).messages[0].body).toBe("Recorded before departure");
});
