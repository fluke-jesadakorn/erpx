import 'server-only';
import {loadIamAuthority} from '@erpx/identity-access-server/public/iam-store';
import {assignmentReadSql} from '@erpx/identity-access-server/public/record-source-authority';
import {initialEntryClockAssignmentsSql,stockHistoryExistsSql} from './read-tail-sql';
import {purchasedCapabilityPredicate} from '@erpx/commercial-server/public/purchased-capability-sql';
import {scheduledFeaturePredicate} from '@erpx/pms-server/public/feature-availability';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {serviceInitialPhysicalStockCapabilities,serviceInitialPhysicalStockCountSource} from '@erpx/inventory-server/adapters/service-initial-physical-stock-count-source';
export type ServiceInitialPhysicalStockCountProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string};
export async function serviceInitialPhysicalStockCountProductEntry(database:SqlDatabase,scope:ServiceInitialPhysicalStockCountProductScope,target:{countId?:string}){
 if(scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 try{await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const count=target.countId?await serviceInitialPhysicalStockCountSource(database,scope,target.countId):null;if(target.countId&&!count)return null;
 const result=await database.prepare(`${initialEntryClockAssignmentsSql}
 WHERE a.id=? AND a.user_id=? AND a.role=? AND a.company_id=initial_entry_scope.company_id AND a.version=? AND ${assignmentReadSql('initial_entry_scope','inventory.read')}
 ${target.countId?'':`AND ${assignmentReadSql('initial_entry_scope','inventory.prepare')}`}
 AND ${['field_service','inventory','finance'].map(module=>scheduledFeaturePredicate('initial_entry_scope.company_id',module)).join(' AND ')}
 AND ${serviceInitialPhysicalStockCapabilities.map(cap=>purchasedCapabilityPredicate('initial_entry_scope.company_id',cap,target.countId?'history':'new-work','initial_entry_clock.now_ms')).join(' AND ')}`).bind(scope.companyId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role,...(target.countId?[]:[scope.assignmentId,scope.userId,scope.role])).first();
 return result?{capability:'service.parts-assets' as const,purchasePurpose:'history' as const}:null;
}
