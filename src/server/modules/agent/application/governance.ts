import {z} from 'zod';
import type {WorkContext} from '../../identity_access/public/authorization';
type Row=Record<string,unknown>;
import {hasModuleAccess} from '@/data/module-access';
import {hasPermission} from "@/lib/identity_access/permissions";
import {BusinessRuleError,type Clock} from '../../shared';
import {agentSourceAllowed} from '../domain/source-policy';
import {reason,version,agentSchema,versionSchema,poolSchema,assignSchema,actionSchema,type AssistantGovernance} from '../domain/governance';
export interface AgentGovernanceRepository<Effect>{policyCandidates(companyId:string,userId:string):Promise<(Row)[]>;
activePrivateSession(sessionId:string,companyId:string,userId:string,assignmentId:string,now:number):Promise<Record<string,unknown>|null>;
lockSession(sessionId:string):Effect;
guardPrivateSession(key:string,value:"private-session",sessionId:string,companyId:string,userId:string,assignmentId:string):Effect;
guardDefaultPolicy(key:string,value:"agent-default",companyId:string,path:string,userId:string):Effect;
lockApprovedAssignment(assignmentId:string):Effect;
lockAssignedPool(poolId:string):Effect;
guardApprovedPolicy(key:string,value:"approved-agent",assignmentId:string,companyId:string,assignmentVersion:number | null,agentVersionId:string | null,agentVersionRevision:number | null,catalogVersion:number | null,userId:string,poolId:string | null,poolVersion:number | null):Effect;
workspaceAgents(companyId:string):Promise<(Row)[]>;
workspaceVersions(companyId:string):Promise<(Row)[]>;
workspacePools(companyId:string):Promise<(Row)[]>;
workspaceAssignments(companyId:string):Promise<(Row)[]>;
workspaceInvocations(companyId:string):Promise<(Row)[]>;
workspacePeople(companyId:string,now:number):Promise<(Row)[]>;
workspaceMembers(companyId:string):Promise<(Row)[]>;
workspacePoolHistory(companyId:string):Promise<(Row)[]>;
workspaceSessions(companyId:string,userId:string,value:0 | 1):Promise<(Row)[]>;
prepareAgent(id:string,companyId:string,name:string,purpose:string,userId:string,now:number):Effect;
nextVersionNumber(agentId:string,companyId:string):Promise<{ n: number }|null>;
guardAgent(key:string,value:"agent",agentId:string,companyId:string):Effect;
prepareAgentVersion(id:string,agentId:string,number:number,prompt:string,value:string,userId:string,now:number):Effect;
preparePool(id:string,companyId:string,name:string,userId:string,now:number):Effect;
guardEligibleMember(key:string,value:string,companyId:string,userId:string,now:number):Effect;
preparePoolMember(value:string,id:string,userId:string,userId2:string,now:number):Effect;
prepareInitialPoolHistory(value:string,id:string,value2:string,value3:"Initial eligible routing members",userId:string,now:number):Effect;
lockPool(id:string,companyId:string):Effect;
guardPoolVersion(key:string,value:"pool",id:string,companyId:string,expectedVersion:number):Effect;

prepareRevokeRemovedMembers(now:number,id:string,value:string):Effect;
prepareRestorePoolMember(value:string,id:string,userId:string,userId2:string,now:number):Effect;
preparePoolHistory(value:string,id:string,value2:number,value3:string,reason:string,userId:string,now:number):Effect;
preparePoolLifecycle(status:"retired" | "active",now:number,id:string):Effect;
lockPrivateSession(id:string):Effect;
guardPrivateSessionControl(key:string,value:"private-session-control",id:string,companyId:string,userId:string,value2:0 | 1,expectedVersion:number):Effect;
guardActiveSession(key:string,value:"active-session",id:string):Effect;
prepareExpireSession(now:number,id:string):Effect;
guardRetentionHold(key:string,value:"session-hold",id:string,value2:0 | 1):Effect;
prepareSessionHold(value:0 | 1,value2:string | null,now:number,id:string):Effect;
prepareSessionEvent(value:string,id:string,value2:number,action:"hold" | "release-hold" | "expire",userId:string,reason:string,now:number):Effect;
guardApprovedVersion(key:string,value:"version",agentVersionId:string,companyId:string):Effect;
guardActivePool(key:string,value:"pool",poolId:string,companyId:string):Effect;
guardEligibleAssignee(key:string,value:"user",companyId:string,value2:string,now:number):Effect;
prepareAssignment(id:string,companyId:string,agentVersionId:string,value:string | null,value2:string | null,scopePath:string,userId:string,now:number):Effect;
currentRecord(kind:"agent"|"version"|"assignment",id:string,companyId:string):Promise<Row|null>;
guardLifecycleVersion(kind:"agent"|"version"|"assignment",key:string,value:"record",id:string,expectedVersion:number,expected:"active" | "approved" | "draft"):Effect;
prepareRecordLifecycle(kind:"agent"|"version"|"assignment",status:"retired" | "approved" | "rejected" | "revoked",userId:string,now:number,id:string):Effect;
latestPrivateSession(companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string):Promise<{id:string;generation:number;status:string;expires_at:number}|null>;
preparePrivateSession(id:string,companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string,assignmentId2:string | null,now:number,generation:number):Effect;
preparePrivateSessionControls(id:string,value:number,now:number):Effect;
recordInvocation(value:string,sessionId:string,turnId:string,companyId:string,userId:string,agentVersionId:string | null,value2:string,value3:string,value4:string,value5:string,outcome:"succeeded" | "failed",value6:number | null,value7:number | null,value8:string | null,now:number,value9:string):Promise<void>;
invocationRecord(companyId:string,id:string):Promise<Row|null>;
sessionRecord(companyId:string,id:string,userId:string,value:0 | 1):Promise<Row|null>;
replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[]):Promise<Result>;
batch(effects:Effect[]):Promise<void>;}
export function createAssistantGovernanceGuards<Effect>(r:AgentGovernanceRepository<Effect>){return (a:WorkContext,key:string,path:string,governance:AssistantGovernance):Effect[]=>{
  const sessionGuards=[r.lockSession(governance.sessionId),
    r.guardPrivateSession(key,"private-session",governance.sessionId,a.companyId,a.userId,a.assignmentId)];
  if(!governance.assignmentId)return [...sessionGuards,r.guardDefaultPolicy(key,"agent-default",a.companyId,path,a.userId)];
  return [...sessionGuards,r.lockApprovedAssignment(governance.assignmentId),
    ...(governance.poolId?[r.lockAssignedPool(governance.poolId)]:[]),
    r.guardApprovedPolicy(key,"approved-agent",governance.assignmentId,a.companyId,governance.assignmentVersion,governance.agentVersionId,governance.agentVersionRevision,governance.catalogVersion,a.userId,governance.poolId,governance.poolVersion)];
};}
export interface AgentGovernancePorts<Effect>{repository(actor:WorkContext):Promise<AgentGovernanceRepository<Effect>>;clock:Clock;hash(value:string):Promise<string>;authorize(actor:WorkContext,permission:string):void;}
export function createAgentGovernanceApplication<Effect>(ports:AgentGovernancePorts<Effect>){function access(a: WorkContext, write = false) {
  ports.authorize(a, write ? "ai.manage" : "records.read");
  if (!hasModuleAccess(a.modules, "agent"))
    throw new BusinessRuleError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Agent governance.",
    );
}
async function assistantPolicy(a: WorkContext, path: string): Promise<Omit<AssistantGovernance, "sessionId">> {
const r=await ports.repository(a);
  const candidates = (await r.policyCandidates(a.companyId,a.userId));
  const row = candidates.find(candidate => agentSourceAllowed(path,[String(candidate.scope_path)]));
  if (!row) return {assignmentId:null,assignmentVersion:null,agentVersionId:null,agentVersionRevision:null,catalogVersion:null,poolId:null,poolVersion:null,prompt:null,allowedSources:["/"]};
  if (row.version_status !== "approved" || row.catalog_status !== "approved" || row.pool_id && row.pool_status !== "active")
    throw new BusinessRuleError(409,"AI_AGENT_UNAVAILABLE","This assigned agent is no longer approved or its pool is unavailable. Continue with the manual workflow guide.");
  return {assignmentId:String(row.id),assignmentVersion:Number(row.assignment_version),agentVersionId:String(row.agent_version_id),agentVersionRevision:Number(row.version_revision),catalogVersion:Number(row.catalog_version),poolId:row.pool_id?String(row.pool_id):null,poolVersion:row.pool_id?Number(row.pool_version):null,prompt:String(row.prompt),allowedSources:JSON.parse(String(row.allowed_sources_json)) as string[]};
}
async function validateAssistantGovernance(a:WorkContext,path:string,governance:AssistantGovernance) {
const r=await ports.repository(a);
  const current=await assistantPolicy(a,path),original={...governance};
  delete (original as Partial<AssistantGovernance>).sessionId;
  if(JSON.stringify(current)!==JSON.stringify(original))
    throw new BusinessRuleError(409,"AI_AGENT_CHANGED","The approved agent or its assignment changed. Continue manually or ask again.");
  const session=await r.activePrivateSession(governance.sessionId,a.companyId,a.userId,a.assignmentId,ports.clock.now());
  if(!session)throw new BusinessRuleError(409,"AI_SESSION_EXPIRED","This private session expired. Ask again to start a new session; retained evidence stays private.");
}
async function agentWorkspace(a: WorkContext) {
  access(a);
  const r = await ports.repository(a),
    [agents, versions, pools, assignments, invocations, people, members, poolHistory, sessions] =
      await Promise.all([
        r.workspaceAgents(a.companyId),
        r.workspaceVersions(a.companyId),
        r.workspacePools(a.companyId),
        r.workspaceAssignments(a.companyId),
        r.workspaceInvocations(a.companyId),
        r.workspacePeople(a.companyId,ports.clock.now()),
        r.workspaceMembers(a.companyId),
        r.workspacePoolHistory(a.companyId),
        r.workspaceSessions(a.companyId,a.userId,hasPermission(a,"ai.manage")?1:0),
      ]);
  return {
    agents: agents,
    versions: versions,
    pools: pools,
    assignments: assignments,
    invocations: invocations,
    people: people,
    members: members,
    poolHistory: poolHistory,
    sessions: sessions.map((session):Row & {effective_status:string}=>({...session,effective_status:session.status==="active"&&Number(session.expires_at)<=ports.clock.now()?"expired":String(session.status)})),
  };
}
async function createAgent(a: WorkContext, raw: unknown, key: string) {
  access(a, true);
  const i = agentSchema.parse(raw),
    r = await ports.repository(a),
    id = `agent-${key}`,
    op = "agent.create",
    prior = await r.replay<{id:string;status:string;version:number}>(a, key, op, id, i);
  if (prior) return prior;
  const now = ports.clock.now(),
    result = { id, status: "draft", version: 1 };
  return r.commit(a, key, op, i, id, 1, result, [
    r.prepareAgent(id,a.companyId,i.name,i.purpose,a.userId,now),
  ]);
}
async function createAgentVersion(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = versionSchema.parse(raw),
    r = await ports.repository(a),
    id = `agent-version-${key}`,
    op = "agent.version.create",
    prior = await r.replay<{id:string;status:string;version:number;versionNumber:number}>(a, key, op, id, i);
  if (prior) return prior;
  const current = await r.nextVersionNumber(i.agentId,a.companyId);
  if (current === null)
    throw new BusinessRuleError(404, "AGENT_NOT_FOUND", "Agent not found.");
  const now = ports.clock.now(),
    number = Number(current.n) + 1,
    result = { id, versionNumber: number, status: "draft", version: 1 };
  return r.commit(a, key, op, i, id, 1, result, [
    r.guardAgent(key,"agent",i.agentId,a.companyId),
    r.prepareAgentVersion(id,i.agentId,number,i.prompt,JSON.stringify(i.allowedSources),a.userId,now),
  ]);
}
async function createAgentPool(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = poolSchema.parse(raw),
    r = await ports.repository(a),
    id = `agent-pool-${key}`,
    op = "agent.pool.create",
    prior = await r.replay(a, key, op, id, i);
  if (prior) return prior;
  const now = ports.clock.now(),
    result = { id, status: "active", version: 1 };
  const statements: Effect[] = [
    r.preparePool(id,a.companyId,i.name,a.userId,now),
  ];
  for (const userId of [...new Set(i.userIds)])
    statements.push(
      r.guardEligibleMember(key,`member-${statements.length}`,a.companyId,userId,now),
      r.preparePoolMember(`agent-pool-member-${key}-${await ports.hash(userId)}`,id,userId,a.userId,now),
    );
  statements.push(r.prepareInitialPoolHistory(`agent-pool-revision-${key}`,id,JSON.stringify([...new Set(i.userIds)].sort()),"Initial eligible routing members",a.userId,now));
  return r.commit(a, key, op, i, id, 1, result, statements);
}
async function changeAgentPool(a:WorkContext,id:string,raw:unknown,key:string) {
  access(a,true);
  const i=z.discriminatedUnion("action",[
    z.object({action:z.literal("replace-members"),expectedVersion:version,userIds:z.array(z.string().min(1)).min(1).max(100),reason}),
    z.object({action:z.literal("retire"),expectedVersion:version,reason}),
  ]).parse(raw),r = await ports.repository(a),op=`agent.pool.${i.action}`,prior=await r.replay(a,key,op,id,i);
  if(prior)return prior;
  const now=ports.clock.now(),members=i.action==="replace-members"?[...new Set(i.userIds)].sort():[],steps:Effect[]=[
    r.lockPool(id,a.companyId),
    r.guardPoolVersion(key,"pool",id,a.companyId,i.expectedVersion),
  ];
  if(i.action==="replace-members") {
    for(const userId of members)steps.push(r.guardEligibleMember(key,`member-${steps.length}`,a.companyId,userId,now));
    steps.push(r.prepareRevokeRemovedMembers(now,id,JSON.stringify(members)));
    for(const userId of members)steps.push(r.prepareRestorePoolMember(`agent-pool-member-${key}-${await ports.hash(userId)}`,id,userId,a.userId,now));
    steps.push(r.preparePoolHistory(`agent-pool-revision-${key}`,id,i.expectedVersion+1,JSON.stringify(members),i.reason,a.userId,now));
  }
  const status=i.action==="retire"?"retired":"active";
  steps.push(r.preparePoolLifecycle(status,now,id));
  return r.commit(a,key,op,i,id,i.expectedVersion+1,{id,status,version:i.expectedVersion+1},steps);
}
async function changePrivateSession(a:WorkContext,id:string,raw:unknown,key:string) {
  const i=z.object({action:z.enum(["hold","release-hold","expire"]),expectedVersion:version,reason}).parse(raw);
  ports.authorize(a,i.action==="expire"?"records.read":"ai.manage");
  const r = await ports.repository(a),op=`agent.session.${i.action}`,prior=await r.replay(a,key,op,id,i);
  if(prior)return prior;
  const now=ports.clock.now(),manage=hasPermission(a,"ai.manage"),steps:Effect[]=[
    r.lockPrivateSession(id),
    r.guardPrivateSessionControl(key,"private-session-control",id,a.companyId,a.userId,manage?1:0,i.expectedVersion),
  ];
  if(i.action==="expire")steps.push(r.guardActiveSession(key,"active-session",id),r.prepareExpireSession(now,id));
  else steps.push(r.guardRetentionHold(key,"session-hold",id,i.action==="hold"?0:1),r.prepareSessionHold(i.action==="hold"?1:0,i.action==="hold"?i.reason:null,now,id));
  steps.push(r.prepareSessionEvent(`ai-private-session-event-${key}`,id,i.expectedVersion+1,i.action,a.userId,i.reason,now));
  return r.commit(a,key,op,i,id,i.expectedVersion+1,{id,version:i.expectedVersion+1,action:i.action},steps);
}
async function createAgentAssignment(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = assignSchema.parse(raw),
    r = await ports.repository(a),
    id = `agent-assignment-${key}`,
    op = "agent.assignment.create",
    prior = await r.replay<{id:string;status:string;version:number}>(a, key, op, id, i);
  if (prior) return prior;
  const now = ports.clock.now(),
    result = { id, status: "active", version: 1 };
  return r.commit(a, key, op, i, id, 1, result, [
    r.guardApprovedVersion(key,"version",i.agentVersionId,a.companyId),
    ...(i.poolId
      ? [
          r.guardActivePool(key,"pool",i.poolId,a.companyId),
        ]
      : [
          r.guardEligibleAssignee(key,"user",a.companyId,i.userId!,now),
        ]),
    r.prepareAssignment(id,a.companyId,i.agentVersionId,i.poolId ?? null,i.userId ?? null,i.scopePath,a.userId,now),
  ]);
}
async function changeAgentRecord(
  a: WorkContext,
  kind: "agent" | "version" | "assignment",
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const i = actionSchema.parse(raw),
    r = await ports.repository(a),
    op = `agent.${kind}.${i.action}`,
    prior = await r.replay(a, key, op, id, i);
  if(prior)return prior;
  if(kind!=="agent"&&kind!=="version"&&kind!=="assignment" || kind==="assignment" && i.action!=="revoke" || kind!=="assignment" && i.action==="revoke")
    throw new BusinessRuleError(422,"AGENT_ACTION_INVALID","Choose an available lifecycle action for this record.");
  const row = await r.currentRecord(kind,id,a.companyId);
  if (!row)
    throw new BusinessRuleError(
      404,
      "AGENT_RECORD_NOT_FOUND",
      "Agent record not found.",
    );
  const expected =
      i.action === "revoke"
        ? "active"
        : i.action === "retire"
          ? "approved"
          : "draft",
    status =
      i.action === "approve"
        ? "approved"
        : i.action === "reject"
          ? "rejected"
          : i.action === "retire"
            ? "retired"
            : "revoked";
  if (row.status !== expected || Number(row.version) !== i.expectedVersion)
    throw new BusinessRuleError(
      409,
      "CONFLICT",
      "Record changed or transition is invalid.",
    );
  if (row.created_by === a.userId && ["approve", "reject"].includes(i.action))
    throw new BusinessRuleError(409, "CONFLICT", "Independent review is required.");
  const now = ports.clock.now();
  return r.commit(a,
    key,
    op,
    i,
    id,
    i.expectedVersion + 1,
    { id, status, version: i.expectedVersion + 1 },
    [
      r.guardLifecycleVersion(kind,key,"record",id,i.expectedVersion,expected),
      r.prepareRecordLifecycle(kind,status,a.userId,now,id),
    ],
  );
}
async function assistantGovernanceContext(
  a: WorkContext,
  conversationKey: string,
  scopeKey: string,
  path: string,
):Promise<AssistantGovernance> {
const r=await ports.repository(a);
  const policy=await assistantPolicy(a,path),now=ports.clock.now();
  const existing=await r.latestPrivateSession(a.companyId,a.userId,a.assignmentId,scopeKey,conversationKey);
  if(existing?.status==="active"&&Number(existing.expires_at)>now)return {sessionId:existing.id,...policy};
  const generation=Number(existing?.generation??0)+1,id=`ai-session-${await ports.hash(JSON.stringify([a.companyId,a.userId,a.assignmentId,scopeKey,conversationKey,generation]))}`;
  await r.batch([
    r.preparePrivateSession(id,a.companyId,a.userId,a.assignmentId,scopeKey,conversationKey,policy.assignmentId,now,generation),
    r.preparePrivateSessionControls(id,now+30*24*60*60*1000,now),
  ]);
  return {sessionId:id,...policy};
}
async function recordAssistantInvocation(
  a: WorkContext,
  input: {
    sessionId: string;
    agentVersionId: string | null;
    assignmentId?: string | null;
    assignmentVersion?: number | null;
    agentVersionRevision?: number | null;
    catalogVersion?: number | null;
    poolId?: string | null;
    poolVersion?: number | null;
    prompt?: string | null;
    allowedSources?: string[];
    turnId: string;
    question: string;
    sources: unknown;
    provider: unknown;
    outcome: "succeeded" | "failed";
    inputTokens?: number;
    outputTokens?: number;
    error?: string;
  },
) {
const r=await ports.repository(a);
  await r.recordInvocation(`ai-evidence-${input.turnId}`,input.sessionId,input.turnId,a.companyId,a.userId,input.agentVersionId,await ports.hash(input.question),await ports.hash(JSON.stringify(input.sources)),JSON.stringify({
        assignmentId: a.assignmentId,
        companyId: a.companyId,
        branchId: a.branchId,
        departmentId: a.departmentId,
      }),JSON.stringify(input.provider),input.outcome,input.inputTokens ?? null,input.outputTokens ?? null,input.error ?? null,ports.clock.now(),JSON.stringify({assignmentId:input.assignmentId??null,assignmentVersion:input.assignmentVersion??null,agentVersionId:input.agentVersionId,agentVersionRevision:input.agentVersionRevision??null,catalogVersion:input.catalogVersion??null,poolId:input.poolId??null,poolVersion:input.poolVersion??null,promptDigest:input.prompt?await ports.hash(input.prompt):null,allowedSources:input.allowedSources??["/"]}));
}
async function agentEvidenceRecord(a:WorkContext,kind:'agent_invocation'|'agent_session',id:string){
 access(a);const r = await ports.repository(a);
 const row=kind==='agent_invocation'
  ?await r.invocationRecord(a.companyId,id)
  :await r.sessionRecord(a.companyId,id,a.userId,hasPermission(a,'ai.manage')?1:0);
 if(!row)throw new BusinessRuleError(404,'AGENT_RECORD_NOT_FOUND','Agent record not found in this work scope.');
 const effective_status=row.status==='active'&&Number(row.expires_at)<=ports.clock.now()?'expired':String(row.status);
 const sessionData=kind==='agent_session'?{agents:[],versions:[],pools:[],assignments:[],invocations:[],people:[],members:[],poolHistory:[],sessions:[{...row,effective_status}]}:undefined;
 return {row:{...row,name:String(row.turn_id??row.conversation_key??id),...(kind==='agent_session'?{effective_status}:{})},sessionData};
}
return {validateAssistantGovernance,agentWorkspace,createAgent,createAgentVersion,createAgentPool,changeAgentPool,changePrivateSession,createAgentAssignment,changeAgentRecord,assistantGovernanceContext,recordAssistantInvocation,agentEvidenceRecord};}
