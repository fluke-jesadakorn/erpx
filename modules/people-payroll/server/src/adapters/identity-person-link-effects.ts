import { nativePpStatement, nativePpQuery } from '@erpx/people-payroll-server/adapters/native-people-payroll-command';
import type {PoolClient} from "pg";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
/** Stages person and employee identity links on the caller's atomic command. */
export function stageInvitationPersonLinks(db:SqlDatabase,userId:string,personId:string,now:number){return [
 nativePpStatement('pp_identity_person_link_effects_stmt_1').bind(userId,now,personId),
 nativePpStatement('pp_identity_person_link_effects_stmt_2').bind(userId,personId),
];}
export async function linkMigratedPerson(client:PoolClient,userId:string,personId:string,companyId:string){await nativePpQuery('pp_identity_person_link_effects_stmt_3',[userId,personId,companyId]);}
export async function linkMigratedEmployee(client:PoolClient,userId:string,localUserId:string,companyId:string){await nativePpQuery('pp_identity_person_link_effects_stmt_4',[userId,localUserId,companyId]);}
