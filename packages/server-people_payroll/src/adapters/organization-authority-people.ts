import {ppRd42ActivePersonPoint} from './pp-read-tail-r42-sources';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {AppointmentInput} from '@erpx/server-organization/public/authority-contract';
import { nativePpStatement } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import { readOrganizationAuthorityPerson, type Lane7Actor } from './native-lane-7/client';
/** Native lane-7 successor: the owner axum reader owns this Company-scoped lookup. */
export async function organizationAuthorityActorPerson(_db:SqlDatabase,companyId:string,userId:string,actor?:Lane7Actor){return readOrganizationAuthorityPerson(companyId,{kind:'actor',userId},actor);}
/** Native lane-7 successor: the owner axum reader owns the engaged person lookup. */
export async function organizationAuthorityAppointee(_db:SqlDatabase,companyId:string,input:AppointmentInput,scopeId:string,selfCeo:boolean,actor?:Lane7Actor){if(!input.personId)return null;return readOrganizationAuthorityPerson(companyId,{kind:'appointee',personId:input.personId,positionId:input.positionId??'',appointmentKind:input.kind,scopeId,selfCeo},actor);}
export async function organizationAuthoritySelectorPerson(db:SqlDatabase,companyId:string,personId:string){return (await db.prepare(ppRd42ActivePersonPoint("user_id","hr_people","id","company_id")).bind(personId,companyId).first<{user_id:string}>())?.user_id??null;}
export function stageOrganizationSelfCeo(_db:SqlDatabase,companyId:string,personId:string|null,positionId:string|null,now:number){return nativePpStatement('pp_organization_self_ceo').bind(now,positionId,companyId,personId);}
