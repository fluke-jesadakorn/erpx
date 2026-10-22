import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {companyAuthority} from '@erpx/server-operations/public/staff-policy';
import {companyBusinessAcceptance} from '@erpx/server-finance/public/company-business-acceptance';
import {companyReleaseStatus} from '@erpx/server-platform_administration/public/release-acceptance';
import {BusinessRuleError} from '@erpx/server-shared/index';import {ApiError} from '@erpx/server-runtime/public/http';import {createBusinessReadinessApplication} from '@erpx/server-runtime/application/business-readiness';
export async function businessReadiness(actor:WorkContext){const {write}=await holdingResources(actor.holdingId);try{return await createBusinessReadinessApplication({prepare:async()=>{},companyAuthority:actor=>companyAuthority(write,actor),companyAcceptance:actor=>companyBusinessAcceptance(write,actor),releaseAcceptance:company=>companyReleaseStatus(write,company)}).businessReadiness(actor);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
