import { beforeAll,expect,it,vi } from "vitest";
import { env,applyMigrations } from "./runtime-env";
import { expenseFixture,expenseDb as db,key } from "./helpers/expense-fixture";
import { previewPositionGpo,publishPositionGpo as rawPublishPositionGpo,previewUnmapPositionGpo,unmapPositionGpo,positionGpoOverview,reconcilePositionGpo,drainPositionGpo,retryPositionGpo,retryPositionGpoConfig } from "../src/server/position-gpo";
import type { DirectoryInventory } from "../src/lib/directory-operations";
import { effectiveGpoRules, gpoRulesOverview, previewGpoRule, publishGpoRule, previewRemoveGpoRule, removeGpoRule } from "../src/server/gpo-rules";
import type { GpoRuleInput } from "../src/lib/gpo-rules";

const provider=vi.hoisted(()=>({read:vi.fn(),apply:vi.fn()}));
const mfa=vi.hoisted(()=>({check:vi.fn()}));
vi.mock("../src/server/directory-provider",async importOriginal=>({...(await importOriginal<typeof import("../src/server/directory-provider")>()),readAutomationInventory:provider.read,applyDirectoryAction:provider.apply}));
vi.mock("../src/server/iam-mfa",async importOriginal=>({...(await importOriginal<typeof import("../src/server/iam-mfa")>()),requireFreshMfa:mfa.check}));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
const inventories=new Map<string,DirectoryInventory>(),failConnections=new Set<string>(),journal=new Set<string>();
const failAfterMember=new Set<string>(),failAfterFilter=new Set<string>(),failFilterBefore=new Set<string>(),failMemberBefore=new Set<string>();
async function publishRule(s:Awaited<ReturnType<typeof setup>>,input:GpoRuleInput){
  const preview=await previewGpoRule(db,s.itActor,input);
  return publishGpoRule(db,s.itActor,{...input,previewToken:preview.token},key()) as Promise<{id:string;version:number;revision:number}>;
}
function ruleInput(s:Awaited<ReturnType<typeof setup>>,target:GpoRuleInput["target"],group="group",gpo="gpo"):GpoRuleInput {
  return {target,expectedVersion:0,connectionId:s.connection,gpoId:gpo,userFilterGroupId:group,computerTargetOuId:"ou",reason:"Assign inherited organization policy"};
}
async function publishPositionGpo(database:typeof db,actor:Parameters<typeof rawPublishPositionGpo>[1],input:Parameters<typeof previewPositionGpo>[2],commandKey:string){
  const token=(await previewPositionGpo(database,actor,input)).token;
  return rawPublishPositionGpo(database,actor,{...input as object,previewToken:token},commandKey);
}

async function setup(){
  const f=await expenseFixture(),headPerson=`person-${f.head.userId}`,ownerPerson=`person-${f.owner.userId}`;
  const assignmentVersion=await db.prepare("SELECT version FROM assignments WHERE id=?").bind(f.it.assignmentId).first<number>("version");
  const itActor={...f.it,authority:{sessionId:`test-mfa-${key()}`,revision:1,assignmentVersion:assignmentVersion!}};
  await db.prepare(`INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)
    VALUES(?,?,?,?,?,1,?,?)`).bind(itActor.authority.sessionId,f.it.userId,f.company,f.it.assignmentId,assignmentVersion,Date.now()+3600000,Date.now()).run();
  mfa.check.mockReset().mockResolvedValue(undefined);
  const rows=(await db.prepare("SELECT person_id,position_id FROM hr_engagements WHERE company_id=?").bind(f.company).all<{person_id:string;position_id:string}>()).results;
  const headPosition=rows.find(r=>r.person_id===headPerson)!.position_id,ownerPosition=rows.find(r=>r.person_id===ownerPerson)!.position_id;
  const connection=`gpo-${key()}`,now=Date.now();
  await db.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'GPO test','active_directory','{}'::jsonb,'active',?,?)").bind(connection,f.company,f.it.userId,now).run();
  await db.prepare("INSERT INTO people_directory_state(connection_id,company_id,version,tested_version,write_enabled) VALUES(?,?,1,1,true)").bind(connection,f.company).run();
  const inventory:DirectoryInventory={providerVersion:"test",observedAt:now,complete:true,fields:[],objects:[
    {id:"gpo",dn:"cn=gpo",kind:"gpo",name:"Policy",parentId:null,version:"1",module:"policies/gpo",properties:{securityFilterGroupIds:["unmanaged"]},memberIds:[]},
    {id:"gpo2",dn:"cn=gpo2",kind:"gpo",name:"Policy 2",parentId:null,version:"1",module:"policies/gpo",properties:{securityFilterGroupIds:[]},memberIds:[]},
    ...["group","group2","unmanaged"].map(id=>({id,dn:`cn=${id}`,kind:"group" as const,name:id,parentId:null,version:"1",module:"groups/group",properties:{},memberIds:[]})),
    {id:"ou",dn:"ou=computers",kind:"ou",name:"Computers",parentId:null,version:"1",module:"container/ou",properties:{gpoLinks:[{gpoId:"gpo2"}]},memberIds:[]},
    {id:"ou2",dn:"ou=other",kind:"ou",name:"Other computers",parentId:null,version:"1",module:"container/ou",properties:{gpoLinks:[]},memberIds:[]},
  ]};
  inventories.set(connection,inventory);
  provider.read.mockImplementation(async (c:{id:string})=>{
    if(failConnections.delete(c.id))throw new Error("DIRECTORY_OFFLINE");
    return structuredClone(inventories.get(c.id)!);
  });
  provider.apply.mockImplementation(async (c:{id:string},operationId:string,action:{kind:string;objectId:string;parentId?:string;groupIds?:string[];memberIds?:string[];remove?:boolean;action?:string})=>{
    const inventory=inventories.get(c.id)!;
    if(journal.has(`${c.id}:${operationId}`))return {verified:true,objectId:action.objectId,version:"journaled"};
    if(action.kind==="gpo.filter"&&failFilterBefore.delete(c.id))throw new Error("WRITE_REJECTED_BEFORE_MUTATION");
    if(action.kind==="group.members"&&action.action==="add"&&failMemberBefore.delete(c.id))throw new Error("WRITE_REJECTED_BEFORE_MUTATION");
    let changed=action.objectId;
    if(action.kind==="gpo.link"){
      const ou=inventory.objects.find(o=>o.id===action.parentId)!;
      const links=(ou.properties.gpoLinks??[]) as {gpoId:string}[];
      ou.properties.gpoLinks=action.remove?links.filter(l=>l.gpoId!==action.objectId):[...links.filter(l=>l.gpoId!==action.objectId),{gpoId:action.objectId}];
      changed=ou.id;
    }else if(action.kind==="gpo.filter")inventory.objects.find(o=>o.id===action.objectId)!.properties.securityFilterGroupIds=action.groupIds??[];
    else if(action.kind==="group.members"){
      const group=inventory.objects.find(o=>o.id===action.objectId)!;
      group.memberIds=action.action==="add"?[...new Set([...group.memberIds,...(action.memberIds??[])])]:group.memberIds.filter(id=>!action.memberIds?.includes(id));
    }
    const target=inventory.objects.find(o=>o.id===changed)!;
    target.version=String(Number(target.version)+1);
    journal.add(`${c.id}:${operationId}`);
    if(action.kind==="group.members"&&action.action==="add"&&failAfterMember.delete(c.id))failConnections.add(c.id);
    if(action.kind==="gpo.filter"&&failAfterFilter.delete(c.id))failConnections.add(c.id);
    inventory.observedAt=Date.now();
    return {verified:true,objectId:action.objectId,version:"2"};
  });
  async function link(personId:string,accountId:string){
    const userId=`gpo-user-${key()}`,identityId=`gpo-identity-${key()}`;
    await db.prepare("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id,person_id) VALUES(?,?,?,?,?,?,true,?,?,?)")
      .bind(identityId,f.company,connection,accountId,accountId,accountId,Date.now(),userId,personId).run();
    await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,connection_id,identity_id) VALUES(?,?,?,?,'active','directory',?,?)")
      .bind(f.company,userId,`principal-${key()}`,personId,connection,identityId).run();
    inventory.objects.push({id:accountId,dn:`uid=${accountId}`,kind:"account",name:accountId,parentId:null,version:"1",module:"users/user",properties:{},memberIds:[]});
  }
  function input(positionId:string,expectedVersion=0,groupId="group",gpoId="gpo",ouId="ou"){
    return {positionId,expectedVersion,connectionId:connection,gpoId,userFilterGroupId:groupId,computerTargetOuId:ouId,reason:"Map Position policy"};
  }
  const members=(groupId:string)=>inventory.objects.find(o=>o.id===groupId)!.memberIds;
  const filter=(gpoId:string)=>inventory.objects.find(o=>o.id===gpoId)!.properties.securityFilterGroupIds as string[];
  const links=(ouId:string)=>(inventory.objects.find(o=>o.id===ouId)!.properties.gpoLinks as {gpoId:string}[]).map(l=>l.gpoId);
  return {f,itActor,connection,inventory,headPerson,ownerPerson,headPosition,ownerPosition,link,input,members,filter,links};
}

it("publishes with missing links and reconciles every verified account after linkage",async()=>{
  const s=await setup();
  expect((await previewPositionGpo(db,s.itActor,s.input(s.headPosition))).holders).toEqual(expect.arrayContaining([expect.objectContaining({person_id:s.headPerson,account_id:null,accountStatus:"awaiting_account_linkage"})]));
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id IS NULL ORDER BY created_at DESC LIMIT 1").bind(s.f.company,s.headPosition).first("status")).toBe("awaiting_identity");
  await s.link(s.headPerson,"head-one");await s.link(s.headPerson,"head-two");
  await drainPositionGpo(db);
  expect(s.members("group")).toEqual(expect.arrayContaining(["head-one","head-two"]));
  expect(await db.prepare("SELECT count(*)::integer count FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id IS NOT NULL AND status='directory_applied'").bind(s.f.company,s.headPosition).first("count")).toBe(2);
  await db.prepare("UPDATE hr_engagements SET job_title='Profile edit',version=version+1 WHERE person_id=?").bind(s.headPerson).run();
  await drainPositionGpo(db);
  expect(s.members("group")).toEqual(expect.arrayContaining(["head-one","head-two"]));
  expect(await db.prepare("SELECT count(*)::integer count FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id IS NOT NULL AND desired_action='add'").bind(s.f.company,s.headPosition).first("count")).toBe(2);
});

it("combines shared GPO settings and removes only ERPX-owned filters and links",async()=>{
  const s=await setup();
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition,0,"group2","gpo","ou2"),key());
  await drainPositionGpo(db);
  expect(s.filter("gpo")).toEqual(expect.arrayContaining(["unmanaged","group","group2"]));
  expect(s.links("ou")).toEqual(expect.arrayContaining(["gpo2","gpo"]));
  expect(s.links("ou2")).toContain("gpo");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,1,"group2","gpo2","ou2"),key());
  await drainPositionGpo(db);
  expect(s.filter("gpo")).toEqual(expect.arrayContaining(["unmanaged","group2"]));
  expect(s.filter("gpo")).not.toContain("group");
  expect(s.links("ou")).toEqual(["gpo2"]);
  expect(s.links("ou2")).toEqual(expect.arrayContaining(["gpo","gpo2"]));
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition,1,"group2","gpo2","ou2"),key());
  await drainPositionGpo(db);
  expect(s.filter("gpo")).toEqual(["unmanaged"]);
  expect(s.links("ou2")).toEqual(["gpo2"]);
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,2,"group","gpo","ou"),key());
  await drainPositionGpo(db);
  expect(s.filter("gpo")).toEqual(expect.arrayContaining(["unmanaged","group"]));
  expect(s.links("ou")).toEqual(expect.arrayContaining(["gpo2","gpo"]));
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition,2,"group","gpo","ou"),key());
  await drainPositionGpo(db);
  expect(s.filter("gpo2")).toEqual([]);
  expect(s.links("ou")).toEqual(expect.arrayContaining(["gpo2","gpo"]));
});

it("preserves shared, package, and pre-existing account memberships after transfer and departure",async()=>{
  const s=await setup();
  await s.link(s.headPerson,"head-account");await s.link(s.headPerson,"preexisting-account");await s.link(s.ownerPerson,"owner-account");
  s.members("group").push("owner-account");
  s.members("group").push("preexisting-account");
  await db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,'group','owner-account','package:held',?)").bind(s.connection,Date.now()).run();
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition),key());
  await drainPositionGpo(db);
  await db.prepare("UPDATE hr_engagements SET position_id=?,version=version+1 WHERE person_id=?").bind(s.ownerPosition,s.headPerson).run();
  await reconcilePositionGpo(db);
  await db.prepare("UPDATE position_gpo_jobs SET updated_at=1 WHERE company_id=? AND account_id='head-account' AND desired_action='remove' AND status='queued'").bind(s.f.company).run();
  await drainPositionGpo(db,1);
  expect(s.members("group")).toEqual(expect.arrayContaining(["head-account","owner-account"]));
  expect(await db.prepare("SELECT count(*)::integer count FROM people_directory_membership_sources WHERE connection_id=? AND member_id='head-account' AND source_id LIKE 'position-gpo:%'").bind(s.connection).first("count")).toBe(1);
  await db.prepare("UPDATE hr_engagements SET status='ended',version=version+1 WHERE person_id=?").bind(s.headPerson).run();
  await drainPositionGpo(db);await drainPositionGpo(db);
  expect(s.members("group")).not.toContain("head-account");
  expect(s.members("group")).toContain("owner-account");
  expect(s.members("group")).toContain("preexisting-account");
  expect(await db.prepare("SELECT count(*)::integer count FROM people_directory_membership_sources WHERE connection_id=? AND member_id='preexisting-account' AND source_id='external'").bind(s.connection).first("count")).toBe(1);
  expect(await db.prepare("SELECT count(*)::integer count FROM people_directory_membership_sources WHERE connection_id=? AND member_id='owner-account' AND source_id='package:held'").bind(s.connection).first("count")).toBe(1);
});

it("waits for future effective placement and supersedes rapid mapping revisions",async()=>{
  const s=await setup();
  await s.link(s.headPerson,"future-account");
  await db.prepare("UPDATE hr_engagements SET start_date='2099-01-01',version=version+1 WHERE person_id=?").bind(s.headPerson).run();
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,1,"group2"),key());
  await drainPositionGpo(db);
  expect(s.members("group")).not.toContain("future-account");
  expect(s.members("group2")).not.toContain("future-account");
  await db.prepare("UPDATE hr_engagements SET start_date='2020-01-01',version=version+1 WHERE person_id=?").bind(s.headPerson).run();
  await drainPositionGpo(db);
  expect(s.members("group2")).toContain("future-account");
  expect(s.members("group")).not.toContain("future-account");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,2,"group"),key());
  await reconcilePositionGpo(db);
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,3,"group2"),key());
  await drainPositionGpo(db);
  expect(s.members("group2")).toContain("future-account");
  expect(s.members("group")).not.toContain("future-account");
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id='future-account' AND revision=3 AND desired_action='add'").bind(s.f.company,s.headPosition).first("status")).toBe("superseded");
});

it("converges after an HR transfer overlaps an IT remap and profile edit",async()=>{
  const s=await setup();await s.link(s.headPerson,"overlap-account");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition,0,"group2"),key());
  await drainPositionGpo(db);
  expect(s.members("group")).toContain("overlap-account");

  // HR changes the placement and queues the old IT mapping. IT publishes a new
  // mapping before the queued work is drained; an unrelated HR edit follows.
  await db.prepare("UPDATE hr_engagements SET position_id=?,version=version+1 WHERE person_id=?")
    .bind(s.ownerPosition,s.headPerson).run();
  await reconcilePositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND account_id='overlap-account' AND position_id=? AND revision=1 AND filter_group_id='group2' AND desired_action='add'")
    .bind(s.f.company,s.ownerPosition).first("status")).toBe("queued");
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition,1,"unmanaged","gpo2","ou2"),key());
  await db.prepare("UPDATE hr_engagements SET job_title='Edited during IT remap',version=version+1 WHERE person_id=?")
    .bind(s.headPerson).run();
  await drainPositionGpo(db);await drainPositionGpo(db);

  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND account_id='overlap-account' AND position_id=? AND revision=1 AND filter_group_id='group2' AND desired_action='add'")
    .bind(s.f.company,s.ownerPosition).first("status")).toBe("superseded");
  expect(s.members("group")).not.toContain("overlap-account");
  expect(s.members("group2")).not.toContain("overlap-account");
  expect(s.members("unmanaged")).toContain("overlap-account");
  expect((await db.prepare("SELECT group_id FROM people_directory_membership_sources WHERE connection_id=? AND member_id='overlap-account' AND source_id LIKE 'position-gpo:%' ORDER BY group_id")
    .bind(s.connection).all<{group_id:string}>()).results.map(r=>r.group_id)).toEqual(["unmanaged"]);
});

it("recovers configuration failure and records Directory readback separately from endpoint application",async()=>{
  const s=await setup();await s.link(s.headPerson,"retry-account");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  failConnections.add(s.connection);
  await drainPositionGpo(db);
  const policy=await db.prepare("SELECT version,directory_config_status FROM position_gpo_policies WHERE company_id=? AND position_id=?")
    .bind(s.f.company,s.headPosition).first<{version:number;directory_config_status:string}>();
  expect(policy?.directory_config_status).toBe("blocked");
  await retryPositionGpoConfig(db,s.itActor,{positionId:s.headPosition,expectedVersion:policy!.version,reason:"Directory is back"},key());
  await drainPositionGpo(db);
  let job=await db.prepare("SELECT id,version,status,directory_observed_at,endpoint_observed_at FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id='retry-account'")
    .bind(s.f.company,s.headPosition).first<{id:string;version:number;status:string;directory_observed_at:number|null;endpoint_observed_at:number|null}>();
  if(job?.status==="blocked"){
    await retryPositionGpo(db,s.itActor,{id:job.id,expectedVersion:job.version,reason:"Directory is back"},key());
    await drainPositionGpo(db);
    job=await db.prepare("SELECT id,version,status,directory_observed_at,endpoint_observed_at FROM position_gpo_jobs WHERE id=?")
      .bind(job.id).first<typeof job>();
  }
  expect(job?.status).toBe("directory_applied");
  expect(job?.directory_observed_at).toBeTruthy();
  expect(job?.endpoint_observed_at).toBeNull();
});

it("unmaps one Position without removing a shared GPO target, then allows a new revision",async()=>{
  const s=await setup();
  await s.link(s.headPerson,"unmap-head");await s.link(s.ownerPerson,"unmap-owner");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition,0,"group2","gpo","ou2"),key());
  await drainPositionGpo(db);
  const request={positionId:s.headPosition,expectedVersion:1,reason:"Position no longer needs a GPO"};
  const preview=await previewUnmapPositionGpo(db,s.itActor,request);
  expect(preview.current.userFilterGroupId).toBe("group");
  await db.prepare("UPDATE hr_people SET legal_name='Renamed after review' WHERE id=?").bind(s.headPerson).run();
  await expect(unmapPositionGpo(db,s.itActor,{...request,previewToken:preview.token},key())).rejects.toMatchObject({code:"GPO_PREVIEW_CHANGED"});
  await unmapPositionGpo(db,s.itActor,{...request,previewToken:(await previewUnmapPositionGpo(db,s.itActor,request)).token},key());
  const position=(await positionGpoOverview(db,s.itActor)).positions.find((p:{id:string})=>p.id===s.headPosition) as {mapped:boolean;mapping_status:string|null};
  expect(position.mapped).toBe(false);
  expect(position.mapping_status).toBeNull();
  await drainPositionGpo(db);
  expect(s.members("group")).not.toContain("unmap-head");
  expect(s.members("group2")).toContain("unmap-owner");
  expect(s.filter("gpo")).toEqual(expect.arrayContaining(["unmanaged","group2"]));
  expect(s.filter("gpo")).not.toContain("group");
  expect(s.links("ou2")).toContain("gpo");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,2),key());
  await drainPositionGpo(db);
  expect(s.members("group")).toContain("unmap-head");
  expect(await db.prepare("SELECT max(revision)::integer FROM position_gpo_revisions WHERE company_id=? AND position_id=?").bind(s.f.company,s.headPosition).first("max")).toBe(3);
});

it("supersedes waiting identities after departure and refuses disabled unmanaged OU links",async()=>{
  const s=await setup();
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id IS NULL ORDER BY created_at DESC LIMIT 1").bind(s.f.company,s.headPosition).first("status")).toBe("awaiting_identity");
  await db.prepare("UPDATE hr_engagements SET status='ended',version=version+1 WHERE person_id=?").bind(s.headPerson).run();
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND person_id=? AND account_id IS NULL ORDER BY created_at DESC LIMIT 1").bind(s.f.company,s.headPosition,s.headPerson).first("status")).toBe("superseded");
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND person_id<>? AND account_id IS NULL ORDER BY created_at DESC LIMIT 1").bind(s.f.company,s.headPosition,s.headPerson).first("status")).toBe("awaiting_identity");
  s.inventory.objects.find(o=>o.id==="ou2")!.properties.gpoLinks=[{gpoId:"gpo",enabled:false}];
  await publishPositionGpo(db,s.itActor,s.input(s.ownerPosition,0,"group2","gpo","ou2"),key());
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT directory_config_error FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(s.f.company,s.ownerPosition).first("directory_config_error")).toBe("GPO_LINK_DISABLED");
  expect((s.inventory.objects.find(o=>o.id==="ou2")!.properties.gpoLinks as {enabled:boolean}[])[0].enabled).toBe(false);
});

it("honors the Directory write switch after publication",async()=>{
  const s=await setup();await s.link(s.headPerson,"write-disabled-account");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await db.prepare("UPDATE people_directory_state SET write_enabled=false WHERE connection_id=?").bind(s.connection).run();
  await drainPositionGpo(db);
  expect(provider.apply.mock.calls.filter(([c]:[{id:string}])=>c.id===s.connection)).toHaveLength(0);
  expect(await db.prepare("SELECT directory_config_error FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(s.f.company,s.headPosition).first("directory_config_error")).toBe("DIRECTORY_WRITE_NOT_READY");
  expect(s.members("group")).not.toContain("write-disabled-account");
});

it("retains account provenance after a successful write with lost readback",async()=>{
  const s=await setup();await s.link(s.headPerson,"partial-account");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  failAfterMember.add(s.connection);
  await drainPositionGpo(db);
  expect(s.members("group")).toContain("partial-account");
  expect(await db.prepare("SELECT count(*)::integer count FROM people_directory_membership_sources WHERE connection_id=? AND member_id='partial-account' AND source_id LIKE 'position-gpo:%'").bind(s.connection).first("count")).toBe(1);
  const blocked=await db.prepare("SELECT id,version,status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id='partial-account'")
    .bind(s.f.company,s.headPosition).first<{id:string;version:number;status:string}>();
  expect(blocked?.status).toBe("blocked");
  await retryPositionGpo(db,s.itActor,{id:blocked!.id,expectedVersion:blocked!.version,reason:"Readback available"},key());
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT count(*)::integer count FROM people_directory_membership_sources WHERE connection_id=? AND member_id='partial-account' AND source_id='external'").bind(s.connection).first("count")).toBe(0);
  await db.prepare("UPDATE hr_engagements SET status='ended',version=version+1 WHERE person_id=?").bind(s.headPerson).run();
  await drainPositionGpo(db);
  expect(s.members("group")).not.toContain("partial-account");
});

it("treats a failed config intent followed by manual setup as unmanaged",async()=>{
  const s=await setup();
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  failFilterBefore.add(s.connection);
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_config_ownership WHERE connection_id=? AND gpo_id='gpo' AND kind='filter_group' AND target_id='group'").bind(s.connection).first("status")).toBe("pending");
  s.inventory.objects.find(o=>o.id==="gpo")!.properties.securityFilterGroupIds=["unmanaged","group"];
  s.inventory.objects.find(o=>o.id==="gpo")!.version="2";
  await drainPositionGpo(db);
  const request={positionId:s.headPosition,expectedVersion:1,reason:"Remove mapping after manual setup"};
  await unmapPositionGpo(db,s.itActor,{...request,previewToken:(await previewUnmapPositionGpo(db,s.itActor,request)).token},key());
  await drainPositionGpo(db);
  expect(s.filter("gpo")).toContain("group");
});

it("queues fresh add and remove jobs for repeated same-version account link cycles",async()=>{
  const s=await setup();await s.link(s.headPerson,"cycle-account");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  await drainPositionGpo(db);
  for(let cycle=0;cycle<2;cycle++){
    await db.prepare("UPDATE iam_members SET status='suspended' WHERE company_id=? AND person_id=? AND connection_id=?")
      .bind(s.f.company,s.headPerson,s.connection).run();
    await drainPositionGpo(db);
    expect(s.members("group")).not.toContain("cycle-account");
    await db.prepare("UPDATE iam_members SET status='active' WHERE company_id=? AND person_id=? AND connection_id=?")
      .bind(s.f.company,s.headPerson,s.connection).run();
    await drainPositionGpo(db);
    expect(s.members("group")).toContain("cycle-account");
  }
  expect(await db.prepare("SELECT count(*)::integer count FROM position_gpo_jobs WHERE company_id=? AND account_id='cycle-account' AND desired_action='add' AND status='directory_applied'").bind(s.f.company).first("count")).toBe(3);
  expect(await db.prepare("SELECT count(*)::integer count FROM position_gpo_jobs WHERE company_id=? AND account_id='cycle-account' AND desired_action='remove' AND status='directory_applied'").bind(s.f.company).first("count")).toBe(2);
});

it("does not claim a membership added manually after ERPX's pre-write failure",async()=>{
  const s=await setup();await s.link(s.headPerson,"manual-after-failure");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  failMemberBefore.add(s.connection);
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT count(*)::integer count FROM people_directory_membership_sources WHERE connection_id=? AND member_id='manual-after-failure'").bind(s.connection).first("count")).toBe(0);
  s.members("group").push("manual-after-failure");
  const blocked=await db.prepare("SELECT id,version FROM position_gpo_jobs WHERE company_id=? AND account_id='manual-after-failure' AND status='blocked'")
    .bind(s.f.company).first<{id:string;version:number}>();
  await retryPositionGpo(db,s.itActor,{id:blocked!.id,expectedVersion:blocked!.version,reason:"Manual membership observed"},key());
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT count(*)::integer count FROM people_directory_membership_sources WHERE connection_id=? AND member_id='manual-after-failure' AND source_id='external'").bind(s.connection).first("count")).toBe(1);
  await db.prepare("UPDATE iam_members SET status='suspended' WHERE company_id=? AND person_id=? AND connection_id=?")
    .bind(s.f.company,s.headPerson,s.connection).run();
  await drainPositionGpo(db);
  expect(s.members("group")).toContain("manual-after-failure");
});

it("rejects stale preview, missing permission, and stale MFA before publish",async()=>{
  const s=await setup();const request=s.input(s.headPosition);
  const preview=await previewPositionGpo(db,s.itActor,request);
  await db.prepare("UPDATE hr_engagements SET job_title='Changed after review',version=version+1 WHERE person_id=?").bind(s.headPerson).run();
  await expect(rawPublishPositionGpo(db,s.itActor,{...request,previewToken:preview.token},key())).rejects.toMatchObject({code:"GPO_PREVIEW_CHANGED"});
  const current=await previewPositionGpo(db,s.itActor,request);
  await expect(rawPublishPositionGpo(db,{...s.itActor,iam:{permissions:[]}}, {...request,previewToken:current.token},key())).rejects.toMatchObject({code:"IT_AUTHORITY_REQUIRED"});
  mfa.check.mockRejectedValueOnce(new Error("FRESH_MFA_REQUIRED"));
  await expect(rawPublishPositionGpo(db,s.itActor,{...request,previewToken:current.token},key())).rejects.toThrow("FRESH_MFA_REQUIRED");
  await rawPublishPositionGpo(db,s.itActor,{...request,previewToken:current.token},key());
});

it("confirms a returned config write after lost readback, then safely cleans it on unmap",async()=>{
  const s=await setup();
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  failAfterFilter.add(s.connection);
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_config_ownership WHERE connection_id=? AND gpo_id='gpo' AND kind='filter_group' AND target_id='group'").bind(s.connection).first("status")).toBe("written");
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_config_ownership WHERE connection_id=? AND gpo_id='gpo' AND kind='filter_group' AND target_id='group'").bind(s.connection).first("status")).toBe("confirmed");
  const request={positionId:s.headPosition,expectedVersion:1,reason:"Remove returned configuration"};
  await unmapPositionGpo(db,s.itActor,{...request,previewToken:(await previewUnmapPositionGpo(db,s.itActor,request)).token},key());
  await drainPositionGpo(db);
  expect(s.filter("gpo")).toEqual(["unmanaged"]);
});

it("supersedes an obsolete blocked account job after an IT remap",async()=>{
  const s=await setup();await s.link(s.headPerson,"old-blocked-account");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  failMemberBefore.add(s.connection);
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND account_id='old-blocked-account' AND revision=1 AND desired_action='add'").bind(s.f.company).first("status")).toBe("blocked");
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,1,"group2"),key());
  await drainPositionGpo(db);
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND account_id='old-blocked-account' AND revision=1 AND desired_action='add'").bind(s.f.company).first("status")).toBe("superseded");
  expect(s.members("group2")).toContain("old-blocked-account");
  expect(s.members("group")).not.toContain("old-blocked-account");
});

it("adds Level, Department and direct policies with independent shared-membership provenance",async()=>{
  const s=await setup();await s.link(s.headPerson,"inherited-account");
  const levelId=await db.prepare("SELECT level_id FROM organization_positions WHERE id=?").bind(s.headPosition).first<string>("level_id");
  await db.prepare("UPDATE organization_positions SET branch_id=?,department_id=?,version=version+1 WHERE id=?").bind(s.f.branch,s.f.department,s.headPosition).run();
  const level=await publishRule(s,ruleInput(s,{kind:"level",id:levelId!}));
  const department=await publishRule(s,ruleInput(s,{kind:"department",id:s.f.department},"group","gpo2"));
  const direct=await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition},"group2","gpo2"));
  const overview=await gpoRulesOverview(db,s.itActor);
  expect(overview.effectiveByPosition[s.headPosition].map(rule=>rule.id).sort()).toEqual([level.id,department.id,direct.id].sort());
  expect(overview.effectiveByPosition[s.headPosition].filter(rule=>rule.inherited)).toHaveLength(2);
  await drainPositionGpo(db,100);
  expect(s.members("group")).toContain("inherited-account");expect(s.members("group2")).toContain("inherited-account");
  expect(await db.prepare("SELECT count(*)::int count FROM organization_gpo_membership_claims WHERE company_id=? AND member_id='inherited-account' AND group_id='group'").bind(s.f.company).first("count")).toBe(2);
  const removal={id:department.id,expectedVersion:1,reason:"Remove only Department source"};
  const preview=await previewRemoveGpoRule(db,s.itActor,removal);
  expect(preview.changes.remove).toEqual([]);
  await removeGpoRule(db,s.itActor,{...removal,previewToken:preview.token},key());
  await drainPositionGpo(db,100);
  expect(s.members("group")).toContain("inherited-account");
  expect(await db.prepare("SELECT count(*)::int count FROM organization_gpo_membership_claims WHERE company_id=? AND member_id='inherited-account' AND group_id='group'").bind(s.f.company).first("count")).toBe(1);
  expect(s.links("ou")).toEqual(expect.arrayContaining(["gpo","gpo2"]));
});

it("uses the Position default Level, covers future positions and follows a Position move",async()=>{
  const s=await setup();await s.link(s.headPerson,"future-inherited");
  const levelId=await db.prepare("SELECT level_id FROM organization_positions WHERE id=?").bind(s.headPosition).first<string>("level_id");
  const rule=await publishRule(s,ruleInput(s,{kind:"level",id:levelId!}));
  // Actual staff rank differs deliberately: inheritance follows the Position.
  await db.prepare("UPDATE hr_engagements SET level_id=?,version=version+1 WHERE person_id=?").bind(`org-level-${s.f.company}-6`,s.headPerson).run();
  await drainPositionGpo(db,100);expect(s.members("group")).toContain("future-inherited");
  const future=`future-position-${key()}`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,title,level_id,reports_to_position_id,created_by,created_at,updated_at) VALUES(?,?,'Future inherited position',?,?,?,1,1)").bind(future,s.f.company,levelId,s.ownerPosition,s.f.owner.userId).run();
  expect((await effectiveGpoRules(db,s.f.company)).some(row=>row.position_id===future&&row.id===rule.id)).toBe(true);
  await db.prepare("UPDATE organization_positions SET level_id=?,version=version+1 WHERE id=?").bind(`org-level-${s.f.company}-4`,s.headPosition).run();
  await drainPositionGpo(db,100);await drainPositionGpo(db,100);
  expect(s.members("group")).not.toContain("future-inherited");
  expect((await gpoRulesOverview(db,s.itActor)).effectiveByPosition[future][0].id).toBe(rule.id);
});

it("rejects expired and stale rule previews and preserves exact command replay",async()=>{
  const s=await setup(),input=ruleInput(s,{kind:"position",id:s.headPosition});
  const expired=await previewGpoRule(db,s.itActor,input);
  await db.prepare("UPDATE organization_gpo_previews SET expires_at=1 WHERE token=?").bind(expired.token).run();
  await expect(publishGpoRule(db,s.itActor,{...input,previewToken:expired.token},key())).rejects.toMatchObject({code:"GPO_PREVIEW_EXPIRED"});
  const stale=await previewGpoRule(db,s.itActor,input);
  await db.prepare("UPDATE organization_positions SET version=version+1 WHERE id=?").bind(s.headPosition).run();
  await expect(publishGpoRule(db,s.itActor,{...input,previewToken:stale.token},key())).rejects.toMatchObject({code:"GPO_PREVIEW_CHANGED"});
  const preview=await previewGpoRule(db,s.itActor,input),commandKey=key(),payload={...input,previewToken:preview.token};
  const result=await publishGpoRule(db,s.itActor,payload,commandKey);
  expect(await publishGpoRule(db,s.itActor,payload,commandKey)).toEqual(result);
  expect(await db.prepare("SELECT count(*)::int count FROM organization_gpo_rules WHERE company_id=?").bind(s.f.company).first("count")).toBe(1);
  await expect(publishGpoRule(db,s.itActor,{...payload,reason:"Different replay input"},commandKey)).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
});

it("supersedes changed inherited rules independently when their revisions and groups overlap",async()=>{
  const s=await setup();await s.link(s.headPerson,"overlapping-rule-account");
  const levelId=await db.prepare("SELECT level_id FROM organization_positions WHERE id=?").bind(s.headPosition).first<string>("level_id");
  const inherited=await publishRule(s,ruleInput(s,{kind:"level",id:levelId!}));
  const direct=await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition},"group","gpo2"));
  await reconcilePositionGpo(db);
  const changed={...ruleInput(s,{kind:"level",id:levelId!},"group2"),id:inherited.id,expectedVersion:1};
  await publishRule(s,changed);
  await drainPositionGpo(db,100);
  expect(s.members("group")).toContain("overlapping-rule-account");
  expect(s.members("group2")).toContain("overlapping-rule-account");
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND rule_id=? AND account_id='overlapping-rule-account' AND revision=1 AND desired_action='add'").bind(s.f.company,inherited.id).first("status")).toBe("superseded");
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND rule_id=? AND account_id='overlapping-rule-account' AND revision=1 AND desired_action='add'").bind(s.f.company,direct.id).first("status")).toBe("directory_applied");
});

it("allows HR to inspect effective rules without publishing and keeps Company boundaries",async()=>{
  const s=await setup(),input=ruleInput(s,{kind:"position",id:s.headPosition});
  await publishRule(s,input);
  const hr=await gpoRulesOverview(db,s.f.owner);
  expect(hr.canManage).toBe(false);expect(hr.effectiveByPosition[s.headPosition]).toHaveLength(1);
  await expect(previewGpoRule(db,s.f.owner,input)).rejects.toMatchObject({code:"IT_AUTHORITY_REQUIRED"});
  const other=await setup();
  await expect(previewGpoRule(db,other.itActor,{...input,connectionId:other.connection})).rejects.toMatchObject({code:"GPO_TARGET_INVALID"});
  expect((await gpoRulesOverview(db,other.itActor)).rules).toEqual([]);
});

it("keeps legacy Position endpoints synchronized with direct rule edits and removal",async()=>{
  const s=await setup();await publishPositionGpo(db,s.itActor,s.input(s.headPosition),key());
  const current=(await gpoRulesOverview(db,s.itActor)).rules[0];
  expect(current.legacyPositionId).toBe(s.headPosition);
  const input={...ruleInput(s,{kind:"position",id:s.headPosition},"group2"),id:current.id,expectedVersion:current.version};
  await publishRule(s,input);
  expect(await db.prepare("SELECT user_filter_group_id FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(s.f.company,s.headPosition).first("user_filter_group_id")).toBe("group2");
  const removal={id:current.id,expectedVersion:2,reason:"Remove direct compatibility rule"};
  const preview=await previewRemoveGpoRule(db,s.itActor,removal);
  await removeGpoRule(db,s.itActor,{...removal,previewToken:preview.token},key());
  expect(await db.prepare("SELECT connection_id FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(s.f.company,s.headPosition).first("connection_id")).toBeNull();
  await publishPositionGpo(db,s.itActor,s.input(s.headPosition,3),key());
  expect((await gpoRulesOverview(db,s.itActor)).rules[0]).toMatchObject({id:current.id,version:4,active:true});
});

it("refreshes delivery evidence when an employee transfers between positions sharing a Level rule",async()=>{
  const s=await setup();await s.link(s.headPerson,"same-level-transfer");
  const levelId=await db.prepare("SELECT level_id FROM organization_positions WHERE id=?").bind(s.headPosition).first<string>("level_id");
  await publishRule(s,ruleInput(s,{kind:"level",id:levelId!}));
  await drainPositionGpo(db,100);
  const next=`same-level-${key()}`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,title,level_id,reports_to_position_id,created_by,created_at,updated_at) VALUES(?,?,'Second position',?,?,?,1,1)").bind(next,s.f.company,levelId,s.ownerPosition,s.f.owner.userId).run();
  await db.prepare("UPDATE hr_engagements SET position_id=?,version=version+1 WHERE person_id=?").bind(next,s.headPerson).run();
  const writes=provider.apply.mock.calls.length;
  await drainPositionGpo(db,100);
  expect(s.members("group")).toContain("same-level-transfer");
  expect(provider.apply.mock.calls.length).toBe(writes);
  expect(await db.prepare("SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND account_id='same-level-transfer' AND desired_action='add'").bind(s.f.company,next).first("status")).toBe("directory_applied");
  expect(await db.prepare("SELECT position_id FROM organization_gpo_membership_claims WHERE company_id=? AND member_id='same-level-transfer'").bind(s.f.company).first("position_id")).toBe(next);
});

it("serializes concurrent drains when the final two policy sources are removed",async()=>{
  const s=await setup();await s.link(s.headPerson,"concurrent-final-sources");
  const levelId=await db.prepare("SELECT level_id FROM organization_positions WHERE id=?").bind(s.headPosition).first<string>("level_id");
  const rules=[await publishRule(s,ruleInput(s,{kind:"level",id:levelId!})),await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition},"group","gpo2"))];
  await drainPositionGpo(db,100);
  for(const rule of rules){const input={id:rule.id,expectedVersion:1,reason:"Remove final policy source"};const preview=await previewRemoveGpoRule(db,s.itActor,input);await removeGpoRule(db,s.itActor,{...input,previewToken:preview.token},key());}
  await Promise.all([drainPositionGpo(db,100),drainPositionGpo(db,100)]);
  expect(s.members("group")).not.toContain("concurrent-final-sources");
  expect(await db.prepare("SELECT count(*)::int count FROM organization_gpo_membership_claims WHERE company_id=? AND member_id='concurrent-final-sources'").bind(s.f.company).first("count")).toBe(0);
});

it("drains with a single-connection pool while committing source ownership before read-back",async()=>{
  const s=await setup();await s.link(s.headPerson,"single-pool-account");
  await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition}));
  const {PostgresDatabase}=await import("../src/server/runtime/postgres");
  const serial=new PostgresDatabase({...db.pool.options,max:1,connectionTimeoutMillis:500});
  try{
    failAfterMember.add(s.connection);
    await drainPositionGpo(serial,100);
    expect(await serial.prepare("SELECT count(*)::int count FROM organization_gpo_membership_claims WHERE company_id=? AND member_id='single-pool-account'").bind(s.f.company).first("count")).toBe(1);
    const job=await serial.prepare("SELECT id,version FROM position_gpo_jobs WHERE company_id=? AND account_id='single-pool-account' AND status='blocked'").bind(s.f.company).first<{id:string;version:number}>();
    expect(job).toBeTruthy();
    await retryPositionGpo(serial,s.itActor,{id:job!.id,expectedVersion:job!.version,reason:"Read-back recovered"},key());
    await drainPositionGpo(serial,100);
    expect(await serial.prepare("SELECT status FROM position_gpo_jobs WHERE id=?").bind(job!.id).first("status")).toBe("directory_applied");
  }finally{await serial.close();}
});

it("defers to the Directory connection lock and retains a source committed by its owner",async()=>{
  const s=await setup();await s.link(s.headPerson,"directory-lock-account");
  const rule=await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition}));
  const directory=await db.pool.connect();
  try{
    await directory.query("SELECT pg_advisory_lock(hashtext($1))",["people-directory:"+s.connection]);
    const writes=provider.apply.mock.calls.length;
    await drainPositionGpo(db,100);
    expect(provider.apply.mock.calls.length).toBe(writes);
    expect(await db.prepare("SELECT directory_config_status FROM organization_gpo_rules WHERE id=?").bind(rule.id).first("directory_config_status")).toBe("pending");
    await directory.query("SELECT pg_advisory_unlock(hashtext($1))",["people-directory:"+s.connection]);
    await drainPositionGpo(db,100);
    expect(s.members("group")).toContain("directory-lock-account");
    const removal={id:rule.id,expectedVersion:1,reason:"Remove only organization ownership"};
    const preview=await previewRemoveGpoRule(db,s.itActor,removal);
    await removeGpoRule(db,s.itActor,{...removal,previewToken:preview.token},key());
    await directory.query("SELECT pg_advisory_lock(hashtext($1))",["people-directory:"+s.connection]);
    await drainPositionGpo(db,100);
    expect(await db.prepare("SELECT count(*)::int count FROM organization_gpo_membership_claims WHERE company_id=?").bind(s.f.company).first("count")).toBe(1);
    await directory.query("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES($1,'group','directory-lock-account','direct:reviewed-operation',$2)",[s.connection,Date.now()]);
    await directory.query("SELECT pg_advisory_unlock(hashtext($1))",["people-directory:"+s.connection]);
    await drainPositionGpo(db,100);
    expect(s.members("group")).toContain("directory-lock-account");
    expect(await db.prepare("SELECT count(*)::int count FROM organization_gpo_membership_claims WHERE company_id=?").bind(s.f.company).first("count")).toBe(0);
  }finally{
    await directory.query("SELECT pg_advisory_unlock_all()");
    directory.release();
  }
});
