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
export type AssetsFinancialProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string;beta?:boolean};
/** Exact Assets financial workspace/record only, before Work role selection.
 * No general Finance, Asset discovery or acting context is manufactured. */
export async function assetsFinancialProductEntry(database:SqlDatabase,scope:AssetsFinancialProductScope,target:{assetId?:string}={}){
 if(scope.beta||scope.branchId||scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const caps=['assets.register','finance.asset-recognition','finance.asset-disposal'] as const,access=await readPurchasedAccesses(database,'enforced',scope.companyId,caps);
 if(!access['assets.register'].history||!access['finance.asset-recognition'].history&&!access['finance.asset-disposal'].history)return null;
 const current=await database.prepare(`WITH assets_financial_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)
 SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) assets_financial_scope CROSS JOIN assets_financial_clock
 WHERE a.id=? AND a.user_id=? AND a.company_id=assets_financial_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind='holding'
 AND ${assignmentReadSql('assets_financial_scope','finance.read')}
 AND ${['assets','finance'].map(feature=>scheduledFeaturePredicate('assets_financial_scope.company_id',feature)).join(' AND ')}
 AND ${purchasedCapabilityPredicate('assets_financial_scope.company_id','assets.register','history','assets_financial_clock.now_ms')}
 AND (${purchasedCapabilityPredicate('assets_financial_scope.company_id','finance.asset-recognition','history','assets_financial_clock.now_ms')} OR ${purchasedCapabilityPredicate('assets_financial_scope.company_id','finance.asset-disposal','history','assets_financial_clock.now_ms')})
 AND (?::text IS NULL OR EXISTS(SELECT 1 FROM finance_assets asset JOIN finance_accounts cost ON cost.id=asset.account_id AND cost.company_id=asset.company_id WHERE asset.id=? AND asset.company_id=assets_financial_scope.company_id))`)
 .bind(scope.companyId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role,target.assetId??null,target.assetId??null).first();
 return current?{revision:createHash('sha256').update(JSON.stringify([scope.companyId,target.assetId??null,scope.assignmentId,scope.assignmentVersion,scope.role,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,...caps.map(cap=>access[cap].revision)])).digest('hex'),purchasePurpose:'history' as const}:null;
}
