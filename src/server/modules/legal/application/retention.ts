import type {Clock,IdentifierSource} from "../../shared/application/ports";
import type {LegalDestructionManifest} from "../domain/destruction-sources";
import {z} from "zod";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {LegalItem} from "../domain/model";
import type {LegalWrite,LegalRows} from "./service";
export type LegalRetentionRepository={
policies(facts:{companyId:string;}):Promise<LegalRows<Record<string, unknown>>>;
assignment(facts:{id:string;}):Promise<Record<string, unknown> | null>;
requests(facts:{id:string;}):Promise<LegalRows<{id:string}&Record<string, unknown>>>;
createPolicy(facts:{key:string;companyId:string;name:string;days:number;reason:string;actorId:string;now:number;}):LegalWrite;
policy(facts:{policyId:string | undefined;companyId:string;}):Promise<{ id: string; created_by: string; state: string; version: number; } | null>;
decidePolicy(facts:{fact0:"approved" | "retired";actorId:string;id:string;}):LegalWrite;
approvedPolicy(facts:{policyId:string | undefined;companyId:string;}):Promise<{ id: string; days: number; version: number; } | null>;
assignRetention(facts:{id:string;id2:string;version:number;updated_at:number;timezone:string;days:number;timezone2:string;actorId:string;now:number;}):LegalWrite;
eligibleAssignment(facts:{id:string;now:number;}):Promise<{ policy_id: string; policy_version: number; } | null>;
requestDestruction(facts:{key:string;id:string;policy_id:string;policy_version:number;actorId:string;reason:string;manifestJson:string;now:number;}):LegalWrite;
destruction(facts:{requestId:string | undefined;id:string;}):Promise<{ id: string; state: string; requested_by: string; approved_by: string | null; } | null>;
changeDestruction(facts:{state:string;fact1:string | null;fact2:number | null;id:string;}):LegalWrite;
advanceRecord(facts:{now:number;id:string;}):LegalWrite;
requireRetentionPolicyVersion(key:string,values:(string|number)[]):LegalWrite;
requireRetentionNoHold(key:string,values:string[]):LegalWrite;
guardChanges(id:string,version:number,key:string,edit?:boolean,allowFrozen?:boolean):LegalWrite[];
companyLock():LegalWrite;
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export function createLegalRetentionApplication(ports:{actor:WorkContext;repository:LegalRetentionRepository;clock:Clock;identifiers:IdentifierSource;require(permission:string):void;samePerson(userId:string|null|undefined):boolean;access(id:string,permission?:string):Promise<LegalItem>;manifest(itemId:string,companyId:string):Promise<LegalDestructionManifest>;error(status:number,code:string,message:string):Error}){const a=ports.actor;function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function retentionView(
  id?: string,
) {
  ports.require("legal.read");
  if (id) await ports.access(id);
  const policies = (
    await ports.repository.policies({companyId:a.companyId})
  ).results;
  const assignment = id
    ? await ports.repository.assignment({id:id})
    : null;
  const requests = id
    ? (
        await ports.repository.requests({id:id})
      ).results
    : [];
  return { policies, assignment, requests };
}
async function retentionCommand(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum([
        "retention.policy",
        "retention.approve-policy",
        "retention.retire-policy",
        "retention.assign",
        "retention.request",
        "retention.approve",
        "retention.cancel",
        "retention.retry",
      ]),
      id: z.string().optional(),
      expectedVersion: z.number().int().positive().optional(),
      policyId: z.string().optional(),
      policyVersion: z.number().int().positive().optional(),
      requestId: z.string().optional(),
      confirmation: z.string().optional(),
      reason: z.string().trim().min(3).max(4000),
    })
    .parse(raw);
  if (
    x.action.startsWith("retention.") &&
    [
      "retention.policy",
      "retention.approve-policy",
      "retention.retire-policy",
    ].includes(x.action)
  ) {
    ports.require("legal.manage");
    if (a.branchId || a.departmentId)
      fail("Manage retention policies at Company scope.");
    const replay = await ports.repository.replay(key,x.action,a.companyId,raw);
    if (replay) return replay;
    const steps: LegalWrite[] = [ports.repository.companyLock()];
    if (x.action === "retention.policy") {
      const p = z
        .object({
          name: z.string().trim().min(3).max(200),
          days: z.coerce.number().int().min(1).max(36500),
        })
        .parse(raw);
      steps.push(
        ports.repository.createPolicy({key:key,companyId:a.companyId,name:p.name,days:p.days,reason:x.reason,actorId:a.userId,now:ports.clock.now()}),
      );
    } else {
      const p = await ports.repository.policy({policyId:x.policyId,companyId:a.companyId});
      if (!p || p.version !== x.policyVersion) fail("Policy changed. Refresh.");
      if (
        x.action === "retention.approve-policy" &&
        (p.state !== "draft" || ports.samePerson(p.created_by))
      )
        fail(
          "An independent manager must approve the policy.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(
        ports.repository.requireRetentionPolicyVersion(key,[p.id, p.version]),
        ports.repository.decidePolicy({fact0:x.action === "retention.approve-policy" ? "approved" : "retired",actorId:a.userId,id:p.id}),
      );
    }
    return ports.repository.commit(key,x.action,raw,a.companyId,1,{ id: a.companyId, version: 1 },steps);
  }
  const item = await ports.access(z.string().parse(x.id),"legal.destroy");
  const replay = await ports.repository.replay(key,x.action,item.id,raw);
  if (replay) return replay;
  const steps = ports.repository.guardChanges(item.id,z.number().int().positive().parse(x.expectedVersion),key,false,x.action === "retention.retry");
  if (x.action === "retention.assign") {
    if (
      !["archived", "closed", "completed", "expired", "terminated"].includes(
        item.state,
      )
    )
      fail("Close or archive the record before assigning retention.");
    const policy = await ports.repository.approvedPolicy({policyId:x.policyId,companyId:a.companyId});
    if (!policy) fail("Select an approved Company policy.");
    steps.push(
      ports.repository.assignRetention({id:item.id,id2:policy.id,version:policy.version,updated_at:item.updated_at,timezone:item.timezone,days:policy.days,timezone2:item.timezone,actorId:a.userId,now:ports.clock.now()}),
    );
  } else if (x.action === "retention.request") {
    const assignment = await ports.repository.eligibleAssignment({id:item.id,now:ports.clock.now()});
    if (!assignment)
      fail("Retention has not elapsed under the currently approved policy.");
    if (
      !["archived", "closed", "completed", "expired", "terminated"].includes(
        item.state,
      )
    )
      fail("This record is still active.");
    steps.push(noHold( item.id, key));
    const manifest = await ports.manifest(item.id,item.company_id);
    steps.push(
      ports.repository.requestDestruction({key:key,id:item.id,policy_id:assignment.policy_id,policy_version:assignment.policy_version,actorId:a.userId,reason:x.reason,manifestJson:JSON.stringify(manifest),now:ports.clock.now()}),
    );
  } else {
    const r = await ports.repository.destruction({requestId:x.requestId,id:item.id});
    if (!r) fail("Destruction request unavailable.");
    let state: string;
    if (x.action === "retention.approve") {
      if (x.confirmation !== r.id)
        fail("Confirm the exact destruction request ID.");
      if (r.state !== "requested" || ports.samePerson(r.requested_by))
        fail(
          "An independent manager must authorize destruction.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(noHold( item.id, key));
      state = "approved";
    } else if (x.action === "retention.retry") {
      if (!["failed", "blocked", "executing"].includes(r.state))
        fail("Only blocked or failed work can be retried.");
      steps.push(noHold( item.id, key));
      state = r.state === "executing" ? "executing" : "requested";
    } else {
      if (["completed", "executing"].includes(r.state))
        fail("This destruction cannot be cancelled.");
      state = "cancelled";
    }
    steps.push(
      ports.repository.changeDestruction({state:state,fact1:state === "approved" ? a.userId : null,fact2:state === "approved" ? ports.clock.now() : null,id:r.id}),
    );
  }
  steps.push(
    ports.repository.advanceRecord({now:ports.clock.now(),id:item.id}),
    ports.repository.event(item.id,x.action,x.reason,item.version + 1,ports.identifiers.next(),ports.clock.now()),
  );
  return ports.repository.commit(key,x.action,raw,item.id,item.version + 1,{ id: item.id, version: item.version + 1 },steps);
}
function noHold( id: string, key: string) {
  return ports.repository.requireRetentionNoHold(key,[id]);
}

return {retentionView,retentionCommand};}
