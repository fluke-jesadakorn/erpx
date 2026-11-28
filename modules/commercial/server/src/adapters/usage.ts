import {commercialFileReconciliation} from '@erpx/files-server/public/commercial-usage-facts';
import {commercialUnreconciledOriginals} from '@erpx/files-server/public/commercial-usage-facts';
import {commercialStoredOriginals} from '@erpx/files-server/public/commercial-usage-facts';
import {commercialAIUsage} from '@erpx/agent-server/public/commercial-usage-facts';
import {commercialUsageNodes} from '@erpx/organization-server/public/commercial-usage-facts';
import 'server-only';
import {createHash} from 'node:crypto';
import {holdingResources,runtimeEnv} from '../../../../../platform/runtime/server/src/public/runtime';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createUsagePeriodApplication} from '@erpx/commercial-server/application/usage';
import {createHoldingUsageApplication,type HoldingUsageRepository} from '@erpx/commercial-server/application/holding-usage';
import type {UsageBinding} from '@erpx/commercial-server/domain/holding-usage';
import {readActiveHoldingSeatCount,readControlSeatSummary,readHoldingSeatIntervals} from '@erpx/commercial-server/adapters/seats';
export {platformUsageItems} from '@erpx/commercial-server/domain/usage';
export type {UsageMetrics,UsageItem} from '@erpx/commercial-server/domain/usage';
export type {UsageBranch,UsageCompany,UsageHolding,UsageReport} from '@erpx/commercial-server/domain/holding-usage';
async function repository(holdingId:string,holdingSlug:string):Promise<HoldingUsageRepository>{const db=(await holdingResources(holdingId)).write,control=(await runtimeEnv()).CONTROL_DB;const holding={holdingId,holdingSlug};return {async facts(binding,start,end,branchId){const [nodes, ai, storage, unreconciled, reconciliation] = await Promise.all([
    commercialUsageNodes(db,),
    commercialAIUsage(db,binding,start,end,branchId),
    commercialStoredOriginals(db,start,end,branchId,holding),
    commercialUnreconciledOriginals(db,end,branchId,holding),
    commercialFileReconciliation(db,holding),
    ]);return {nodes:nodes.results,ai:ai.results,storage:storage.results,unreconciled,reconciliation};},seats:month=>readHoldingSeatIntervals(db,{holdingId,holdingSlug},month),monthlySeats:(binding,month)=>readControlSeatSummary(control,binding.id,month),activeSeats:()=>readActiveHoldingSeatCount(db,control)};}
export function usageMonth(value:string|null){try{return createUsagePeriodApplication({now:()=>Date.now()})(value);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function readHoldingUsage(binding:UsageBinding,month:string,branchId:string|null=null){try{return await createHoldingUsageApplication({clock:{now:()=>Date.now()},hash:value=>createHash('sha256').update(value).digest('hex'),repository})(binding,month,branchId);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}






