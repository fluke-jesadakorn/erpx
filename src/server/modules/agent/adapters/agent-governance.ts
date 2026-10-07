import {agentGovernanceEligiblePeople,agentGovernanceEligibleMember} from "../../identity_access/public/agent-governance-facts";
export {agentSourceAllowed} from "@/server/modules/agent/domain/source-policy";
import "server-only";

import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "@/server/modules/runtime/public/runtime/types";
type Row=Record<string,unknown>;
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";

import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {sha256} from "@/server/modules/runtime/public/crypto";



import {BusinessRuleError} from '../../shared';
import {createAgentGovernanceApplication,createAssistantGovernanceGuards,type AgentGovernanceRepository} from '../application/governance';
import type {AssistantGovernance} from '../domain/governance';
export type {AssistantGovernance} from '../domain/governance';
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
prepareAgent(id:string,companyId:string,name:string,purpose:string,userId:string,now:number){return db
      .prepare("INSERT INTO agent_catalog VALUES(?,?,?,?,'draft',?,NULL,1,?,?)")
      .bind(id, companyId, name, purpose, userId, now, now);},
async nextVersionNumber(agentId:string,companyId:string){return db
    .prepare(
      "SELECT COALESCE(max(v.version_number),0)::int n FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE v.agent_id=? AND a.company_id=?",
    )
    .bind(agentId, companyId)
    .first<{ n: number }>();},
guardAgent(key:string,value:"agent",agentId:string,companyId:string){return assertion(
      db,
      key,
      value,
      "EXISTS(SELECT 1 FROM agent_catalog WHERE id=? AND company_id=? AND status='approved')",
      [agentId, companyId],
    );},
prepareAgentVersion(id:string,agentId:string,number:number,prompt:string,value:string,userId:string,now:number){return db
      .prepare(
        'INSERT INTO agent_versions VALUES(?,?,?,?,?,\'["approve","sign","pay"]\',\'draft\',?,NULL,1,?,?)',
      )
      .bind(
        id,
        agentId,
        number,
        prompt,
        value,
        userId,
        now,
        now,
      );},
preparePool(id:string,companyId:string,name:string,userId:string,now:number){return db
      .prepare("INSERT INTO agent_pools VALUES(?,?,?,'active',?,1,?,?)")
      .bind(id, companyId, name, userId, now, now);},
guardEligibleMember(key:string,value:string,companyId:string,userId:string,now:number){return agentGovernanceEligibleMember(db,key,value,companyId,userId,now);},
preparePoolMember(value:string,id:string,userId:string,userId2:string,now:number){return db
        .prepare("INSERT INTO agent_pool_members(id,pool_id,user_id,created_by,created_at) VALUES(?,?,?,?,?)")
        .bind(
          value,
          id,
          userId,
          userId2,
          now,
        );},
prepareInitialPoolHistory(value:string,id:string,value2:string,value3:"Initial eligible routing members",userId:string,now:number){return db.prepare("INSERT INTO agent_pool_revisions VALUES(?,?,1,?,?,?,?)").bind(value,id,value2,value3,userId,now);},
lockPool(id:string,companyId:string){return db.prepare("SELECT id FROM agent_pools WHERE id=? AND company_id=? FOR UPDATE").bind(id,companyId);},
guardPoolVersion(key:string,value:"pool",id:string,companyId:string,expectedVersion:number){return assertion(db,key,value,"EXISTS(SELECT 1 FROM agent_pools WHERE id=? AND company_id=? AND version=? AND status='active')",[id,companyId,expectedVersion]);},

prepareRevokeRemovedMembers(now:number,id:string,value:string){return db.prepare("UPDATE agent_pool_members SET revoked_at=?,version=version+1 WHERE pool_id=? AND revoked_at IS NULL AND user_id NOT IN (SELECT jsonb_array_elements_text(?::jsonb))").bind(now,id,value);},
prepareRestorePoolMember(value:string,id:string,userId:string,userId2:string,now:number){return db.prepare("INSERT INTO agent_pool_members(id,pool_id,user_id,created_by,created_at) VALUES(?,?,?,?,?) ON CONFLICT(pool_id,user_id) DO UPDATE SET revoked_at=NULL,version=agent_pool_members.version+1 WHERE agent_pool_members.revoked_at IS NOT NULL")
      .bind(value,id,userId,userId2,now);},
preparePoolHistory(value:string,id:string,value2:number,value3:string,reason:string,userId:string,now:number){return db.prepare("INSERT INTO agent_pool_revisions VALUES(?,?,?,?,?,?,?)").bind(value,id,value2,value3,reason,userId,now);},
preparePoolLifecycle(status:"retired" | "active",now:number,id:string){return db.prepare("UPDATE agent_pools SET status=?,version=version+1,updated_at=? WHERE id=?").bind(status,now,id);},
lockPrivateSession(id:string){return db.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR UPDATE").bind(id);},
guardPrivateSessionControl(key:string,value:"private-session-control",id:string,companyId:string,userId:string,value2:0 | 1,expectedVersion:number){return assertion(db,key,value,"EXISTS(SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.id=? AND s.company_id=? AND (s.user_id=? OR ?=1) AND c.version=?)",[id,companyId,userId,value2,expectedVersion]);},
guardActiveSession(key:string,value:"active-session",id:string){return assertion(db,key,value,"EXISTS(SELECT 1 FROM ai_private_session_controls WHERE session_id=? AND status='active')",[id]);},
prepareExpireSession(now:number,id:string){return db.prepare("UPDATE ai_private_session_controls SET status='expired',version=version+1,updated_at=? WHERE session_id=?").bind(now,id);},
guardRetentionHold(key:string,value:"session-hold",id:string,value2:0 | 1){return assertion(db,key,value,"EXISTS(SELECT 1 FROM ai_private_session_controls WHERE session_id=? AND legal_hold=?)",[id,value2]);},
prepareSessionHold(value:0 | 1,value2:string | null,now:number,id:string){return db.prepare("UPDATE ai_private_session_controls SET legal_hold=?,hold_reason=?,version=version+1,updated_at=? WHERE session_id=?").bind(value,value2,now,id);},
prepareSessionEvent(value:string,id:string,value2:number,action:"hold" | "release-hold" | "expire",userId:string,reason:string,now:number){return db.prepare("INSERT INTO ai_private_session_events VALUES(?,?,?,?,?,?,?)").bind(value,id,value2,action,userId,reason,now);},
guardApprovedVersion(key:string,value:"version",agentVersionId:string,companyId:string){return assertion(
      db,
      key,
      value,
      "EXISTS(SELECT 1 FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE v.id=? AND v.status='approved' AND a.status='approved' AND a.company_id=?)",
      [agentVersionId, companyId],
    );},
guardActivePool(key:string,value:"pool",poolId:string,companyId:string){return assertion(
            db,
            key,
            value,
            "EXISTS(SELECT 1 FROM agent_pools WHERE id=? AND company_id=? AND status='active')",
            [poolId, companyId],
          );},
guardEligibleAssignee(key:string,value:"user",companyId:string,value2:string,now:number){return agentGovernanceEligibleMember(db,key,value,companyId,value2,now);},
prepareAssignment(id:string,companyId:string,agentVersionId:string,value:string | null,value2:string | null,scopePath:string,userId:string,now:number){return db
      .prepare(
        "INSERT INTO agent_assignments VALUES(?,?,?,?,?,?,'active',?,NULL,1,?,?)",
      )
      .bind(
        id,
        companyId,
        agentVersionId,
        value,
        value2,
        scopePath,
        userId,
        now,
        now,
      );},
async currentRecord(kind:"agent"|"version"|"assignment",id:string,companyId:string){const table=kind==='agent'?'agent_catalog':kind==='version'?'agent_versions':'agent_assignments';return db
      .prepare(
        `SELECT x.* FROM ${table} x ${kind === "version" ? "JOIN agent_catalog a ON a.id=x.agent_id" : ""} WHERE x.id=? AND ${kind === "version" ? "a" : "x"}.company_id=?`,
      )
      .bind(id, companyId)
      .first<Row>();},
guardLifecycleVersion(kind:"agent"|"version"|"assignment",key:string,value:"record",id:string,expectedVersion:number,expected:"active" | "approved" | "draft"){const table=kind==='agent'?'agent_catalog':kind==='version'?'agent_versions':'agent_assignments';return assertion(
        db,
        key,
        value,
        `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status=?)`,
        [id, expectedVersion, expected],
      );},
prepareRecordLifecycle(kind:"agent"|"version"|"assignment",status:"retired" | "approved" | "rejected" | "revoked",userId:string,now:number,id:string){const table=kind==='agent'?'agent_catalog':kind==='version'?'agent_versions':'agent_assignments',review=kind==='assignment'?'revoked_by':'reviewed_by';return db
        .prepare(
          `UPDATE ${table} SET status=?,${review}=?,version=version+1,updated_at=? WHERE id=?`,
        )
        .bind(status, userId, now, id);},
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
async sessionRecord(companyId:string,id:string,userId:string,value:0 | 1){return db.prepare('SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND s.id=? AND (s.user_id=? OR ?)').bind(companyId,id,userId,value).first<Row>();},replay:(...args)=>replayCommand(db,...args),commit:(...args)=>executeCommand(db,...args),async batch(effects){await db.batch(effects);}};}
function application(db?:SqlDatabase){const repositories=new Map<string,Promise<AgentGovernanceRepository<SqlStatement>>>();return createAgentGovernanceApplication({repository:async actor=>{if(db)return bindRepository(db);if(!repositories.has(actor.holdingId))repositories.set(actor.holdingId,holdingResources(actor.holdingId).then(resources=>bindRepository(resources.write)));return repositories.get(actor.holdingId)!;},clock:{now:()=>Date.now()},hash:sha256,authorize:requirePermission});}
export async function validateAssistantGovernance(db:SqlDatabase,a:WorkContext,path:string,governance:AssistantGovernance){return exposed(()=>application(db).validateAssistantGovernance(a,path,governance));}
export async function agentWorkspace(a: WorkContext){return exposed(()=>application().agentWorkspace(a));}
export async function createAgent(a: WorkContext,raw: unknown,key: string){return exposed(()=>application().createAgent(a,raw,key));}
export async function createAgentVersion(a: WorkContext,raw: unknown,key: string){return exposed(()=>application().createAgentVersion(a,raw,key));}
export async function createAgentPool(a: WorkContext,raw: unknown,key: string){return exposed(()=>application().createAgentPool(a,raw,key));}
export async function changeAgentPool(a:WorkContext,id:string,raw:unknown,key:string){return exposed(()=>application().changeAgentPool(a,id,raw,key));}
export async function changePrivateSession(a:WorkContext,id:string,raw:unknown,key:string){return exposed(()=>application().changePrivateSession(a,id,raw,key));}
export async function createAgentAssignment(a: WorkContext,raw: unknown,key: string){return exposed(()=>application().createAgentAssignment(a,raw,key));}
export async function changeAgentRecord(a: WorkContext,kind: "agent" | "version" | "assignment",id: string,raw: unknown,key: string){return exposed(()=>application().changeAgentRecord(a,kind,id,raw,key));}
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
