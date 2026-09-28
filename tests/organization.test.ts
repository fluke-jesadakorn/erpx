import { organizationChangeWork, organizationChangeNotificationTarget } from "../src/server/organization-work";
import { listStaffNotifications } from "../src/server/staff-notifications";
import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { env,applyMigrations } from "./runtime-env";
import { expenseFixture,expenseDb as db,key } from "./helpers/expense-fixture";
import { organizationOverview,installStaffLevels,savePosition,positionSchema,saveOrganizationTemplate,applyOrganizationTemplate,organizationGuard,organizationNodeChangeSchema,saveOrganizationNodeChange,decideOrganizationNodeChange,cancelOrganizationNodeChange } from "../src/server/organization";
import { directoryUpdateSchema,saveDirectoryPerson,setDirectoryStatus } from "../src/server/people-directory";
import { grantPersonAccess,revokePersonAccess } from "../src/server/account-access";
import { companyBusinessAcceptance, companyBusinessAcceptanceWork, submitCompanyBusinessAcceptance, decideCompanyBusinessAcceptance, revokeCompanyBusinessAcceptance } from "../src/server/company-business-acceptance";
import { executeCommand } from "../src/server/commands";
import type { WorkContext } from "../src/server/authorization";
import { attachmentBindingState, enableAttachmentV2ForTest, seedAttachmentSet } from "./helpers/attachment-v2";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:db,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,ceo:string,staffPosition:string,hr:WorkContext;
const level=(n:number)=>`org-level-${f.company}-${n}`;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);enableAttachmentV2ForTest();});
beforeEach(async()=>{
  f=await expenseFixture(false);hr=await f.member("hr","hr_admin");
  await installStaffLevels(db,f.owner,key());
  ceo=String((await savePosition(db,f.owner,null,positionSchema.parse({title:"CEO",branchId:null,departmentId:null,levelId:level(1),reportsToPositionId:null,isCeo:true,expectedVersion:0,reason:"Set up leadership"}),key())).id);
  staffPosition=String((await savePosition(db,hr,null,positionSchema.parse({title:"Operations staff",branchId:f.branch,departmentId:f.department,levelId:level(6),reportsToPositionId:ceo,expectedVersion:0,reason:"Set up staff position"}),key())).id);
  await db.prepare("UPDATE hr_engagements SET position_id=?,level_id=? WHERE person_id=?").bind(ceo,level(1),`person-${f.owner.userId}`).run();
});
function draft(extra:Record<string,unknown>={}) {return directoryUpdateSchema.parse({legalName:"New staff",email:`${key()}@example.test`,countryCode:"TH",branchId:f.branch,departmentId:f.department,jobTitle:"Operations staff",workerType:"employee",employmentType:"salaried",startDate:"2026-01-01",currency:"THB",expectedVersion:1,...extra});}
it("keeps legacy ranks unmapped and installs one explicit six-level catalog",async()=>{
  await installStaffLevels(db,hr,key());
  const data=await organizationOverview(db,hr);
  expect(data.levels.map(l=>l.level_number)).toEqual([1,2,3,4,5,6]);
  expect(data.people.find(p=>p.user_id===f.claimant.userId)?.issues).toContain("Assign one of the six staff levels.");
  expect(await db.prepare("SELECT rank,level_number FROM staff_levels WHERE id=?").bind(`${f.company}-level-10`).first()).toEqual({rank:10,level_number:null});
});
it("preserves incomplete onboarding and requires manager and numbered level for activation",async()=>{
  const input=draft();const result=await saveDirectoryPerson(hr,null,input,key());
  await expect(setDirectoryStatus(hr,result.id,{status:"active",expectedVersion:1},key())).rejects.toMatchObject({code:"STAFF_ORGANIZATION_INCOMPLETE"});
  await saveDirectoryPerson(hr,result.id,{...input,levelId:level(6),positionId:staffPosition,managerPersonId:`person-${f.owner.userId}`,reason:"Complete staff reporting"},key());
  await setDirectoryStatus(hr,result.id,{status:"active",expectedVersion:2},key());
  const person=(await organizationOverview(db,hr)).people.find(p=>p.person_id===result.id)!;
  expect(person.status).toBe("active");expect(person.level_number).toBe(6);expect(person.issues).toEqual([]);
  expect((await db.prepare("SELECT id FROM staff_organization_history WHERE engagement_id=?").bind(person.engagement_id).all()).results).toHaveLength(2);
});
it("allows the explicit CEO exception and rejects self or junior reporting managers",async()=>{
  const input=draft({positionId:ceo,levelId:level(1)}),result=await saveDirectoryPerson(hr,null,input,key());
  await setDirectoryStatus(hr,result.id,{status:"active",expectedVersion:1},key());
  await expect(saveDirectoryPerson(hr,result.id,{...input,expectedVersion:2,positionId:staffPosition,levelId:level(6),managerPersonId:result.id},key())).rejects.toMatchObject({code:"STAFF_ORGANIZATION_INCOMPLETE"});
  await expect(saveDirectoryPerson(hr,result.id,{...input,expectedVersion:2,positionId:staffPosition,levelId:level(1),managerPersonId:`person-${f.owner.userId}`},key())).rejects.toMatchObject({code:"STAFF_ORGANIZATION_INCOMPLETE"});
});
it("checks position scope and optimistic versions without allowing reporting cycles",async()=>{
  await expect(savePosition(db,hr,ceo,positionSchema.parse({title:"CEO",branchId:null,departmentId:null,levelId:level(1),reportsToPositionId:staffPosition,isCeo:false,expectedVersion:1,reason:"Bad cycle"}),key())).rejects.toMatchObject({status:422});
  const input=positionSchema.parse({title:"Operations specialist",branchId:f.branch,departmentId:f.department,levelId:level(6),reportsToPositionId:ceo,expectedVersion:1,reason:"Rename position"});
  const command=key();await savePosition(db,hr,staffPosition,input,command);await savePosition(db,hr,staffPosition,input,command);
  await expect(savePosition(db,hr,staffPosition,input,key())).rejects.toMatchObject({status:409});
  const outsider={...hr,branchId:"other",assignmentScope:"branch" as const};
  await expect(savePosition(db,outsider,staffPosition,input,key())).rejects.toMatchObject({status:403});
});
it("defaults new Positions to backoffice and preserves roster requirement on omitted edits",async()=>{
  expect((await db.prepare("SELECT roster_required FROM organization_positions WHERE id=?").bind(staffPosition).first<{roster_required:number}>())?.roster_required).toBe(0);
  const enabled=positionSchema.parse({title:"Operations staff",branchId:f.branch,departmentId:f.department,levelId:level(6),reportsToPositionId:ceo,rosterRequired:true,expectedVersion:1,reason:"Staff work shifts"});
  await savePosition(db,hr,staffPosition,enabled,key());
  const renamed=positionSchema.parse({title:"Operations specialist",branchId:f.branch,departmentId:f.department,levelId:level(6),reportsToPositionId:ceo,expectedVersion:2,reason:"Clarify title"});
  await savePosition(db,hr,staffPosition,renamed,key());
  expect((await db.prepare("SELECT roster_required FROM organization_positions WHERE id=?").bind(staffPosition).first<{roster_required:number}>())?.roster_required).toBe(1);
});
it("reviews a reusable template, reuses the CEO, adds vacancies and never grants roles",async()=>{
  const template=await saveOrganizationTemplate(db,hr,null,{name:"Small team",expectedVersion:0,positions:[{key:"ceo",title:"CEO",levelNumber:1,reportsToKey:null,isCeo:true},{key:"staff",title:"Team staff",levelNumber:6,reportsToKey:"ceo",isCeo:false}]},key());
  const before=await db.prepare("SELECT count(*) n FROM assignments WHERE company_id=?").bind(f.company).first();
  const input={expectedVersion:1,branchId:f.branch,departmentId:f.department,reason:"Reviewed branch expansion"};const command=key();
  await applyOrganizationTemplate(db,hr,String(template.id),input,command);await applyOrganizationTemplate(db,hr,String(template.id),input,command);
  const data=await organizationOverview(db,hr);expect(data.positions.filter(p=>p.is_ceo)).toHaveLength(1);expect(data.positions.filter(p=>p.title==="Team staff")).toHaveLength(1);
  expect(await db.prepare("SELECT count(*) n FROM assignments WHERE company_id=?").bind(f.company).first()).toEqual(before);
});
it("routes HR grants through fresh MFA and review while preventing self changes and broader scope",async()=>{
  await expect(grantPersonAccess(hr,`person-${f.claimant.userId}`,{role:"treasury",scope:"department",expiresAt:"2027-01-01"},key())).rejects.toMatchObject({code:"FRESH_MFA_REQUIRED"});
  await expect(grantPersonAccess(hr,`person-${hr.userId}`,{role:"finance_approver",scope:"holding",expiresAt:"2027-01-01"},key())).rejects.toMatchObject({code:"SELF_ACCESS_CHANGE"});
  const scoped={...hr,assignmentScope:"department" as const,branchId:f.branch,departmentId:f.department};
  await expect(grantPersonAccess(scoped,`person-${f.claimant.userId}`,{role:"treasury",scope:"holding",expiresAt:"2027-01-01"},key())).rejects.toMatchObject({code:"GRANT_SCOPE_FORBIDDEN"});
  await expect(revokePersonAccess(hr,`person-${f.owner.userId}`,f.owner.assignmentId,key())).rejects.toMatchObject({code:"FRESH_MFA_REQUIRED"});
});
it("rejects a stale organization snapshot in the committing transaction",async()=>{
  const command=key(),guard=await organizationGuard(db,f.company,command);
  await db.prepare("UPDATE hr_engagements SET manager_person_id=? WHERE person_id=?").bind(`person-${f.owner.userId}`,`person-${f.claimant.userId}`).run();
  await expect(executeCommand(db,hr,command,"organization.test",{},f.company,1,{id:f.company},[guard])).rejects.toMatchObject({status:409});
});
it("applies independently reviewed branch and department changes with safe suspension and immutable history",async()=>{
  const reviewer=await f.member("organization-reviewer","hr_admin");
  const branchInput=organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`north-${key().slice(0,8)}`,displayName:"Northern branch",branchId:null,targetNodeId:null,expectedNodeVersion:null,expectedVersion:0,reason:"Open the approved northern operation"});
  const proposal=await saveOrganizationNodeChange(db,f.owner,null,branchInput,key(),true);
  await expect(decideOrganizationNodeChange(db,f.owner,proposal.id,{expectedVersion:1,decision:"approve",reason:"Self review"},key())).rejects.toMatchObject({code:"INDEPENDENT_REVIEW_REQUIRED"});
  await expect(decideOrganizationNodeChange(db,{...reviewer,assignmentScope:"branch",branchId:f.branch},proposal.id,{expectedVersion:1,decision:"approve",reason:"Scoped reviewer"},key())).rejects.toMatchObject({code:"COMPANY_AUTHORITY_REQUIRED"});
  const linkedIdentity={...reviewer,iam:{roleRevision:1,identityVersion:1,permissions:["organization.manage"],grantablePermissions:[],principalId:"linked-person",personId:null,personUserIds:[f.owner.userId,reviewer.userId],freshUntil:null}};
  expect((await organizationOverview(db,linkedIdentity)).nodeChanges.find(c=>c.id===proposal.id)?.canReview).toBe(false);
  expect((await organizationOverview(db,reviewer)).nodeChanges.find(c=>c.id===proposal.id)?.canReview).toBe(true);
  await expect(decideOrganizationNodeChange(db,linkedIdentity,proposal.id,{expectedVersion:1,decision:"approve",reason:"Same person with linked account"},key())).rejects.toMatchObject({code:"INDEPENDENT_REVIEW_REQUIRED"});
  const approved=await decideOrganizationNodeChange(db,reviewer,proposal.id,{expectedVersion:1,decision:"approve",reason:"Capacity and authority reviewed"},key());
  const branch=await db.prepare("SELECT * FROM organization_nodes WHERE id=?").bind(approved.nodeId).first<{id:string;display_name:string;status:string;hierarchy_revision:number}>();
  expect(branch).toMatchObject({display_name:"Northern branch",status:"active",hierarchy_revision:1});
  expect(await db.prepare("SELECT name FROM messaging_rooms WHERE organization_node_id=?").bind(branch!.id).first()).toEqual({name:"Northern branch"});

  const department=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"create",kind:"department",slug:"service",displayName:"Service",branchId:branch!.id,targetNodeId:null,expectedNodeVersion:null,expectedVersion:0,reason:"Create the reviewed service team"}),key(),true);
  await decideOrganizationNodeChange(db,reviewer,department.id,{expectedVersion:1,decision:"approve",reason:"Team placement reviewed"},key());
  const movable=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"create",kind:"department",slug:`movable-${key().slice(0,8)}`,displayName:"Movable team",branchId:f.branch,targetNodeId:null,expectedNodeVersion:null,expectedVersion:0,reason:"Create a team for relocation"}),key(),true);
  const movableApplied=await decideOrganizationNodeChange(db,reviewer,movable.id,{expectedVersion:1,decision:"approve",reason:"Relocation target reviewed"},key());
  await db.batch([
    db.prepare("UPDATE hr_engagements SET branch_id=?,department_id=?,position_id=? WHERE person_id=? AND company_id=?").bind(f.branch,movableApplied.nodeId,staffPosition,`person-${f.claimant.userId}`,f.company),
    db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,department_id,display_name,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id").bind(`employee-${f.claimant.userId}`,f.claimant.userId,f.company,f.branch,movableApplied.nodeId,"Claimant",Date.now()),
    db.prepare("UPDATE organization_positions SET branch_id=?,department_id=? WHERE id=? AND company_id=?").bind(f.branch,movableApplied.nodeId,staffPosition,f.company),
    db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source) VALUES(?,?,?,'[]',?,?,0,9999999999999,'department',?,NULL,'manual')").bind(`department-grant-${key()}`,f.claimant.userId,"hr_admin",f.company,f.branch,movableApplied.nodeId),
  ]);
  const relocation=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"relocate",kind:"department",slug:null,displayName:"Movable team",branchId:branch!.id,targetNodeId:movableApplied.nodeId,expectedNodeVersion:1,expectedVersion:0,reason:"Move the team to its operating branch"}),key(),true);
  const relocationApplied=await decideOrganizationNodeChange(db,reviewer,relocation.id,{expectedVersion:1,decision:"approve",reason:"Staff, positions and scoped access moved together"},key());
  const relocatedNodeId=String(relocationApplied.nodeId);
  expect(relocatedNodeId).not.toBe(movableApplied.nodeId);
  expect(await db.prepare("SELECT branch_id,parent_id,status FROM organization_nodes WHERE id=?").bind(movableApplied.nodeId).first()).toEqual({branch_id:f.branch,parent_id:f.branch,status:"archived"});
  expect(await db.prepare("SELECT branch_id,parent_id,status FROM organization_nodes WHERE id=?").bind(relocatedNodeId).first()).toEqual({branch_id:branch!.id,parent_id:branch!.id,status:"active"});
  expect(await db.prepare("SELECT branch_id,department_id FROM hr_engagements WHERE person_id=?").bind(`person-${f.claimant.userId}`).first()).toEqual({branch_id:branch!.id,department_id:relocatedNodeId});
  expect(await db.prepare("SELECT branch_id,department_id FROM employees WHERE user_id=?").bind(f.claimant.userId).first()).toEqual({branch_id:branch!.id,department_id:relocatedNodeId});
  expect(await db.prepare("SELECT branch_id,department_id FROM organization_positions WHERE id=?").bind(staffPosition).first()).toEqual({branch_id:branch!.id,department_id:relocatedNodeId});
  expect(await db.prepare("SELECT branch_id FROM assignments WHERE scope_kind='department' AND scope_node_id=?").bind(relocatedNodeId).first()).toEqual({branch_id:branch!.id});
  const empty=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"create",kind:"department",slug:`archive-${key().slice(0,8)}`,displayName:"Archive candidate",branchId:branch!.id,targetNodeId:null,expectedNodeVersion:null,expectedVersion:0,reason:"Retire an unused team"}),key(),true);
  const emptyApplied=await decideOrganizationNodeChange(db,reviewer,empty.id,{expectedVersion:1,decision:"approve",reason:"No active dependencies remain"},key());
  const archive=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"archive",kind:"department",slug:null,displayName:"Archive candidate",branchId:branch!.id,targetNodeId:emptyApplied.nodeId,expectedNodeVersion:1,expectedVersion:0,reason:"Retire the unused team with history retained"}),key(),true);
  await decideOrganizationNodeChange(db,reviewer,archive.id,{expectedVersion:1,decision:"approve",reason:"Archive evidence reviewed"},key());
  expect(await db.prepare("SELECT status FROM organization_nodes WHERE id=?").bind(emptyApplied.nodeId).first("status")).toBe("archived");
  const restore=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"reactivate",kind:"department",slug:null,displayName:"Archive candidate",branchId:branch!.id,targetNodeId:emptyApplied.nodeId,expectedNodeVersion:2,expectedVersion:0,reason:"Reopen the retained team"}),key(),true);
  await decideOrganizationNodeChange(db,reviewer,restore.id,{expectedVersion:1,decision:"approve",reason:"Reactivation evidence reviewed"},key());
  expect(await db.prepare("SELECT status FROM organization_nodes WHERE id=?").bind(emptyApplied.nodeId).first("status")).toBe("active");
  const suspend=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"suspend",kind:"branch",slug:null,displayName:"Northern branch",branchId:null,targetNodeId:branch!.id,expectedNodeVersion:1,expectedVersion:0,reason:"Close the branch after relocation"}),key(),true);
  await expect(decideOrganizationNodeChange(db,reviewer,suspend.id,{expectedVersion:1,decision:"approve",reason:"Closure evidence reviewed"},key())).rejects.toMatchObject({code:"ORGANIZATION_NODE_IN_USE"});
  expect((await db.prepare("SELECT state FROM organization_node_changes WHERE id=?").bind(suspend.id).first<{state:string}>())?.state).toBe("submitted");
  const events=(await db.prepare("SELECT action FROM organization_node_change_events WHERE change_id=? ORDER BY created_at,id").bind(proposal.id).all<{action:string}>()).results;
  expect(events.map(event=>event.action)).toEqual(["submitted","applied"]);
  const applied=(await organizationOverview(db,reviewer,{changeState:"applied",changeQuery:"Northern branch"})).nodeChanges;
  expect(applied).toHaveLength(1);
  expect(applied[0].events?.map(event=>event.action)).toEqual(["submitted","applied"]);
  expect(applied[0].events?.at(-1)?.reason).toBe("Capacity and authority reviewed");
  await expect(db.prepare("UPDATE organization_node_change_events SET action='rejected' WHERE change_id=?").bind(proposal.id).run()).rejects.toThrow("IMMUTABLE_ORGANIZATION_NODE_CHANGE_EVENT");
});
it("binds organization-change proof in the independent review transaction and leaves stale proof unbound",async()=>{
  const reviewer=await f.member("organization-proof-reviewer","hr_admin");
  const proposal=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`proof-${key().slice(0,8)}`,displayName:"Evidence branch",expectedVersion:0,reason:"Create a branch with reviewed proof"}),key(),true);
  const proof=await seedAttachmentSet(db,reviewer,{targetType:"organization_change",targetId:proposal.id,componentKind:"action",componentRef:"organization-change-review",purposeKey:"org_change_proof",expectedTargetVersion:proposal.version});
  const result=await decideOrganizationNodeChange(db,reviewer,proposal.id,{expectedVersion:proposal.version,decision:"approve",reason:"Registration and permit proof reviewed",attachmentSetIds:[proof.setId]},key());
  expect(result.state).toBe("applied");
  expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"organization_change_review"});

  const staleProposal=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`stale-proof-${key().slice(0,8)}`,displayName:"Stale evidence branch",expectedVersion:0,reason:"Verify stale proof remains editable"}),key(),true);
  const staleProof=await seedAttachmentSet(db,reviewer,{targetType:"organization_change",targetId:staleProposal.id,componentKind:"action",componentRef:"organization-change-review",purposeKey:"org_change_proof",expectedTargetVersion:staleProposal.version});
  await expect(decideOrganizationNodeChange(db,reviewer,staleProposal.id,{expectedVersion:staleProposal.version+1,decision:"approve",reason:"Attempt review against a stale revision",attachmentSetIds:[staleProof.setId]},key())).rejects.toMatchObject({code:"STALE_VERSION"});
  expect(await attachmentBindingState(db,staleProof.setId)).toMatchObject({set_state:"draft",link_state:"draft"});
});
it("supports draft, correction rejection and requester cancellation with optimistic versions",async()=>{
  const reviewer=await f.member("organization-reviewer-2","hr_admin"),input=organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`draft-${key().slice(0,8)}`,displayName:"Draft branch",branchId:null,targetNodeId:null,expectedNodeVersion:null,expectedVersion:0,reason:"Evaluate a possible branch"});
  const draftChange=await saveOrganizationNodeChange(db,f.owner,null,input,key(),false);
  await expect(saveOrganizationNodeChange(db,f.owner,draftChange.id,{...input,displayName:"Stale overwrite",expectedVersion:0},key(),true)).rejects.toMatchObject({status:409});
  expect((await organizationOverview(db,f.owner)).nodeChanges.find(c=>c.id===draftChange.id)).toMatchObject({display_name:"Draft branch",version:1,state:"draft"});
  await expect(decideOrganizationNodeChange(db,reviewer,draftChange.id,{expectedVersion:1,decision:"approve",reason:"Too early"},key())).rejects.toMatchObject({code:"CHANGE_NOT_SUBMITTED"});
  const submitted=await saveOrganizationNodeChange(db,f.owner,draftChange.id,{...input,expectedVersion:1},key(),true);
  await decideOrganizationNodeChange(db,reviewer,submitted.id,{expectedVersion:2,decision:"reject",reason:"Add regional operating evidence"},key());
  await expect(cancelOrganizationNodeChange(db,f.owner,submitted.id,2,key())).rejects.toMatchObject({code:"CONFLICT"});
  await cancelOrganizationNodeChange(db,f.owner,submitted.id,3,key());
  expect((await db.prepare("SELECT state,version FROM organization_node_changes WHERE id=?").bind(submitted.id).first())).toEqual({state:"cancelled",version:4});
  const history=(await organizationOverview(db,f.owner,{changeState:"cancelled"})).nodeChanges.find(c=>c.id===submitted.id);
  expect(history?.events?.map(event=>event.action)).toEqual(["drafted","submitted","rejected","cancelled"]);
  expect(history?.events?.[2].reason).toBe("Add regional operating evidence");
});

it("paginates and searches organization requests without exposing company review evidence to scoped staff",async()=>{
  for(let i=0;i<21;i++)await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({
    action:"create",kind:"branch",slug:`review-${i}-${key().slice(0,8)}`,displayName:`Review branch ${String(i).padStart(2,"0")}`,
    expectedVersion:0,reason:i===0?"Capacity 50% complete_approved":"Capacity review awaiting evidence",
  }),key(),false);
  const first=await organizationOverview(db,hr,{changeState:"draft",changeQuery:"Review branch"});
  const second=await organizationOverview(db,hr,{changeState:"draft",changeQuery:"Review branch",changePage:"2"});
  expect(first.changeQueue).toMatchObject({total:21,page:1,pages:2,pageSize:20});
  expect(first.nodeChanges).toHaveLength(20);expect(second.nodeChanges).toHaveLength(1);
  expect(new Set([...first.nodeChanges,...second.nodeChanges].map(c=>c.id)).size).toBe(21);
  const literal=await organizationOverview(db,hr,{changeState:"all",changeQuery:"50% complete_"});
  expect(literal.changeQueue.total).toBe(1);
  expect(literal.nodeChanges[0].events?.[0].reason).toBe("Capacity 50% complete_approved");
  const clamped=await organizationOverview(db,hr,{changeState:"draft",changePage:"99"});
  expect(clamped.changeQueue.page).toBe(2);
  const branchWorkspace:WorkContext={...hr,scope:{...hr.scope,kind:"branch",branchId:f.branch,branchSlug:"scoped-branch",branchName:"Scoped branch"}};
  for(const actor of [f.claimant,{...hr,assignmentScope:"branch" as const,branchId:f.branch},branchWorkspace]){
    const restricted=await organizationOverview(db,actor,{changeState:"all"});
    expect(restricted.canManageUnits).toBe(false);expect(restricted.nodeChanges).toEqual([]);expect(restricted.changeQueue.total).toBe(0);
  }
  await expect(saveOrganizationNodeChange(db,branchWorkspace,null,organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:"wrong-context",displayName:"Wrong context",reason:"Company review from branch workspace",expectedVersion:0}),key(),true)).rejects.toMatchObject({code:"COMPANY_AUTHORITY_REQUIRED"});
  const otherCompany=await organizationOverview(db,{...hr,companyId:`${f.company}-other`},{changeState:"all"});
  expect(otherCompany.nodeChanges).toEqual([]);
});


it("connects draft, review, correction, completion and cancellation to Work and durable Inbox links",async()=>{
  const input=organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`queue-${key()}`,displayName:"Work queue branch",expectedVersion:0,reason:"Open a reviewed operating branch"});
  const proposal=await saveOrganizationNodeChange(db,f.owner,null,input,key(),false);
  const work=async(actor:WorkContext)=>(await organizationChangeWork(db,actor)).find(item=>item.id===proposal.id);
  const notices=async(actor:WorkContext)=>(await listStaffNotifications(db,actor)).filter(item=>item.record_id===proposal.id);
  expect(await work(f.owner)).toMatchObject({queue:"todo",canAct:true,relationships:["requested"]});
  expect(await work(hr)).toBeUndefined();expect(await notices(hr)).toEqual([]);
  const submitKey=key();
  await saveOrganizationNodeChange(db,f.owner,proposal.id,{...input,expectedVersion:1},submitKey,true);
  await saveOrganizationNodeChange(db,f.owner,proposal.id,{...input,expectedVersion:1},submitKey,true);
  expect(await work(f.owner)).toMatchObject({queue:"waiting",canAct:false});
  expect(await work(hr)).toMatchObject({queue:"todo",canAct:true,relationships:["approval"]});
  expect(await notices(hr)).toHaveLength(1);expect(await notices(f.owner)).toHaveLength(0);
  const href=(await notices(hr))[0].href;
  expect(href).toBe((await work(hr))!.href);
  const filters=Object.fromEntries(new URL(href,"https://example.test").searchParams);
  // The same link survives every state transition, independent of list pagination.
  expect((await organizationOverview(db,hr,filters)).nodeChanges.map(c=>c.id)).toEqual([proposal.id]);
  await decideOrganizationNodeChange(db,hr,proposal.id,{expectedVersion:2,decision:"reject",reason:"Include staffing and operating hours"},key());
  expect(await work(f.owner)).toMatchObject({queue:"todo",canAct:true,detail:"Include staffing and operating hours"});
  expect((await notices(f.owner))[0].message).toContain("returned for correction");
  await saveOrganizationNodeChange(db,f.owner,proposal.id,{...input,expectedVersion:3},key(),true);
  const decisionKey=key(),decision={expectedVersion:4,decision:"approve" as const,reason:"Revised staffing and operating hours verified"};
  await decideOrganizationNodeChange(db,hr,proposal.id,decision,decisionKey);
  await decideOrganizationNodeChange(db,hr,proposal.id,decision,decisionKey);
  expect(await work(f.owner)).toMatchObject({queue:"completed",canAct:false,status:"applied"});
  expect(await work(hr)).toMatchObject({queue:"completed",canAct:false});
  expect(await notices(f.owner)).toHaveLength(2);
  expect((await organizationOverview(db,hr,filters)).nodeChanges[0]).toMatchObject({id:proposal.id,state:"applied"});
  expect((await organizationOverview(db,hr,{changeId:"missing",changeState:"all"})).nodeChanges).toEqual([]);

  const cancelled=await saveOrganizationNodeChange(db,f.owner,null,{...input,slug:`cancel-${key()}`},key(),true);
  const cancelKey=key();await cancelOrganizationNodeChange(db,f.owner,cancelled.id,1,cancelKey);
  await cancelOrganizationNodeChange(db,f.owner,cancelled.id,1,cancelKey);
  expect((await listStaffNotifications(db,hr)).filter(n=>n.record_id===cancelled.id).map(n=>n.record_version).sort()).toEqual([1,2]);
  expect((await organizationChangeWork(db,hr)).find(n=>n.id===cancelled.id)).toBeUndefined();
  expect((await organizationChangeWork(db,f.owner)).find(n=>n.id===cancelled.id)).toMatchObject({queue:"completed",canAct:false});
});

it("removes Work, linked records and Inbox evidence when company authority expires or is revoked",async()=>{
  const reviewer=await f.member("notification-reviewer","hr_admin");
  const input=organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`authority-${key()}`,displayName:"Restricted organization evidence",expectedVersion:0,reason:"Independent company review required"});
  const proposal=await saveOrganizationNodeChange(db,f.owner,null,input,key(),true);
  expect(await organizationChangeNotificationTarget(db,reviewer,proposal.id)).toBeTruthy();
  for(const actor of [f.claimant,{...reviewer,assignmentScope:"department" as const},{...reviewer,scope:{...reviewer.scope,kind:"branch" as const,branchId:f.branch,branchSlug:"branch",branchName:"Branch"}},{...reviewer,companyId:`${f.company}-other`}]){
    expect(await organizationChangeWork(db,actor)).toEqual([]);
    expect(await organizationChangeNotificationTarget(db,actor,proposal.id)).toBeNull();
  }
  await db.prepare("UPDATE assignments SET valid_until=1 WHERE id=?").bind(reviewer.assignmentId).run();
  expect(await organizationChangeWork(db,reviewer)).toEqual([]);
  expect(await listStaffNotifications(db,reviewer)).toEqual([]);
  expect((await organizationOverview(db,reviewer,{changeId:proposal.id})).nodeChanges).toEqual([]);
  await db.prepare("UPDATE assignments SET valid_until=9999999999999,revoked_at=? WHERE id=?").bind(Date.now(),reviewer.assignmentId).run();
  expect(await organizationChangeNotificationTarget(db,reviewer,proposal.id)).toBeNull();
  expect(await organizationChangeWork(db,reviewer)).toEqual([]);
  const second=await saveOrganizationNodeChange(db,f.owner,null,{...input,slug:`second-${key()}`},key(),true);
  expect(await db.prepare("SELECT 1 FROM staff_notifications WHERE user_id=? AND record_id=?").bind(reviewer.userId,second.id).first()).toBeNull();
});

it("keeps linked requester identities out of the independent review queue",async()=>{
  const proposal=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`linked-${key()}`,displayName:"Linked identity branch",expectedVersion:0,reason:"Another person must review this proposal"}),key(),true);
  const linked={...hr,iam:{roleRevision:1,identityVersion:1,permissions:["organization.manage"],grantablePermissions:[],principalId:"linked-person",personId:null,personUserIds:[f.owner.userId,hr.userId],freshUntil:null}};
  expect((await organizationChangeWork(db,linked)).find(item=>item.id===proposal.id)).toMatchObject({queue:"waiting",canAct:false,relationships:["team"]});
  const before=(await db.prepare("SELECT count(*) n FROM staff_notifications WHERE record_id=?").bind(proposal.id).first())!;
  await expect(decideOrganizationNodeChange(db,linked,proposal.id,{expectedVersion:1,decision:"approve",reason:"Cannot review through another account"},key())).rejects.toMatchObject({code:"INDEPENDENT_REVIEW_REQUIRED"});
  expect(await db.prepare("SELECT count(*) n FROM staff_notifications WHERE record_id=?").bind(proposal.id).first()).toEqual(before);
  for(const user of [f.owner,hr])await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?)")
    .bind(f.company,user.userId,`principal-${user.userId}`,`person-${f.owner.userId}`).run();
  // A stale context without personUserIds must still use current identity linkage.
  expect((await organizationChangeWork(db,hr)).find(item=>item.id===proposal.id)?.canAct).toBe(false);
  expect((await organizationOverview(db,hr,{changeId:proposal.id})).nodeChanges[0].canReview).toBe(false);
  await expect(decideOrganizationNodeChange(db,hr,proposal.id,{expectedVersion:1,decision:"approve",reason:"Linked since the page was loaded"},key())).rejects.toMatchObject({code:"INDEPENDENT_REVIEW_REQUIRED"});
  const next=await saveOrganizationNodeChange(db,f.owner,null,organizationNodeChangeSchema.parse({action:"create",kind:"branch",slug:`linked-next-${key()}`,displayName:"Next linked identity proposal",expectedVersion:0,reason:"Independent reviewer only"}),key(),true);
  expect(await db.prepare("SELECT 1 FROM staff_notifications WHERE record_id=? AND user_id=?").bind(next.id,hr.userId).first()).toBeNull();
});

it("runs Company setup acceptance with readiness blockers, independent review, history and revocation",async()=>{
  const readiness=await companyBusinessAcceptance(db,f.owner);
  expect(readiness.status).toBe("draft");
  expect(readiness.canSubmit).toBe(false);
  expect(readiness.checks.find(check=>check.id==="staff-ready")?.ready).toBe(false);
  await expect(submitCompanyBusinessAcceptance(db,f.owner,{reason:"Initial Company operating setup reviewed"},key())).rejects.toMatchObject({code:"COMPANY_SETUP_INCOMPLETE"});

  await db.prepare("UPDATE hr_engagements SET position_id=?,level_id=?,manager_person_id=? WHERE company_id=? AND person_id<>?").bind(staffPosition,level(6),`person-${f.owner.userId}`,f.company,`person-${f.owner.userId}`).run();
  const ready=await companyBusinessAcceptance(db,f.owner);
  expect(ready.canSubmit).toBe(true);
  const submitted=await submitCompanyBusinessAcceptance(db,f.owner,{reason:"All Company structure evidence reviewed"},key());
  expect(submitted).toMatchObject({status:"submitted",version:1});
  expect(await companyBusinessAcceptanceWork(db,hr)).toMatchObject([{id:f.company,queue:"todo",canAct:true,relationships:["approval"]}]);
  expect((await listStaffNotifications(db,hr)).find(notification=>notification.record_type==="company_business_acceptance"&&notification.record_id===f.company)?.href).toBe("/settings/business-readiness#company-setup-acceptance");
  expect((await companyBusinessAcceptance(db,hr)).canReview).toBe(true);
  await expect(decideCompanyBusinessAcceptance(db,f.owner,{expectedVersion:1,decision:"accept",reason:"Independent review is required"},key())).rejects.toMatchObject({code:"INDEPENDENT_REVIEW_REQUIRED"});
  const rejected=await decideCompanyBusinessAcceptance(db,hr,{expectedVersion:1,decision:"reject",reason:"Attach the final staff placement evidence"},key());
  expect(rejected).toMatchObject({status:"rejected",version:2});
  const resubmitted=await submitCompanyBusinessAcceptance(db,f.owner,{reason:"Final staff placement evidence attached"},key());
  expect(resubmitted).toMatchObject({status:"submitted",version:3});
  const accepted=await decideCompanyBusinessAcceptance(db,hr,{expectedVersion:3,decision:"accept",reason:"Structure and staff placement independently verified"},key());
  expect(accepted).toMatchObject({status:"accepted",version:4});
  expect((await companyBusinessAcceptance(db,f.owner)).status).toBe("accepted");
  const revoked=await revokeCompanyBusinessAcceptance(db,f.owner,{expectedVersion:4,reason:"Company structure changed; reopen setup review"},key());
  expect(revoked).toMatchObject({status:"revoked",version:5});
  expect((await db.prepare("SELECT action,version FROM company_business_acceptance_events WHERE company_id=? ORDER BY version").bind(f.company).all()).results).toEqual([{action:"submitted",version:1},{action:"rejected",version:2},{action:"submitted",version:3},{action:"accepted",version:4},{action:"revoked",version:5}]);
});
