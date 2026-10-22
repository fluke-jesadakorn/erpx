import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {companyAuthority} from '@erpx/server-operations/public/staff-policy';
import {canManagePeople} from '@erpx/server-people_payroll/public/people-directory';
import {createAdministrationApplication} from '@erpx/server-runtime/application/administration';
export async function administrationAccess(actor:WorkContext){const {write}=await holdingResources(actor.holdingId);return createAdministrationApplication({prepare:async()=>{},companyAuthority:(actor,roles)=>companyAuthority(write,actor,roles),canManagePeople}).administrationAccess(actor);}
