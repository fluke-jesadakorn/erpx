export {agentSourceAllowed} from "@erpx/server-agent/domain/source-policy";
import "server-only";

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
type Row=Record<string,unknown>;
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";

import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {sha256} from "@erpx/server-runtime/public/crypto";



import {BusinessRuleError} from '@erpx/server-shared/index';
import {nativeOwnerWrite,nativeOwnerWriteNoopSql} from './native-owner-write';
import {agentSelectColsFromSql} from './read-tail-r26-source-sql';
import {createAgentGovernanceApplication,createAssistantGovernanceGuards,type AgentGovernanceRepository} from '@erpx/server-agent/application/governance';
import type {AssistantGovernance} from '@erpx/server-agent/domain/governance';
export type {AssistantGovernance} from '@erpx/server-agent/domain/governance';
/** Byte-identical Agent pool-member access EXISTS shared by policyCandidates and guardDefaultPolicy.
 * Each call site keeps its own binds; the native policy owner remains the authority. */
const agentPoolMemberAccessSql = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)';
/** The guardApprovedPolicy witness correlates the same pool-member head to agent_pools
 * (p.id) instead of agent_assignments (x.pool_id). Derived from the one authored head so
 * the two witnesses cannot drift; both keep their own binds. */
const agentPoolMemberAccessByPoolSql = agentPoolMemberAccessSql.replace('x.pool_id', 'p.id');
/** Parameterized Agent active-private-session witness shared by activePrivateSession (bound expiry)
 * and guardPrivateSession (DB-clock expiry). The expiry expression is the only variation; both call
 * sites keep their own binds; the native session owner remains the authority. */
const agentActivePrivateSessionSql = (expiresAt: string) =>
  `SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>${expiresAt}`;
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function bindRepository(db:SqlDatabase):AgentGovernanceRepository<SqlStatement>{return {async policyCandidates(companyId:string,userId:string){return (await db.prepare(
    `SELECT x.id,x.version assignment_version,x.scope_path,x.pool_id,x.agent_version_id,
      v.version version_revision,v.prompt,v.allowed_sources_json,v.status version_status,
      c.version catalog_version,c.status catalog_status,p.version pool_version,p.status pool_status
     FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id
     JOIN agent_catalog c ON c.id=v.agent_id LEFT JOIN agent_pools p ON p.id=x.pool_id
     WHERE x.company_id=? AND c.company_id=x.company_id AND x.status='active'
       AND (x.user_id=? OR ${agentPoolMemberAccessSql})
     ORDER BY length(x.scope_path) DESC,x.created_at DESC,x.id`,
  ).bind(companyId,userId,userId).all<Row>()).results;},
async activePrivateSession(sessionId:string,companyId:string,userId:string,assignmentId:string,now:number){return db.prepare(agentActivePrivateSessionSql('?'))
    .bind(sessionId,companyId,userId,assignmentId,now).first();},
lockSession(sessionId:string){return db.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE").bind(sessionId);},
guardPrivateSession(key:string,value:"private-session",sessionId:string,companyId:string,userId:string,assignmentId:string){return assertion(db,key,value,`EXISTS(${agentActivePrivateSessionSql('floor(extract(epoch FROM clock_timestamp())*1000)::bigint')})`,[sessionId,companyId,userId,assignmentId]);},
guardDefaultPolicy(key:string,value:"agent-default",companyId:string,path:string,userId:string){return assertion(db,key,value,`NOT EXISTS(SELECT 1 FROM agent_assignments x WHERE x.company_id=? AND x.status='active'
    AND (x.scope_path='/' OR ?=x.scope_path OR starts_with(?,x.scope_path||'/') OR starts_with(?,x.scope_path||'?'))
    AND (x.user_id=? OR ${agentPoolMemberAccessSql}))`,[companyId,path,path,path,userId,userId]);},
lockApprovedAssignment(assignmentId:string){return db.prepare("SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c").bind(assignmentId);},
lockAssignedPool(poolId:string){return db.prepare("SELECT id FROM agent_pools WHERE id=? FOR SHARE").bind(poolId);},
guardApprovedPolicy(key:string,value:"approved-agent",assignmentId:string,companyId:string,assignmentVersion:number | null,agentVersionId:string | null,agentVersionRevision:number | null,catalogVersion:number | null,userId:string,poolId:string | null,poolVersion:number | null){return assertion(db,key,value,`EXISTS(SELECT 1 FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id LEFT JOIN agent_pools p ON p.id=x.pool_id
      WHERE x.id=? AND x.company_id=? AND c.company_id=x.company_id AND x.version=? AND x.status='active'
      AND v.id=? AND v.version=? AND v.status='approved' AND c.version=? AND c.status='approved'
      AND (x.user_id=? OR p.id=? AND p.status='active' AND p.version=? AND ${agentPoolMemberAccessByPoolSql}))`,
    [assignmentId,companyId,assignmentVersion,agentVersionId,agentVersionRevision,catalogVersion,userId,poolId,poolVersion,userId]);},
/** Workspace and evidence-record reads now live in the owner-native Rust port;
 * this repository no longer authors their SQL. */
async latestPrivateSession(companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string){return db.prepare(agentSelectColsFromSql('s.id,s.generation,c.status,c.expires_at','ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND s.user_id=? AND s.assignment_id=? AND s.scope_key=? AND s.conversation_key=? ORDER BY s.generation DESC LIMIT 1'))
    .bind(companyId,userId,assignmentId,scopeKey,conversationKey).first<{id:string;generation:number;status:string;expires_at:number}>();},
async preparePrivateSession(id:string,companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string,assignmentId2:string | null,now:number,generation:number){return nativeOwnerWrite('ag_private_session_insert',[id,companyId,userId,assignmentId,scopeKey,conversationKey,assignmentId2,now,now,generation],`ag-session:${id}`,{companyId,now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async preparePrivateSessionControls(id:string,value:number,now:number){return nativeOwnerWrite('ag_session_controls_insert',[id,value,now,now],`ag-session-controls:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async recordInvocation(value:string,sessionId:string,turnId:string,companyId:string,userId:string,agentVersionId:string | null,value2:string,value3:string,value4:string,value5:string,outcome:"succeeded" | "failed",value6:number | null,value7:number | null,value8:string | null,now:number,value9:string){
    await nativeOwnerWrite('ag_invocation_evidence_insert',[
      value,sessionId,turnId,companyId,userId,agentVersionId,value2,value3,value4,value5,
      outcome,value6,value7,value8,now,value9,
    ],`ag-invocation:${turnId}`,{companyId,now},()=>db.prepare(nativeOwnerWriteNoopSql));
  },
async batch(effects){await db.batch(effects);}};}
function application(db?:SqlDatabase){const repositories=new Map<string,Promise<AgentGovernanceRepository<SqlStatement>>>();return createAgentGovernanceApplication({repository:async actor=>{if(db)return bindRepository(db);if(!repositories.has(actor.holdingId))repositories.set(actor.holdingId,holdingResources(actor.holdingId).then(resources=>bindRepository(resources.write)));return repositories.get(actor.holdingId)!;},clock:{now:()=>Date.now()},hash:sha256,authorize:requirePermission});}
export async function validateAssistantGovernance(db:SqlDatabase,a:WorkContext,path:string,governance:AssistantGovernance){return exposed(()=>application(db).validateAssistantGovernance(a,path,governance));}
export async function assistantGovernanceContext(db: SqlDatabase,a: WorkContext,conversationKey: string,scopeKey: string,path: string){return exposed(()=>application(db).assistantGovernanceContext(a,conversationKey,scopeKey,path));}
export async function recordAssistantInvocation(db: SqlDatabase,a: WorkContext,input: {
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
  }){return exposed(()=>application(db).recordAssistantInvocation(a,input));}
export function assistantGovernanceGuards(db:SqlDatabase,a:WorkContext,key:string,path:string,governance:AssistantGovernance):SqlStatement[]{return createAssistantGovernanceGuards(bindRepository(db))(a,key,path,governance);}
