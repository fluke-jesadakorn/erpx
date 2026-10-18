import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {loadIamAuthority} from '../../identity_access/public/iam-store';
import {assignmentReadSql} from '../../identity_access/public/record-source-authority';
import {readPurchasedAccesses} from '../../commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import {workerStartupSourcePolicy,workerStartupMarkedRows,type WorkerStartupConsumer} from './project-worker-startup-source-sql';
export type ProjectWorkerProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string;beta?:boolean};
/** Exact HR preparation only. No Projects discovery, roles or Work context are
 * manufactured from the target or the included reference capability. */
export async function workerStartupProductEntry(database:SqlDatabase,scope:ProjectWorkerProductScope,target:{personId?:string},consumer:WorkerStartupConsumer){
 const policy=workerStartupSourcePolicy(consumer),projectWorkerCapabilities=policy.capabilities,projectWorkerSourceRows=policy.sourceRows,projectWorkerMarkedRows=workerStartupMarkedRows;
 if(scope.beta||scope.branchId||scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&[401,403].includes(error.status))return null;throw error;}
 if(!iam.permissions.includes('people.manage')||!iam.permissions.includes('organization.manage'))return null;
 const access=await readPurchasedAccesses(database,'enforced',scope.companyId,projectWorkerCapabilities);if(!projectWorkerCapabilities.every(cap=>access[cap].history))return null;
 const current=await database.prepare(`WITH project_worker_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)
 SELECT a.id FROM assignments a CROSS JOIN (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) project_worker_scope CROSS JOIN project_worker_clock
 WHERE a.id=? AND a.user_id=? AND a.company_id=project_worker_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind='holding' AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions revoked WHERE revoked.company_id=a.company_id AND revoked.user_id=a.user_id AND revoked.session_id=?)
 AND ${assignmentReadSql('project_worker_scope','people.manage')} AND ${assignmentReadSql('project_worker_scope','organization.manage')}
 AND ${policy.releases.map(feature=>scheduledFeaturePredicate('project_worker_scope.company_id',feature)).join(' AND ')}
 AND ${projectWorkerCapabilities.map(cap=>purchasedCapabilityPredicate('project_worker_scope.company_id',cap,'history','project_worker_clock.now_ms')).join(' AND ')}
 AND (?::text IS NULL OR EXISTS(SELECT 1 FROM hr_people person WHERE person.id=? AND person.company_id=project_worker_scope.company_id AND person.merged_into_id IS NULL AND (NOT EXISTS(SELECT 1 FROM (${projectWorkerMarkedRows}) marked WHERE marked.person_id=person.id) OR EXISTS(SELECT 1 FROM (${projectWorkerSourceRows}) source WHERE source.person_id=person.id AND source.company_id=person.company_id))))`)
 .bind(scope.companyId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.sessionId,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.userId,scope.role,target.personId??null,target.personId??null).first();
 return current?{revision:createHash('sha256').update(JSON.stringify([scope.companyId,target.personId??null,scope.assignmentId,scope.assignmentVersion,scope.role,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,...projectWorkerCapabilities.map(cap=>access[cap].revision)])).digest('hex'),purchasePurpose:'history' as const}:null;
}

export const projectWorkerStartupProductEntry=(database:SqlDatabase,scope:ProjectWorkerProductScope,target:{personId?:string}={})=>workerStartupProductEntry(database,scope,target,'projects');
