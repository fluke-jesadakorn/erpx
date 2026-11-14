import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
type Row=Record<string,unknown>;
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';
import {agentSourceAllowed} from '@erpx/server-agent/domain/source-policy';
import type {AssistantGovernance} from '@erpx/server-agent/domain/governance';
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
 latestPrivateSession(companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string):Promise<{id:string;generation:number;status:string;expires_at:number}|null>;
 preparePrivateSession(id:string,companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string,assignmentId2:string | null,now:number,generation:number):Effect;
 preparePrivateSessionControls(id:string,value:number,now:number):Effect;
 recordInvocation(value:string,sessionId:string,turnId:string,companyId:string,userId:string,agentVersionId:string | null,value2:string,value3:string,value4:string,value5:string,outcome:"succeeded" | "failed",value6:number | null,value7:number | null,value8:string | null,now:number,value9:string):Promise<void>;
 invocationRecord(companyId:string,id:string):Promise<Row|null>;
 sessionRecord(companyId:string,id:string,userId:string,value:0 | 1):Promise<Row|null>;
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
async function agentWorkspace(a: WorkContext, includeEvidence = true) {
  access(a);
  const r = await ports.repository(a),
    [agents, versions, pools, assignments, people, members, poolHistory] =
      await Promise.all([
        r.workspaceAgents(a.companyId),
        r.workspaceVersions(a.companyId),
        r.workspacePools(a.companyId),
        r.workspaceAssignments(a.companyId),
        r.workspacePeople(a.companyId,ports.clock.now()),
        r.workspaceMembers(a.companyId),
        r.workspacePoolHistory(a.companyId),
      ]);
  const [invocations, sessions]: [Row[], Row[]] = includeEvidence
    ? await Promise.all([
        r.workspaceInvocations(a.companyId),
        r.workspaceSessions(a.companyId,a.userId,hasPermission(a,"ai.manage")?1:0),
      ])
    : [[], []];
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
return {validateAssistantGovernance,agentWorkspace,assistantGovernanceContext,recordAssistantInvocation,agentEvidenceRecord};}
