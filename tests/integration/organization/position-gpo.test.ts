// Organization Position GPO rule oracles. Delivery, membership ownership and
// Directory apply are native (packages/server-identity_access Rust +
// rust-position-gpo-postgres); this suite keeps the retained Organization rule
// producer coverage and seeds the legacy policy row the native publisher writes.
import { beforeAll,expect,it,vi } from "vitest";
import { env,applyMigrations } from "../../support/runtime-env";
import { expenseFixture,expenseDb as db,key } from "../../support/helpers/expense-fixture";
import type { DirectoryInventory } from "@erpx/feature-identity_access/lib/directory-operations";
import { effectiveGpoRules, gpoRulesOverview, previewGpoRule, publishGpoRule, previewRemoveGpoRule, removeGpoRule, markPositionGpoNotRequired } from "@erpx/server-identity_access/public/gpo-rules";
import type { GpoRuleInput } from "@erpx/feature-organization/lib/gpo-rules";

const provider=vi.hoisted(()=>({read:vi.fn(),apply:vi.fn()}));
const mfa=vi.hoisted(()=>({check:vi.fn()}));
vi.mock("@erpx/server-identity_access/public/directory-provider",async importOriginal=>({...(await importOriginal<typeof import("@erpx/server-identity_access/public/directory-provider")>()),readAutomationInventory:provider.read,applyDirectoryAction:provider.apply}));
vi.mock("@erpx/server-identity_access/public/iam-mfa",async importOriginal=>({...(await importOriginal<typeof import("@erpx/server-identity_access/public/iam-mfa")>()),requireFreshMfa:mfa.check}));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});

async function publishRule(s:Awaited<ReturnType<typeof setup>>,input:GpoRuleInput){
  const preview=await previewGpoRule(db,s.itActor,input);
  return publishGpoRule(db,s.itActor,{...input,previewToken:preview.token},key()) as Promise<{id:string;version:number;revision:number}>;
}
function ruleInput(s:Awaited<ReturnType<typeof setup>>,target:GpoRuleInput["target"],group="group",gpo="gpo"):GpoRuleInput {
  return {target,expectedVersion:0,connectionId:s.connection,gpoId:gpo,userFilterGroupId:group,computerTargetOuId:"ou",reason:"Assign inherited organization policy"};
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
  provider.read.mockResolvedValue(structuredClone(inventory));
  provider.apply.mockResolvedValue({verified:true});
  return {f,itActor,connection,inventory,headPerson,ownerPerson,headPosition,ownerPosition};
}

it("creates a pending Position task and supports multiple mappings",async()=>{
  const s=await setup();
  const before=(await gpoRulesOverview(db,s.itActor)).tasks.find(task=>task.positionId===s.headPosition);
  expect(before).toMatchObject({status:"pending",mappingCount:0});
  const first=await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition}));
  const second=await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition},"group2","gpo2"));
  expect((await gpoRulesOverview(db,s.itActor)).tasks.find(task=>task.positionId===s.headPosition)).toMatchObject({status:"pending",mappingCount:2});
  const overview=await gpoRulesOverview(db,s.itActor);
  expect(overview.effectiveByPosition[s.headPosition].map(rule=>rule.id).sort()).toEqual([first.id,second.id].sort());
});

it("rejects Level and Department writes and never inherits a policy after a Position move",async()=>{
  const s=await setup();
  const levelId=await db.prepare("SELECT level_id FROM organization_positions WHERE id=?").bind(s.headPosition).first<string>("level_id");
  await expect(previewGpoRule(db,s.itActor,{...ruleInput(s,{kind:"position",id:levelId!}),target:{kind:"level",id:levelId!}})).rejects.toThrow();
  await expect(previewGpoRule(db,s.itActor,{...ruleInput(s,{kind:"position",id:s.f.department}),target:{kind:"department",id:s.f.department}})).rejects.toThrow();
  const rule=await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition}));
  const future=`future-position-${key()}`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,title,level_id,reports_to_position_id,created_by,created_at,updated_at) VALUES(?,?,'Future direct position',?,?,?,1,1)").bind(future,s.f.company,levelId,s.ownerPosition,s.f.owner.userId).run();
  expect((await effectiveGpoRules(db,s.f.company)).some(row=>row.position_id===future&&row.id===rule.id)).toBe(false);
  expect((await gpoRulesOverview(db,s.itActor)).tasks.find(task=>task.positionId===future)).toMatchObject({status:"pending"});
});

it("closes a task with a reason and version, then reopens on a Position change",async()=>{
  const s=await setup();
  const task=(await gpoRulesOverview(db,s.itActor)).tasks.find(item=>item.positionId===s.headPosition)!;
  await expect(markPositionGpoNotRequired(db,s.itActor,{positionId:s.headPosition,expectedVersion:task.version,reason:" "},key())).rejects.toThrow();
  await markPositionGpoNotRequired(db,s.itActor,{positionId:s.headPosition,expectedVersion:task.version,reason:"No Windows policy needed"},key());
  expect((await gpoRulesOverview(db,s.itActor)).tasks.find(item=>item.positionId===s.headPosition)).toMatchObject({status:"not_required",reason:"No Windows policy needed"});
  await expect(markPositionGpoNotRequired(db,s.itActor,{positionId:s.headPosition,expectedVersion:task.version,reason:"Still no policy"},key())).rejects.toMatchObject({code:"GPO_TASK_CHANGED"});
  await db.prepare("UPDATE organization_positions SET title=title||' revised',version=version+1 WHERE id=?").bind(s.headPosition).run();
  expect((await gpoRulesOverview(db,s.itActor)).tasks.find(item=>item.positionId===s.headPosition)?.status).toBe("pending");
  const pending=(await gpoRulesOverview(db,s.itActor)).tasks.find(item=>item.positionId===s.headPosition)!;
  await markPositionGpoNotRequired(db,s.itActor,{positionId:s.headPosition,expectedVersion:pending.version,reason:"Still no policy"},key());
  await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition}));
  const reopened=(await gpoRulesOverview(db,s.itActor)).tasks.find(item=>item.positionId===s.headPosition)!;
  expect(reopened.status).toBe("pending");
  await expect(markPositionGpoNotRequired(db,s.itActor,{positionId:s.headPosition,expectedVersion:reopened.version,reason:"Still no policy"},key())).rejects.toMatchObject({code:"GPO_TASK_CHANGED"});
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

it("keeps legacy Position policy rows synchronized with direct rule edits and removal",async()=>{
  const s=await setup(),now=Date.now();
  // The native publisher writes the immutable revision and the policy row; the
  // migration trigger mirrors the policy into the legacy Organization rule.
  await db.prepare(`INSERT INTO position_gpo_policies(company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,updated_at)
    VALUES(?,?,1,1,?,'gpo','group','ou','pending',?)`).bind(s.f.company,s.headPosition,s.connection,now).run();
  const current=(await gpoRulesOverview(db,s.itActor)).rules[0];
  expect(current.legacyPositionId).toBe(s.headPosition);
  const input={...ruleInput(s,{kind:"position",id:s.headPosition},"group2"),id:current.id,expectedVersion:current.version};
  await publishRule(s,input);
  expect(await db.prepare("SELECT user_filter_group_id FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(s.f.company,s.headPosition).first("user_filter_group_id")).toBe("group2");
  const removal={id:current.id,expectedVersion:2,reason:"Remove direct compatibility rule"};
  const preview=await previewRemoveGpoRule(db,s.itActor,removal);
  await removeGpoRule(db,s.itActor,{...removal,previewToken:preview.token},key());
  expect(await db.prepare("SELECT connection_id FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(s.f.company,s.headPosition).first("connection_id")).toBeNull();
});

it("reopens a completed task after a material Position change",async()=>{
  const s=await setup(),rule=await publishRule(s,ruleInput(s,{kind:"position",id:s.headPosition}));
  await db.prepare("UPDATE organization_gpo_rules SET directory_config_status='directory_applied',version=version+1 WHERE id=?").bind(rule.id).run();
  expect((await gpoRulesOverview(db,s.itActor)).tasks.find(task=>task.positionId===s.headPosition)?.status).toBe("completed");
  await db.prepare("UPDATE organization_positions SET title=title||' changed',version=version+1 WHERE id=?").bind(s.headPosition).run();
  expect((await gpoRulesOverview(db,s.itActor)).tasks.find(task=>task.positionId===s.headPosition)?.status).toBe("pending");
});
