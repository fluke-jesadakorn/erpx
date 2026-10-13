import type { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { installStaffLevels, savePosition, positionSchema } from "../../../src/server/modules/organization/public/organization";

/** Explicit test fixture mapping; this never remaps application staff from roles. */
export async function mapSpendingOrganization(f:Awaited<ReturnType<typeof seedExpenseCompany>>) {
  const key=()=>crypto.randomUUID(),level=(n:number)=>`org-level-${f.company}-${n}`;
  await installStaffLevels(f.db,f.owner,key());
  const ceo=String((await savePosition(f.db,f.owner,null,positionSchema.parse({title:"CEO",levelId:level(1),branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:"Fixture organization"}),key())).id);
  const positions=new Map<number,string>([[1,ceo]]);
  for(const n of [3,4,5,6])positions.set(n,String((await savePosition(f.db,f.owner,null,positionSchema.parse({title:`Test position ${n}`,levelId:level(n),branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:"Fixture organization"}),key())).id));
  for(const [actor,n] of [[f.owner,1],[f.head,3],[f.tie,3],[f.it,4],[f.reviewer,5],[f.manager,4],[f.payer,5],[f.claimant,6]] as const){
    await f.db.prepare("UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE person_id=?").bind(level(n),positions.get(n),n===1?null:`person-${actor===f.claimant?f.head.userId:f.owner.userId}`,`person-${actor.userId}`).run();
  }
  await f.db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(f.company,f.company,ceo,`person-${f.owner.userId}`).run();
  await f.db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,person_id,effective_at) VALUES(?,'head',?,1,?,1)").bind(f.company,f.department,`person-${f.head.userId}`).run();
  return {level,positions};
}
