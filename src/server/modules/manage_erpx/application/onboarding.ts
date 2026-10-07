import {z} from 'zod';
import {BusinessRuleError,type Clock} from '../../shared';
import {requireVerified,onboardingSchema,type BetaUser,type SlotRow,type BetaResourceSlot,type BetaCapacity} from '../domain/onboarding';
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
export function createBetaOnboardingApplication<Effect>(ports:{control:BetaControlRepository<Effect>;holding(id:string):BetaHoldingRepository<Effect>;publicBeta:boolean;resourceSlots():BetaResourceSlot[];limits():BetaCapacity;clock:Clock}){async function onboard(
  user: BetaUser,
  input: z.infer<typeof onboardingSchema>,
) {
  requireVerified(user);
  if (!ports.publicBeta)
    throw new BusinessRuleError(403, "BETA_CLOSED", "Public registration is closed.");
  const data = onboardingSchema.parse(input),
    r = ports.control,
    now = ports.clock.now();
  let slot = await r.ownerSlot(user.id);
  if (!slot) {
    if (
      await r.slugBinding(data.slug)
    )
      throw new BusinessRuleError(
        409,
        "SLUG_UNAVAILABLE",
        "Choose another company URL.",
      );
    try {
      slot = await r.claimSlot(user.id,data.slug,data.companyName,data.branchName,now,user.id);
    } catch {
      slot = await r.recoverOwnerSlot(user.id);
      if (!slot)
        throw new BusinessRuleError(
          409,
          "SLUG_UNAVAILABLE",
          "Choose another company URL.",
        );
    }
    if (!slot)
      slot = await r.currentOwnerSlot(user.id);
    if (!slot) {
      await r.waitlist(user.id,now);
      return { state: "waitlisted", href: null };
    }
  }
  if (
    slot.slug !== data.slug ||
    slot.company_name !== data.companyName ||
    slot.branch_name !== data.branchName
  )
    throw new BusinessRuleError(
      409,
      "ONBOARDING_ALREADY_STARTED",
      "Resume using the original company details.",
    );
  const resource = ports.resourceSlots().find(
    (s) => s.id === slot.holding_id && s.beta,
  );
  if (!resource)
    throw new BusinessRuleError(
      503,
      "SLOT_UNAVAILABLE",
      "Your reserved company is being prepared. Try again shortly.",
    );
  const h = ports.holding(slot.holding_id),
    company = `company-${slot.holding_id}`,
    branch = `branch-${slot.holding_id}`,
    limits = ports.limits();
  // The holding transaction is the authoritative bootstrap boundary. The
  // control-plane activation below is deliberately retriable: a completed
  // receipt means the graph is safe to expose even when binding activation
  // previously failed.
  const bootstrap = createBetaBootstrapEffects(h,ports.clock);
  const bootstrapKey = `bootstrap-${slot.holding_id}`;
  const receipt = await h.provisioningReceipt(company,slot.holding_id);
  if (receipt?.status !== "active") {
    await h.batch([
      h.guardBootstrapOwner(bootstrapKey,slot.holding_id,user.id,slot.holding_id,user.id),
      h.prepareProvisioning(company,slot.holding_id,user.id,now,now),
      h.prepareStartedEvent(`company-provisioning-started-${slot.holding_id}`,company,slot.holding_id,user.id,JSON.stringify({ companyName: slot.company_name, branchName: slot.branch_name, slug: slot.slug }),now),
      h.prepareBootstrap(slot.holding_id,user.id,now),
      h.prepareCompany(company,slot.slug,slot.company_name,company),
      ...bootstrap.branchStatements(company, branch, slot.branch_name, "main"),
      h.prepareMemberLevel(`level-${company}`,company,now,now),
      ...[
        [1, "CEO"],
        [2, "Executive / Director"],
        [3, "Branch / Department Manager"],
        [4, "Supervisor"],
        [5, "Senior Staff"],
        [6, "Staff"],
      ].map(([number, name]) =>
        h.prepareOrganizationLevel(`org-level-${company}-${number}`,company,`ORG_L${number}`,name,-Number(number),Number(number),now,now),
      ),
      h.prepareCeoPosition(`company-ceo-${company}`,company,`org-level-${company}-1`,user.id,now,now),
      h.preparePolicy(company,limits.members,limits.branches,limits.ai,limits.storage,now),
      ...bootstrap.memberStatements(company, branch, user, true, null),
      h.prepareProvisioned(now + 1,now + 1,company,slot.holding_id),
      h.prepareCompletedEvent(`company-provisioning-completed-${slot.holding_id}`,company,slot.holding_id,user.id,JSON.stringify({ levels: 6, ceoPositions: 1, branches: 1, departments: 1, owner: user.id }),now + 1),
      h.clearBootstrapAssertions(bootstrapKey),
    ]);
  }
  const completed = await h.completedReceipt(company,slot.holding_id);
  if (!completed) throw new BusinessRuleError(503, "PROVISIONING_INCOMPLETE", "Company setup is still being prepared. Try again shortly.");
  await r.batch([
    r.preparePrincipal(`principal-${user.id}`,user.id,user.name,now),
    r.prepareBinding(slot.holding_id,slot.slug,slot.company_name,resource.write,resource.read,resource.files),
    r.prepareBilling(slot.holding_id,now),
    r.prepareActiveSlot(now,slot.holding_id,user.id),
  ]);
  return { state: "active", href: `/holdings/${slot.slug}/home` };
}
async function view(user:BetaUser){requireVerified(user);return {enabled:ports.publicBeta,slot:await ports.control.onboardingSlot(user.id)};}
return {onboard,view};}
