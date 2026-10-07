import type {PoolClient} from "pg";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function clearIdentityCommandAssertions(db:SqlDatabase,key:string){return db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key);}
export async function recordDirectoryVerification(client:PoolClient,job:{id:string;company_id:string},result:Record<string,unknown>,now:number){await client.query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$1,'people.directory.verified','people_directory',$2,1,$3,$4) ON CONFLICT(id) DO NOTHING",["directory-verified:"+job.id,job.id,JSON.stringify({id:job.id,companyId:job.company_id,result}),now]);}
