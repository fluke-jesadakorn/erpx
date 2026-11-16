import {agentGovernanceEligiblePeople} from "@erpx/server-identity_access/public/agent-governance-facts";
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
import {createAgentGovernanceApplication,createAssistantGovernanceGuards,type AgentGovernanceRepository} from '@erpx/server-agent/application/governance';
import type {AssistantGovernance} from '@erpx/server-agent/domain/governance';
export type {AssistantGovernance} from '@erpx/server-agent/domain/governance';
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function bindRepository(db:SqlDatabase):AgentGovernanceRepository<SqlStatement>{return {async policyCandidates(companyId:string,userId:string){return (await db.prepare(
    `SELECT x.id,x.version assignment_version,x.scope_path,x.pool_id,x.agent_version_id,
      v.version version_revision,v.prompt,v.allowed_sources_json,v.status version_status,
      c.version catalog_version,c.status catalog_status,p.version pool_version,p.status pool_status
     FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id
     JOIN agent_catalog c ON c.id=v.agent_id LEFT JOIN agent_pools p ON p.id=x.pool_id
     WHERE x.company_id=? AND c.company_id=x.company_id AND x.status='active'
       AND (x.user_id=? OR EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL))
     ORDER BY length(x.scope_path) DESC,x.created_at DESC,x.id`,
  ).bind(companyId,userId,userId).all<Row>()).results;},
async activePrivateSession(sessionId:string,companyId:string,userId:string,assignmentId:string,now:number){return db.prepare("SELECT 1 FROM ai_private_session_controls c JOIN ai_private_sessions s ON s.id=c.session_id WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>?")
    .bind(sessionId,companyId,userId,assignmentId,now).first();},
lockSession(sessionId:string){return db.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE").bind(sessionId);},
guardPrivateSession(key:string,value:"private-session",sessionId:string,companyId:string,userId:string,assignmentId:string){return assertion(db,key,value,`EXISTS(SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id
      WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,[sessionId,companyId,userId,assignmentId]);},
guardDefaultPolicy(key:string,value:"agent-default",companyId:string,path:string,userId:string){return assertion(db,key,value,`NOT EXISTS(SELECT 1 FROM agent_assignments x WHERE x.company_id=? AND x.status='active'
    AND (x.scope_path='/' OR ?=x.scope_path OR starts_with(?,x.scope_path||'/') OR starts_with(?,x.scope_path||'?'))
    AND (x.user_id=? OR EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)))`,[companyId,path,path,path,userId,userId]);},
lockApprovedAssignment(assignmentId:string){return db.prepare("SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c").bind(assignmentId);},
lockAssignedPool(poolId:string){return db.prepare("SELECT id FROM agent_pools WHERE id=? FOR SHARE").bind(poolId);},
guardApprovedPolicy(key:string,value:"approved-agent",assignmentId:string,companyId:string,assignmentVersion:number | null,agentVersionId:string | null,agentVersionRevision:number | null,catalogVersion:number | null,userId:string,poolId:string | null,poolVersion:number | null){return assertion(db,key,value,`EXISTS(SELECT 1 FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id LEFT JOIN agent_pools p ON p.id=x.pool_id
      WHERE x.id=? AND x.company_id=? AND c.company_id=x.company_id AND x.version=? AND x.status='active'
      AND v.id=? AND v.version=? AND v.status='approved' AND c.version=? AND c.status='approved'
      AND (x.user_id=? OR p.id=? AND p.status='active' AND p.version=? AND EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=p.id AND m.user_id=? AND m.revoked_at IS NULL)))`,
    [assignmentId,companyId,assignmentVersion,agentVersionId,agentVersionRevision,catalogVersion,userId,poolId,poolVersion,userId]);},
async workspaceAgents(companyId:string){return (await db
          .prepare(
            "SELECT * FROM agent_catalog WHERE company_id=? ORDER BY updated_at DESC",
          )
          .bind(companyId)
          .all<Row>()).results;},
async workspaceVersions(companyId:string){return (await db
          .prepare(
            "SELECT v.*,a.name agent_name FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE a.company_id=? ORDER BY v.created_at DESC",
          )
          .bind(companyId)
          .all<Row>()).results;},
async workspacePools(companyId:string){return (await db
          .prepare("SELECT * FROM agent_pools WHERE company_id=? ORDER BY name")
          .bind(companyId)
          .all<Row>()).results;},
async workspaceAssignments(companyId:string){return (await db
          .prepare(
            "SELECT x.*,v.version_number,a.name agent_name FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog a ON a.id=v.agent_id WHERE x.company_id=? ORDER BY x.updated_at DESC",
          )
          .bind(companyId)
          .all<Row>()).results;},
async workspaceInvocations(companyId:string){return (await db
          .prepare(
            "SELECT * FROM ai_invocation_evidence WHERE company_id=? ORDER BY created_at DESC LIMIT 50",
          )
          .bind(companyId)
          .all<Row>()).results;},
async workspacePeople(companyId:string,now:number){return agentGovernanceEligiblePeople(db,companyId,now);},
async workspaceMembers(companyId:string){return (await db.prepare("SELECT m.* FROM agent_pool_members m JOIN agent_pools p ON p.id=m.pool_id WHERE p.company_id=? ORDER BY m.pool_id,m.user_id").bind(companyId).all<Row>()).results;},
async workspacePoolHistory(companyId:string){return (await db.prepare("SELECT r.* FROM agent_pool_revisions r JOIN agent_pools p ON p.id=r.pool_id WHERE p.company_id=? ORDER BY r.created_at DESC").bind(companyId).all<Row>()).results;},
async workspaceSessions(companyId:string,userId:string,value:0 | 1){return (await db.prepare("SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND (s.user_id=? OR ?) ORDER BY c.updated_at DESC LIMIT 100").bind(companyId,userId,value).all<Row>()).results;},
async latestPrivateSession(companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string){return db.prepare("SELECT s.id,s.generation,c.status,c.expires_at FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND s.user_id=? AND s.assignment_id=? AND s.scope_key=? AND s.conversation_key=? ORDER BY s.generation DESC LIMIT 1")
    .bind(companyId,userId,assignmentId,scopeKey,conversationKey).first<{id:string;generation:number;status:string;expires_at:number}>();},
preparePrivateSession(id:string,companyId:string,userId:string,assignmentId:string,scopeKey:string,conversationKey:string,assignmentId2:string | null,now:number,generation:number){return db.prepare("INSERT INTO ai_private_sessions(id,company_id,user_id,assignment_id,scope_key,conversation_key,agent_assignment_id,created_at,last_used_at,generation) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,user_id,assignment_id,scope_key,conversation_key,generation) DO NOTHING")
      .bind(id,companyId,userId,assignmentId,scopeKey,conversationKey,assignmentId2,now,now,generation);},
preparePrivateSessionControls(id:string,value:number,now:number){return db.prepare("INSERT INTO ai_private_session_controls(session_id,expires_at,last_used_at,updated_at) VALUES(?,?,?,?) ON CONFLICT(session_id) DO NOTHING").bind(id,value,now,now);},
async recordInvocation(value:string,sessionId:string,turnId:string,companyId:string,userId:string,agentVersionId:string | null,value2:string,value3:string,value4:string,value5:string,outcome:"succeeded" | "failed",value6:number | null,value7:number | null,value8:string | null,now:number,value9:string){await db
    .prepare(
      "INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at,governance_snapshot_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(turn_id) DO NOTHING",
    )
    .bind(
      value,
      sessionId,
      turnId,
      companyId,
      userId,
      agentVersionId,
      value2,
      value3,
      value4,
      value5,
      outcome,
      value6,
      value7,
      value8,
      now,
      value9,
    )
    .run();},
async invocationRecord(companyId:string,id:string){return db.prepare('SELECT id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,outcome,input_tokens,output_tokens,error,created_at FROM ai_invocation_evidence WHERE company_id=? AND id=?').bind(companyId,id).first<Row>();},
async sessionRecord(companyId:string,id:string,userId:string,value:0 | 1){return db.prepare('SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND s.id=? AND (s.user_id=? OR ?)').bind(companyId,id,userId,value).first<Row>();},async batch(effects){await db.batch(effects);}};}
function application(db?:SqlDatabase){const repositories=new Map<string,Promise<AgentGovernanceRepository<SqlStatement>>>();return createAgentGovernanceApplication({repository:async actor=>{if(db)return bindRepository(db);if(!repositories.has(actor.holdingId))repositories.set(actor.holdingId,holdingResources(actor.holdingId).then(resources=>bindRepository(resources.write)));return repositories.get(actor.holdingId)!;},clock:{now:()=>Date.now()},hash:sha256,authorize:requirePermission});}
export async function validateAssistantGovernance(db:SqlDatabase,a:WorkContext,path:string,governance:AssistantGovernance){return exposed(()=>application(db).validateAssistantGovernance(a,path,governance));}
export async function agentWorkspace(a: WorkContext, includeEvidence = true){return exposed(()=>application().agentWorkspace(a, includeEvidence));}
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
export async function agentEvidenceRecord(a:WorkContext,kind:'agent_invocation'|'agent_session',id:string){return exposed(()=>application().agentEvidenceRecord(a,kind,id));}
export function assistantGovernanceGuards(db:SqlDatabase,a:WorkContext,key:string,path:string,governance:AssistantGovernance):SqlStatement[]{return createAssistantGovernanceGuards(bindRepository(db))(a,key,path,governance);}
