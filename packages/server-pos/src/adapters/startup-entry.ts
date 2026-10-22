import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {loadIamAuthority} from '@erpx/server-identity_access/public/iam-store';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {platformOwnerProjectionSources as identitySources} from '@erpx/server-identity_access/public/platform-owner-projection-sources';
import {readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
export type PosStartupProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string;beta?:boolean};
const capabilities=['pos.sales','inventory.stock-core','finance.pos-posting'] as const;
/** Exact setup eligibility before any Work role selection. A real current
 * native Company Finance or Company/Branch Stock assignment is required; no
 * module discovery, role selection or new assignment is granted. */
export async function posStartupProductEntry(database:SqlDatabase,scope:PosStartupProductScope){
 if(scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const access=await readPurchasedAccesses(database,'enforced',scope.companyId,capabilities);if(!capabilities.every(capability=>access[capability].history))return null;
 const permissionArgs:(string|number|null)[]=[];
 const permission=(name:string)=>{permissionArgs.push(scope.assignmentId,scope.userId,scope.role);return assignmentReadSql('pos_startup_scope',name);};
 const finance=scope.branchId?'FALSE':`(${permission('finance.read')} AND (${permission('finance.write')} OR ${permission('finance.approve')}))`;
 const stock=`(${permission('inventory.read')} AND ${permission('inventory.prepare')})`;
 const current=await database.prepare(`WITH pos_startup_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) pos_startup_scope CROSS JOIN pos_startup_clock
 WHERE a.id=? AND a.user_id=? AND a.company_id=pos_startup_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind IN ('holding','branch')
 AND (${finance} OR ${stock})
 AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate('pos_startup_scope.company_id',module)).join(' AND ')}
 AND ${capabilities.map(capability=>purchasedCapabilityPredicate('pos_startup_scope.company_id',capability,'history','pos_startup_clock.now_ms')).join(' AND ')}`)
 .bind(scope.companyId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,...permissionArgs).first();
 return current?{revision:createHash('sha256').update(JSON.stringify([scope.companyId,scope.branchId,scope.assignmentId,scope.assignmentVersion,scope.role,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,...capabilities.map(capability=>access[capability].revision)])).digest('hex'),purchasePurpose:'history' as const}:null;
}
