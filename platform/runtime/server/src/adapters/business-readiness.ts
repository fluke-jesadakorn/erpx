import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {holdingResources} from '../public/runtime';
import {companyAuthority} from '@erpx/operations-server/public/staff-policy';
import {companyBusinessAcceptance} from '@erpx/finance-server/public/company-business-acceptance';
import {companyReleaseStatus} from '@erpx/platform-administration-server/public/release-acceptance';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';import {ApiError} from '@erpx/platform-runtime-server/public/http';import {createBusinessReadinessApplication} from '@erpx/platform-runtime-server/application/business-readiness';
export async function businessReadiness(actor:WorkContext){const {write}=await holdingResources(actor.holdingId);try{return await createBusinessReadinessApplication({prepare:async()=>{},companyAuthority:actor=>companyAuthority(write,actor),companyAcceptance:actor=>companyBusinessAcceptance(write,actor),releaseAcceptance:company=>companyReleaseStatus(write,company)}).businessReadiness(actor);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
