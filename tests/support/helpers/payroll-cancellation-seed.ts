import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";

/** Seed the exact native cancellation receipt + immutable evidence for tests.
 * Mirrors the owner command's transaction shape; does not bypass the trigger. */
export async function cancelPayrollRunSeed(db:SqlDatabase,input:{runId:string;companyId:string;actorId:string;reason:string;key?:string}){
 const key=input.key??`payroll-cancel-seed-${crypto.randomUUID()}`,now=Date.now(),cancellationId=`payroll-cancellation-${key}`;
 const run=await db.prepare("SELECT to_jsonb(r)::text snapshot,status,version,revision FROM hr_payroll_runs r WHERE id=? AND company_id=?").bind(input.runId,input.companyId).first<{snapshot:string;status:string;version:number;revision:number}>();
 if(!run)throw new Error("PAYROLL_RUN_NOT_FOUND");
 const result=JSON.stringify({id:input.runId,status:"cancelled",version:run.version+1,cancellationId});
 await db.batch([
  db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)").bind(key,"people.payroll.cancel",input.actorId,"payroll-cancel-seed",input.runId,result,now,JSON.stringify({companyId:input.companyId})),
  db.prepare("UPDATE hr_payroll_runs SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now,input.runId,input.companyId),
  db.prepare("INSERT INTO hr_payroll_run_cancellations(id,company_id,run_id,command_id,cancelled_by,reason,prior_status,prior_version,run_revision,source_snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(cancellationId,input.companyId,input.runId,key,input.actorId,input.reason,run.status,run.version,run.revision,run.snapshot,now),
 ]);
}
