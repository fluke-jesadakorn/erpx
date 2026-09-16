import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { env,applyMigrations } from "./runtime-env";
import { expenseFixture,expenseDb as db,key } from "./helpers/expense-fixture";
import { organizationOverview,installStaffLevels,savePosition,positionSchema,saveOrganizationTemplate,applyOrganizationTemplate,organizationGuard } from "../src/server/organization";
import { directoryUpdateSchema,saveDirectoryPerson,setDirectoryStatus } from "../src/server/people-directory";
import { grantPersonAccess,revokePersonAccess } from "../src/server/account-access";
import { executeCommand } from "../src/server/commands";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:db,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,ceo:string,staffPosition:string,hr:WorkContext;
const level=(n:number)=>`org-level-${f.company}-${n}`;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);});
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
