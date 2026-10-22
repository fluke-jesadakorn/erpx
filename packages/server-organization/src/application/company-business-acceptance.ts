import type {z} from "zod";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {companyAcceptanceSubmitSchema,companyAcceptanceDecisionSchema,companyAcceptanceRevokeSchema,type Check,type AcceptanceRow,type EventRow} from "@erpx/server-organization/domain/company-business-acceptance";
export interface OwnerCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>):Promise<Result>;}
export type AcceptanceRepository<Effect>={company(companyId:string,companyId2:string):Promise<{ id: string; display_name: string; status: string }|null>;
branches(companyId:string):Promise<{ count: number }|null>;
levels(companyId:string):Promise<{ count: number }|null>;
ceo(companyId:string):Promise<{ count: number }|null>;
incompletePlacement(companyId:string):Promise<{ count: number }|null>;
acceptance(companyId:string):Promise<AcceptanceRow|null>;
events(companyId:string):Promise<{results:EventRow[]}>;
submitCurrent(companyId:string):Promise<AcceptanceRow|null>;
submitVersion(key:string,value:"acceptance-version",companyId:string,version:number):Effect;
submitAcceptance(companyId:string,recordVersion:number,userId:string,now:number,reason:string,now2:number):Effect;
submittedEvent(value:string,companyId:string,userId:string,recordVersion:number,reason:string,now:number):Effect;
reviewerNotifications(key:string,companyId:string,companyId2:string,recordVersion:number,now:number,companyId3:string,userId:string,now2:number,now3:number):Effect;
reviewCurrent(companyId:string):Promise<AcceptanceRow|null>;
reviewVersion(key:string,value:"acceptance-version",companyId:string,expectedVersion:number):Effect;
independentReview(key:string,value:"independent-review",userId:string,requested_by:string,companyId:string):Effect;
reviewAcceptance(status:"rejected" | "accepted",version:number,userId:string,now:number,reason:string,now2:number,companyId:string):Effect;
reviewEvent(value:string,companyId:string,status:"rejected" | "accepted",userId:string,version:number,reason:string,now:number):Effect;
resultNotifications(key:string,value:"Company setup acceptance was approved" | "Company setup acceptance was returned for correction",now:number,companyId:string):Effect;
revokeCurrent(companyId:string):Promise<AcceptanceRow|null>;
revokeVersion(key:string,value:"acceptance-version",companyId:string,expectedVersion:number):Effect;
revokeAcceptance(version:number,now:number,reason:string,now2:number,companyId:string):Effect;
revokeEvent(value:string,companyId:string,userId:string,version:number,reason:string,now:number):Effect;
workCurrent(companyId:string):Promise<AcceptanceRow|null>;};
export function createCompanyAcceptanceService<Effect>(ports:{repository:AcceptanceRepository<Effect>;commands:OwnerCommands<Effect>;companyAuthority(actor:WorkContext):Promise<boolean>;now():number;error(status:number,code:string,message:string):Error}) {
function requireCompanyScope(actor: WorkContext) {
  if (actor.branchId || actor.departmentId || actor.scope.kind !== "holding")
    throw ports.error(403, "COMPANY_AUTHORITY_REQUIRED", "Open the Company workspace to review Company acceptance.");
  return actor;
}
async function readChecks(actor: WorkContext): Promise<Check[]> {
  const company = await ports.repository.company(actor.companyId,actor.companyId);
  const branches = await ports.repository.branches(actor.companyId);
  const levels = await ports.repository.levels(actor.companyId);
  const ceo = await ports.repository.ceo(actor.companyId);
  const incomplete = await ports.repository.incompletePlacement(actor.companyId);
  return [
    { id: "company-active", label: "Company identity is active", ready: company?.status === "active", owner: "Company administrator", detail: company?.display_name ?? "Company record is unavailable." },
    { id: "branch-ready", label: "At least one active branch", ready: Number(branches?.count ?? 0) > 0, owner: "Company administrator", detail: Number(branches?.count ?? 0) > 0 ? `${branches?.count} active branch(es)` : "Create and approve the first branch." },
    { id: "levels-ready", label: "Six numbered staff levels", ready: Number(levels?.count ?? 0) === 6, owner: "Company HR administrator", detail: Number(levels?.count ?? 0) === 6 ? "Levels 1–6 are installed." : "Install all six numbered levels." },
    { id: "ceo-ready", label: "Active CEO position", ready: Number(ceo?.count ?? 0) === 1, owner: "People administrator", detail: Number(ceo?.count ?? 0) === 1 ? "One active CEO position is configured." : "Create exactly one active CEO position." },
    { id: "staff-ready", label: "Active staff have an organization placement", ready: Number(incomplete?.count ?? 0) === 0, owner: "People manager", detail: Number(incomplete?.count ?? 0) === 0 ? "All active staff have a level and position." : `${incomplete?.count} active staff record(s) need a level or position.` },
  ];
}
async function companyBusinessAcceptance(actor: WorkContext) {
  requireCompanyScope(actor);
  if (!(await ports.companyAuthority(actor))) throw ports.error(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  const [checks, acceptance, eventRows] = await Promise.all([
    readChecks(actor),
    ports.repository.acceptance(actor.companyId),
    ports.repository.events(actor.companyId),
  ]);
  const independent = acceptance ? !samePerson(actor, acceptance.requested_by) : false;
  const canReview = !!acceptance && acceptance.status === "submitted" && independent && hasPermission(actor, "organization.manage");
  const canSubmit = checks.every(check => check.ready) && (!acceptance || ["draft", "rejected", "revoked"].includes(acceptance.status));
  const canRevoke = acceptance?.status === "accepted" && samePerson(actor, acceptance.requested_by);
  return {
    companyId: actor.companyId,
    status: acceptance?.status ?? "draft",
    version: acceptance?.version ?? 0,
    checks,
    blockers: checks.filter(check => !check.ready).map(check => check.detail),
    canSubmit,
    canReview,
    canRevoke,
    requestedBy: acceptance?.requested_by ?? null,
    reviewedBy: acceptance?.reviewed_by ?? null,
    reason: acceptance?.reason ?? null,
    reviewReason: acceptance?.review_reason ?? null,
    updatedAt: acceptance?.updated_at ?? null,
    events: eventRows.results,
  };
}
async function submitCompanyBusinessAcceptance(actor: WorkContext, input: z.infer<typeof companyAcceptanceSubmitSchema>, key: string) {
  requireCompanyScope(actor);
  if (!(await ports.companyAuthority(actor))) throw ports.error(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  input = companyAcceptanceSubmitSchema.parse(input);
  const operation="organization.company-acceptance.submit",recordId=actor.companyId;
  const prior=await ports.commands.replay(actor,key,operation,recordId,input);if(prior)return prior;
  const checks = await readChecks(actor);
  if (checks.some(check => !check.ready)) throw ports.error(409, "COMPANY_SETUP_INCOMPLETE", `Complete Company setup before acceptance: ${checks.filter(check => !check.ready).map(check => check.label).join(", ")}.`);
  const current = await ports.repository.submitCurrent(actor.companyId);
  if (current && !["draft", "rejected", "revoked"].includes(current.status)) throw ports.error(409, "COMPANY_ACCEPTANCE_LOCKED", "This Company acceptance is already submitted or accepted.");
  const recordVersion = (current?.version ?? 0) + 1, now = ports.now();
  const result = { companyId: actor.companyId, status: "submitted" as const, version: recordVersion };
  return ports.commands.commit(actor, key, operation, input, recordId, recordVersion, result, [
    ...(current ? [ports.repository.submitVersion(key,"acceptance-version",actor.companyId,current.version)] : []),
    ports.repository.submitAcceptance(actor.companyId,recordVersion,actor.userId,now,input.reason,now),
    ports.repository.submittedEvent(`company-acceptance-event-${key}`,actor.companyId,actor.userId,recordVersion,input.reason,now),
    ports.repository.reviewerNotifications(key,actor.companyId,actor.companyId,recordVersion,now,actor.companyId,actor.userId,now,now),
  ]);
}
async function decideCompanyBusinessAcceptance(actor: WorkContext, input: z.infer<typeof companyAcceptanceDecisionSchema>, key: string) {
  requireCompanyScope(actor);
  if (!(await ports.companyAuthority(actor))) throw ports.error(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  input = companyAcceptanceDecisionSchema.parse(input);
  const operation = "organization.company-acceptance.review", prior = await ports.commands.replay(actor, key, operation, actor.companyId, input); if (prior) return prior;
  const current = await ports.repository.reviewCurrent(actor.companyId);
  if (!current) throw ports.error(404, "COMPANY_ACCEPTANCE_NOT_FOUND", "Submit Company setup for review first.");
  if (current.status !== "submitted") throw ports.error(409, "COMPANY_ACCEPTANCE_NOT_SUBMITTED", "Only submitted Company setup can be reviewed.");
  if (samePerson(actor, current.requested_by)) throw ports.error(403, "INDEPENDENT_REVIEW_REQUIRED", "A different Company administrator must review this acceptance.");
  if (current.version !== input.expectedVersion) throw ports.error(409, "STALE_VERSION", "Company acceptance changed. Reload and review it again.");
  const now = ports.now(), version = current.version + 1, status = input.decision === "accept" ? "accepted" : "rejected";
  const result = { companyId: actor.companyId, status, version };
  return ports.commands.commit(actor, key, operation, input, actor.companyId, version, result, [
    ports.repository.reviewVersion(key,"acceptance-version",actor.companyId,input.expectedVersion),
    ports.repository.independentReview(key,"independent-review",actor.userId,current.requested_by,actor.companyId),
    ports.repository.reviewAcceptance(status,version,actor.userId,now,input.reason,now,actor.companyId),
    ports.repository.reviewEvent(`company-acceptance-event-${key}`,actor.companyId,status,actor.userId,version,input.reason,now),
    ports.repository.resultNotifications(key,status === "accepted" ? "Company setup acceptance was approved" : "Company setup acceptance was returned for correction",now,actor.companyId),
  ]);
}
async function revokeCompanyBusinessAcceptance(actor: WorkContext, input: z.infer<typeof companyAcceptanceRevokeSchema>, key: string) {
  requireCompanyScope(actor);
  if (!(await ports.companyAuthority(actor))) throw ports.error(403, "COMPANY_AUTHORITY_REQUIRED", "An active Company organization-management assignment is required.");
  input = companyAcceptanceRevokeSchema.parse(input);
  const operation = "organization.company-acceptance.revoke", prior = await ports.commands.replay(actor, key, operation, actor.companyId, input); if (prior) return prior;
  const current = await ports.repository.revokeCurrent(actor.companyId);
  if (!current || current.status !== "accepted") throw ports.error(409, "COMPANY_ACCEPTANCE_NOT_ACTIVE", "Only an accepted Company setup can be revoked.");
  if (!samePerson(actor, current.requested_by)) throw ports.error(403, "ACCEPTANCE_OWNER_REQUIRED", "Only the Company acceptance owner can revoke it.");
  if (current.version !== input.expectedVersion) throw ports.error(409, "STALE_VERSION", "Company acceptance changed. Reload and review it again.");
  const now = ports.now(), version = current.version + 1, result = { companyId: actor.companyId, status: "revoked" as const, version };
  return ports.commands.commit(actor, key, operation, input, actor.companyId, version, result, [
    ports.repository.revokeVersion(key,"acceptance-version",actor.companyId,input.expectedVersion),
    ports.repository.revokeAcceptance(version,now,input.reason,now,actor.companyId),
    ports.repository.revokeEvent(`company-acceptance-event-${key}`,actor.companyId,actor.userId,version,input.reason,now),
  ]);
}
async function companyBusinessAcceptanceWork(actor: WorkContext): Promise<WorkItem[]> {
  if (actor.beta || actor.branchId || actor.departmentId || actor.scope.kind !== "holding" || !hasPermission(actor, "organization.manage")) return [];
  if (!(await ports.companyAuthority(actor))) return [];
  const row = await ports.repository.workCurrent(actor.companyId);
  if (!row || row.status === "draft" || row.status === "revoked") return [];
  const requester = samePerson(actor, row.requested_by), review = row.status === "submitted" && !requester && hasPermission(actor, "organization.manage");
  return [{ id: actor.companyId, type: "company_business_acceptance", title: "Company setup acceptance", detail: row.status === "submitted" ? (review ? "Review the Company setup evidence and accept or return it." : "Waiting for an independent Company administrator review.") : row.review_reason ?? "Company setup acceptance is complete.", href: "/settings/business-readiness#company-setup-acceptance", status: row.status, updatedAt: Number(row.updated_at), queue: review ? "todo" : requester && row.status === "rejected" ? "todo" : "waiting", responsibleQueue: review ? "Company administrator" : "Requester", nextAction: review ? "Accept or return Company setup" : row.status === "rejected" ? "Correct the Company setup evidence" : "Review the retained acceptance record", relationships: [ ...(requester ? ["requested" as const] : []), ...(review ? ["approval" as const] : []) ], canAct: review || (requester && row.status === "rejected") }];
}
return {companyBusinessAcceptance,submitCompanyBusinessAcceptance,decideCompanyBusinessAcceptance,revokeCompanyBusinessAcceptance,companyBusinessAcceptanceWork};}

