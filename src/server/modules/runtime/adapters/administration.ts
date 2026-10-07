import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {holdingResources} from '../public/runtime';
import {companyAuthority} from '../../operations/public/staff-policy';
import {canManagePeople} from '../../people_payroll/public/people-directory';
import {createAdministrationApplication} from '../application/administration';
export async function administrationAccess(actor:WorkContext){const {write}=await holdingResources(actor.holdingId);return createAdministrationApplication({prepare:async()=>{},companyAuthority:(actor,roles)=>companyAuthority(write,actor,roles),canManagePeople}).administrationAccess(actor);}
