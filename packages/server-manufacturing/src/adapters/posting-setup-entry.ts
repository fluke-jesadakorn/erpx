import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {loadIamAuthority} from '@erpx/server-identity_access/public/iam-store';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {platformOwnerProjectionSources as identitySources} from '@erpx/server-identity_access/public/platform-owner-projection-sources';
import {readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/hospitality-rollout';
export type ManufacturingPostingSetupProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string;beta?:boolean};
const capabilities=['manufacturing.materials','manufacturing.costs','inventory.stock-core','finance.production-costing'] as const;
/** Exact setup eligibility before any Work role selection. A real current
 * native holding assignment is required; no module discovery/role is granted. */
export async function manufacturingPostingSetupProductEntry(database:SqlDatabase,scope:ManufacturingPostingSetupProductScope){
 if(scope.beta||scope.branchId||scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const access=await readPurchasedAccesses(database,'enforced',scope.companyId,capabilities);if(!capabilities.every(capability=>access[capability].history))return null;
 const current=await database.prepare(`WITH manufacturing_setup_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) manufacturing_setup_scope CROSS JOIN manufacturing_setup_clock
 WHERE a.id=? AND a.user_id=? AND a.company_id=manufacturing_setup_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind='holding'
 AND ${assignmentReadSql('manufacturing_setup_scope','finance.read')} AND (${assignmentReadSql('manufacturing_setup_scope','finance.write')} OR ${assignmentReadSql('manufacturing_setup_scope','finance.approve')})
 AND ${['manufacturing','inventory','finance'].map(module=>scheduledFeaturePredicate('manufacturing_setup_scope.company_id',module)).join(' AND ')}
 AND ${capabilities.map(capability=>purchasedCapabilityPredicate('manufacturing_setup_scope.company_id',capability,'history','manufacturing_setup_clock.now_ms')).join(' AND ')}`)
 .bind(scope.companyId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.userId,scope.role).first();
 return current?{revision:createHash('sha256').update(JSON.stringify([scope.companyId,scope.assignmentId,scope.assignmentVersion,scope.role,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,...capabilities.map(capability=>access[capability].revision)])).digest('hex'),purchasePurpose:'history' as const}:null;
}
