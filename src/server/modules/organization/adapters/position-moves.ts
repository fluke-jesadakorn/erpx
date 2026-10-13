import { platformOwnerProjectionSources as identity_accessPlatformSources } from "../../identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import "server-only";
import {createHash} from 'node:crypto';
import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from '../../identity_access/public/authorization';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {readGpoRules} from '../../identity_access/public/gpo-rules';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import {ApiError} from '../../runtime/public/http';
import {assertion,executeCommand,replayCommand} from '../../workflow/public/commands';
import {appendOrganizationPositionMoveEvents} from '../../workflow/public/organization-position-move-events';
import {applyOrganizationPositionMoveHolder,organizationPositionMoveMembers,organizationPositionMovePlacements,organizationPositionMoveLeave} from '../../people_payroll/public/organization-position-moves';
import {BusinessRuleError} from '../../shared';
import {organizationRoster} from './organization';
import {recommendationStatements} from './staff-defaults';
import {positionMoveDate,type PositionMoveInput,type PositionMoveRecord} from "@/lib/people_payroll/position-moves";
import {createPositionMoveApplication,type PositionMovePorts,type ClaimedPositionMove} from '../application/position-moves';
import type {LeaveDefault} from '../domain/position-moves';
export {positionMoveGraphIssues,positionMoveLeaveImpact} from '../domain/position-moves';
function rules<T>(call:()=>T):T{try{return call();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function asyncRules<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
function portsFor(db:SqlDatabase):PositionMovePorts{return {clock:{now:()=>Date.now()},hash:value=>createHash('sha256').update(JSON.stringify(value)).digest('hex'),authorize:(actor,permission)=>{rules(()=>requirePermission(actor,permission));},canReadPeople:actor=>hasPermission(actor,'people.manage'),transaction:run=>(db as PostgresDatabase).transaction(tx=>run(portsFor(tx))),roster:organizationCompanyId=>organizationRoster(db,organizationCompanyId),
nodes:async(companyId)=>(await db.prepare("SELECT id,kind,branch_id,status,hierarchy_revision FROM organization_nodes WHERE company_id=? ORDER BY id").bind(companyId).all<{ id: string; kind: string; branch_id: string | null; status: string; hierarchy_revision: number }>()).results,
levels:async(companyId)=>(await db.prepare("SELECT id,level_number,active,version FROM staff_levels WHERE company_id=? ORDER BY id").bind(companyId).all<{ id: string; level_number: number; active: number; version: number }>()).results,
pending:async(companyId,moveId,positionId)=>!!(await db.prepare(`SELECT id FROM organization_position_moves WHERE company_id=? AND status='scheduled' AND id IS DISTINCT FROM ? AND
    (position_id=? OR EXISTS(SELECT 1 FROM organization_position_move_members h JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=h.person_id WHERE h.move_id=organization_position_moves.id AND e.company_id=? AND e.position_id=? AND e.status!='ended')) LIMIT 1`)
    .bind(companyId, moveId, positionId, companyId, positionId).first()),
defaults:async(companyId)=>(await db.prepare(`SELECT DISTINCT ON(d.level_id) d.level_id,d.policy_version_id,d.version,p.effective_from FROM level_leave_defaults d JOIN (${people_payrollPlatformSources.leave_policy_versions}) p ON p.id=d.policy_version_id WHERE d.company_id=? ORDER BY d.level_id,d.version DESC`).bind(companyId).all<LeaveDefault>()).results,
members:(companyId,positionId)=>organizationPositionMoveMembers(db,companyId,positionId),
openPlacements:(companyId,positionId)=>organizationPositionMovePlacements(db,companyId,positionId),
leave:(companyId,positionId,date)=>organizationPositionMoveLeave(db,companyId,positionId,date),
rules:companyId=>readGpoRules(db,companyId),
rows:async(actor,id)=>(await db.prepare(`SELECT m.id,m.company_id,m.position_id,m.status,m.version,m.effective_at,m.definition,m.preview,m.created_by,m.created_at,m.updated_at,m.applied_at,m.last_error
    FROM organization_position_moves m JOIN organization_positions p ON p.company_id=m.company_id AND p.id=m.position_id
    WHERE m.company_id=? AND (?::text IS NULL OR m.id=?) AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY m.created_at DESC,m.id LIMIT 100`)
    .bind(actor.companyId, id ?? null, id ?? null, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<PositionMoveRecord>()).results,
replay:(actor,key,operation,id,payload)=>replayCommand(db,actor,key,operation,id,payload),schedule:async(actor,input,key,payload,prepared,now)=>{const id=`position-move-${key}`,operation='organization.position-move.submit',result={id,version:1,status:'scheduled' as const,effectiveAt:input.effectiveAt},savedActor={...actor,authority:undefined,availableAssignments:undefined,assignments:[],workspaces:[]};
    return executeCommand(db, actor, key, operation, payload, id, 1, result, [
      db.prepare("INSERT INTO organization_position_moves(id,company_id,position_id,status,effective_at,definition,preview,snapshot_hash,authorizing_assignment_id,actor_json,created_by,created_at,updated_at) VALUES(?,?,?,'scheduled',?,?,?,?,?,?,?,?,?)")
        .bind(id, actor.companyId, input.positionId, input.effectiveAt, JSON.stringify(input), JSON.stringify(prepared.preview), prepared.snapshotHash, actor.assignmentId, JSON.stringify(savedActor), actor.userId, now, now),
      ...prepared.members.map(member => db.prepare("INSERT INTO organization_position_move_members(move_id,engagement_id,person_id) VALUES(?,?,?)").bind(id, member.id, member.person_id)),
    ]);
 },cancel:async(actor,id,expectedVersion,key,now)=>{const version=expectedVersion+1;
    return executeCommand(db, actor, key, "organization.position-move.cancel", { expectedVersion }, id, version, { id, version, status: "cancelled" }, [
      assertion(db, key, "move-version", "EXISTS(SELECT 1 FROM organization_position_moves WHERE id=? AND company_id=? AND status='scheduled' AND version=? AND effective_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)", [id, actor.companyId, expectedVersion]),
      db.prepare("UPDATE organization_position_moves SET status='cancelled',version=?,updated_at=? WHERE id=? AND company_id=?").bind(version, now, id, actor.companyId),
    ]);
 },claim:async(now)=>{
        const providerIdle = await db.prepare("SELECT pg_try_advisory_xact_lock(821039320) acquired").first<{ acquired: boolean }>();
        if (!providerIdle?.acquired) return null;
        const row = await db.prepare("SELECT * FROM organization_position_moves WHERE status='scheduled' AND effective_at<=? ORDER BY effective_at,id FOR UPDATE SKIP LOCKED LIMIT 1").bind(now)
          .first<ClaimedPositionMove>();
        if (!row) return null;
        return row;

 },assignmentValid:async(row,actor,affectedCount,now)=>!!(await db.prepare(`SELECT 1 FROM (${identity_accessPlatformSources.assignments}) a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
          AND iam_role_allows(a.role,a.company_id,'organization.manage') AND (?=0 OR iam_role_allows(a.role,a.company_id,'people.manage'))
          AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?))`)
          .bind(row.authorizing_assignment_id, row.company_id, row.created_by, actor.role, now, now, affectedCount, actor.branchId, actor.departmentId).first()),setPayrollDate:async(date)=>{await db.prepare("SELECT set_config('erpx.payroll_assignment_date',?,true)").bind(date).first();},applyPosition:async(companyId,candidate,now)=>{await db.prepare('UPDATE organization_positions SET branch_id=?,department_id=?,level_id=?,reports_to_position_id=?,version=version+1,updated_at=? WHERE company_id=? AND id=?').bind(candidate.branch_id,candidate.department_id,candidate.level_id,candidate.reports_to_position_id,now,companyId,candidate.id).run();},applyHolder:(row,input,holder,original,now)=>applyOrganizationPositionMoveHolder(db,row,input,holder,original,now),recommendLeave:(actor,row,input,candidate,holder,original)=>asyncRules(async()=>{await db.batch(await recommendationStatements(db,actor,{engagementId:holder.engagementId,positionId:candidate.id,levelId:holder.after.levelId,priorLevelId:holder.before.levelId,startDate:positionMoveDate(row.effective_at,input.timezone),currency:original.currency,acceptPay:false,acceptLeave:true,commandId:`${row.id}-${holder.engagementId}`}));}),complete:async(row,input,affectedCount,now)=>{
        await db.prepare("UPDATE organization_position_moves SET status='applied',version=version+1,applied_at=?,updated_at=?,last_error=NULL WHERE id=?").bind(now, now, row.id).run();
 await appendOrganizationPositionMoveEvents(db,row,input,affectedCount,now);

 },needsReview:async(id,message,now)=>{await db.prepare("UPDATE organization_position_moves SET status='needs_review',version=version+1,last_error=?,updated_at=? WHERE id=? AND status='scheduled'").bind(message,now,id).run();}};}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const previewPositionMove=(db:PostgresDatabase,actor:WorkContext,input:PositionMoveInput)=>invoke(()=>createPositionMoveApplication(portsFor(db)).preview(actor,input));
export const submitPositionMove=(db:PostgresDatabase,actor:WorkContext,input:PositionMoveInput,previewToken:string,expiresAt:number,key:string)=>invoke(()=>createPositionMoveApplication(portsFor(db)).submit(actor,input,previewToken,expiresAt,key));
export const positionMoves=(db:SqlDatabase,actor:WorkContext,id?:string)=>invoke(()=>createPositionMoveApplication(portsFor(db)).rows(actor,id));
export const cancelPositionMove=(db:PostgresDatabase,actor:WorkContext,id:string,expectedVersion:number,key:string)=>invoke(()=>createPositionMoveApplication(portsFor(db)).cancel(actor,id,expectedVersion,key));
export const drainPositionMoves=(db:PostgresDatabase,now=Date.now())=>invoke(()=>createPositionMoveApplication(portsFor(db)).drain(now));
