import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {holdingResources} from '../public/runtime';
import {companyAuthority} from '../../operations/public/staff-policy';
import {companyBusinessAcceptance} from '../../finance/public/company-business-acceptance';
import {companyReleaseStatus} from '../../platform_administration/public/release-acceptance';
import {BusinessRuleError} from '../../shared';import {ApiError} from '../public/http';import {createBusinessReadinessApplication} from '../application/business-readiness';
export async function businessReadiness(actor:WorkContext){const {write}=await holdingResources(actor.holdingId);try{return await createBusinessReadinessApplication({prepare:async()=>{},companyAuthority:actor=>companyAuthority(write,actor),companyAcceptance:actor=>companyBusinessAcceptance(write,actor),releaseAcceptance:company=>companyReleaseStatus(write,company)}).businessReadiness(actor);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
