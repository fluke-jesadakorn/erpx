import 'server-only';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {JobEnv} from "@erpx/server-workflow/public/job-projector";
import {ApiError} from "@erpx/server-runtime/public/http";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";

import {BusinessRuleError} from '@erpx/server-shared/index';
import {createCustomerNotificationApplication,type CustomerNotificationRepository} from '@erpx/server-sales/application/notifications';
import {requireSalesNotificationRead} from '@erpx/server-sales/adapters/notification-purchases';
export {customerNotificationSchema,customerNotificationRetrySchema,customerNotificationSettingsSchema} from '@erpx/server-sales/domain/notifications';
type Row=Record<string,string|number|null>;
const exec=promisify(execFile);
function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);return createCustomerNotificationApplication({companyId:actor.companyId,authorize:permission=>{requirePermission(actor,permission);},repository:async()=>repository(await database(),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):CustomerNotificationRepository{return {
 settings:async()=>write.prepare('SELECT * FROM commerce_customer_notification_settings WHERE company_id=?').bind(actor.companyId).first<Row>(),
 };}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerNotificationSettings(actor:WorkContext){return translated(async()=>{const database=(await holdingResources(actor.holdingId)).write;await requireSalesNotificationRead(database,actor);return application(actor,database).settings();});}

/**
 * Workflow bridge: one queued/retried notice event asks the exact native Sales
 * worker to deliver this immutable notice. TypeScript performs no SQL write and
 * no provider call; a missing native worker fails the job closed.
 */
export async function deliverCustomerNotification(_database:SqlDatabase,_env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,id:string):Promise<void>{
 const binary=process.env.ERPX_NATIVE_SERVER_BIN,manifest=process.env.ERPX_DATABASE_MANIFEST;
 if(!binary||!manifest)throw new ApiError(503,'NATIVE_SALES_NOTIFICATION_WORKER_REQUIRED','The native customer-notice worker is not configured.');
 let stdout:string;
 try{({stdout}=await exec(binary,['sales-notification-worker','--once','--notification',id,'--database-manifest',manifest],{timeout:120_000,maxBuffer:2*1024*1024,env:process.env,windowsHide:true}));}
 catch{throw new ApiError(503,'NATIVE_SALES_NOTIFICATION_WORKER_FAILED','The native customer-notice worker did not complete.');}
 let outcome:unknown;
 try{outcome=JSON.parse(stdout);}
 catch{throw new ApiError(503,'NATIVE_SALES_NOTIFICATION_WORKER_RESPONSE_INVALID','The native customer-notice worker response is invalid.');}
 const holdings=outcome&&typeof outcome==='object'&&!Array.isArray(outcome)?(outcome as {holdings?:unknown}).holdings:undefined;
 if(!Array.isArray(holdings))throw new ApiError(503,'NATIVE_SALES_NOTIFICATION_WORKER_RESPONSE_INVALID','The native customer-notice worker response is invalid.');
}
