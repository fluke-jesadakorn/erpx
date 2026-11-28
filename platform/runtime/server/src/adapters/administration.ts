import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {holdingResources} from '../public/runtime';
import {companyAuthority} from '@erpx/operations-server/public/staff-policy';
import {canManagePeople} from '@erpx/people-payroll-server/public/people-directory';
import {createAdministrationApplication} from '@erpx/platform-runtime-server/application/administration';
export async function administrationAccess(actor:WorkContext){const {write}=await holdingResources(actor.holdingId);return createAdministrationApplication({prepare:async()=>{},companyAuthority:(actor,roles)=>companyAuthority(write,actor,roles),canManagePeople}).administrationAccess(actor);}
