import "server-only";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import { nativeWorkflowReadAll } from "./native-workflow-read";
import { commandReceiptByCommandSql } from './receipt-sql';

export async function departmentOverrideReceiptOperation(db:SqlDatabase,key:string){return (await db.prepare(commandReceiptByCommandSql('operation')).bind(key).first<{operation:string}>())?.operation??null;}

/** Owner-native read: the department approval queue statement is planned by the
 * mounted Workflow wave3 command planner (WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE)
 * and executed on the caller's open unit. This read carries no SQL. */
export async function departmentOverridePendingApprovals(db:SqlDatabase,companyId:string){
  return nativeWorkflowReadAll<{kind:'expense'|'leave'|'overtime';id:string;updated_at:number}>(
    'wf_native_department_approval_queue',
    [companyId],
    companyId,
    db,
  );
}
