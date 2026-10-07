import {commercialFileReconciliation} from '../../files/public/commercial-usage-facts';
import {commercialUnreconciledOriginals} from '../../files/public/commercial-usage-facts';
import {commercialStoredOriginals} from '../../files/public/commercial-usage-facts';
import {commercialAIUsage} from '../../agent/public/commercial-usage-facts';
import {commercialUsageNodes} from '../../organization/public/commercial-usage-facts';
import 'server-only';
import {createHash} from 'node:crypto';
import {holdingResources,runtimeEnv} from '../../runtime/public/runtime';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {createUsagePeriodApplication} from '../application/usage';
import {createHoldingUsageApplication,type HoldingUsageRepository} from '../application/holding-usage';
import type {UsageBinding} from '../domain/holding-usage';
import {readActiveHoldingSeatCount,readControlSeatSummary,readHoldingSeatIntervals} from './seats';
export {platformUsageItems} from '../domain/usage';
export type {UsageMetrics,UsageItem} from '../domain/usage';
export type {UsageBranch,UsageCompany,UsageHolding,UsageReport} from '../domain/holding-usage';
async function repository(holdingId:string):Promise<HoldingUsageRepository>{const db=(await holdingResources(holdingId)).write,control=(await runtimeEnv()).CONTROL_DB;return {async facts(binding,start,end,branchId){const [nodes, ai, storage, unreconciled, reconciliation] = await Promise.all([
    commercialUsageNodes(db,),
    commercialAIUsage(db,binding,start,end,branchId),
    commercialStoredOriginals(db,start,end,branchId),
    commercialUnreconciledOriginals(db,end,branchId),
    commercialFileReconciliation(db,),
    ]);return {nodes:nodes.results,ai:ai.results,storage:storage.results,unreconciled,reconciliation};},seats:month=>readHoldingSeatIntervals(db,month),monthlySeats:(binding,month)=>readControlSeatSummary(control,binding.id,month),activeSeats:()=>readActiveHoldingSeatCount(db,control)};}
export function usageMonth(value:string|null){try{return createUsagePeriodApplication({now:()=>Date.now()})(value);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function readHoldingUsage(binding:UsageBinding,month:string,branchId:string|null=null){try{return await createHoldingUsageApplication({clock:{now:()=>Date.now()},hash:value=>createHash('sha256').update(value).digest('hex'),repository})(binding,month,branchId);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}






