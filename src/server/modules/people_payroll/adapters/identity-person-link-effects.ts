import type {PoolClient} from "pg";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Stages person and employee identity links on the caller's atomic command. */
export function stageInvitationPersonLinks(db:SqlDatabase,userId:string,personId:string,now:number){return [
 db.prepare("UPDATE hr_people SET user_id=?,version=version+1,updated_at=? WHERE id=?").bind(userId,now,personId),
 db.prepare("UPDATE employees SET user_id=? WHERE person_id=?").bind(userId,personId),
];}
export async function linkMigratedPerson(client:PoolClient,userId:string,personId:string,companyId:string){await client.query("UPDATE hr_people SET user_id=$1,version=version+1 WHERE id=$2 AND company_id=$3",[userId,personId,companyId]);}
export async function linkMigratedEmployee(client:PoolClient,userId:string,localUserId:string,companyId:string){await client.query("UPDATE employees SET user_id=$1 WHERE user_id=$2 AND company_id=$3",[userId,localUserId,companyId]);}
