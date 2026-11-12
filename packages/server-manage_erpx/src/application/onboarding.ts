import {type Clock} from '@erpx/server-shared/index';
import {type BetaUser,type SlotRow} from '@erpx/server-manage_erpx/domain/onboarding';
export interface BetaHoldingRepository<Effect>{prepareMembership(id:string,invitation:string | null,now:number):Effect;
preparePerson(person:string,id:string,company:string,name:string,email:string,id2:string,now:number,now2:number):Effect;
prepareEmployee(value:string,id:string,company:string,branch:string,department:string,name:string,now:number):Effect;
prepareEngagement(value:string,person:string,company:string,branch:string,department:string,id:string,now:number,now2:number,value2:string):Effect;
prepareAssignment(value:string,id:string,value2:"holding_owner" | "employee",company:string,value3:string | null,now:number,value4:number,value5:"holding" | "branch",value6:string | null,value7:string | null,value8:"manual" | "legacy"):Effect;
prepareBranch(id:string,slug:string,name:string,company:string,company2:string,id2:string):Effect;
prepareDepartment(value:string,id:string,company:string,id2:string):Effect;
provisioningReceipt(company:string,holding_id:string):Promise<{ status: string }|null>;
guardBootstrapOwner(bootstrapKey:string,holding_id:string,id:string,holding_id2:string,id2:string):Effect;
prepareProvisioning(company:string,holding_id:string,id:string,now:number,now2:number):Effect;
prepareStartedEvent(value:string,company:string,holding_id:string,id:string,value2:string,now:number):Effect;
prepareBootstrap(holding_id:string,id:string,now:number):Effect;
prepareCompany(company:string,slug:string,company_name:string,company2:string):Effect;
prepareMemberLevel(value:string,company:string,now:number,now2:number):Effect;
prepareOrganizationLevel(value:string,company:string,value2:string,name:string | number,value3:number,value4:number,now:number,now2:number):Effect;
prepareCeoPosition(value:string,company:string,value2:string,id:string,now:number,now2:number):Effect;
preparePolicy(company:string,members:number,branches:number,ai:number,storage:number,now:number):Effect;
prepareProvisioned(value:number,value2:number,company:string,holding_id:string):Effect;
prepareCompletedEvent(value:string,company:string,holding_id:string,id:string,value2:string,value3:number):Effect;
clearBootstrapAssertions(bootstrapKey:string):Effect;
completedReceipt(company:string,holding_id:string):Promise<{ status: string }|null>;batch(effects:Effect[]):Promise<void>;}
export interface BetaControlRepository<Effect>{onboardingSlot(userId:string):Promise<{slug:string;company_name:string;branch_name:string;state:string}|null>;ownerSlot(id:string):Promise<SlotRow|null>;
slugBinding(slug:string):Promise<Record<string,unknown>|null>;
claimSlot(id:string,slug:string,companyName:string,branchName:string,now:number,id2:string):Promise<SlotRow|null>;
recoverOwnerSlot(id:string):Promise<SlotRow|null>;
currentOwnerSlot(id:string):Promise<SlotRow|null>;
waitlist(id:string,now:number):Promise<void>;
preparePrincipal(value:string,id:string,name:string,now:number):Effect;
prepareBinding(holding_id:string,slug:string,company_name:string,write:string,read:string,files:string):Effect;
prepareBilling(holding_id:string,now:number):Effect;
prepareActiveSlot(now:number,holding_id:string,id:string):Effect;batch(effects:Effect[]):Promise<void>;}
export function createBetaBootstrapEffects<Effect>(r:BetaHoldingRepository<Effect>,clock:Clock){function memberStatements(
  company: string,
  branch: string,
  user: BetaUser,
  owner: boolean,
  invitation: string | null,
  grant?: { parentId: string; validUntil: number },
) {
  const now = clock.now(),
    person = `person-${user.id}`,
    department = `department-${branch}`;
  return [
    r.prepareMembership(user.id,invitation,now),
    r.preparePerson(person,user.id,company,user.name,user.email,user.id,now,now),
    r.prepareEmployee(`employee-${user.id}`,user.id,company,branch,department,user.name,now),
    r.prepareEngagement(`engagement-${user.id}`,person,company,branch,department,user.id,now,now,`level-${company}`),
    r.prepareAssignment(`beta-assignment-${user.id}`,user.id,owner ? "holding_owner" : "employee",company,owner ? null : branch,now,grant?.validUntil ?? now + 90 * 86400000,owner ? "holding" : "branch",owner ? null : branch,grant?.parentId ?? null,grant ? "manual" : "legacy"),
  ];
}
function branchStatements(
  company: string,
  id: string,
  name: string,
  slug: string,
) {
  return [
    r.prepareBranch(id,slug,name,company,company,id),
    r.prepareDepartment(`department-${id}`,id,company,id),
  ];
}
return {memberStatements,branchStatements};}
