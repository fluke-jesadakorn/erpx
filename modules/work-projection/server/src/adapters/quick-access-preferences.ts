import "server-only";
import {createHash} from "node:crypto";
import type {QuickAccessPreference} from "@erpx/work-projection-ui/lib/quick-access";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {getRuntime} from "@erpx/platform-runtime-server/public/runtime/env";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {createQuickPreferenceService,type QuickPreferenceUnit} from "@erpx/work-projection-server/application/quick-access-preferences";
import type {SaveInput} from "@erpx/work-projection-server/domain/quick-access-preferences";
import {assertPrivateSessionPreferenceUnit} from '@erpx/identity-access-server/public/private-session-preferences';
import { nativeOwnerWrite } from "./native-owner-write";
import { noopSql, receiptByIdSql } from "./read-tail-sql";
export {quickAccessPreferenceSchema} from "@erpx/work-projection-server/domain/quick-access-preferences";
function unit(db:SqlDatabase,owned=false):QuickPreferenceUnit{const noop=()=>db.prepare(noopSql);return {transactional:owned||isTransactionalSqlDatabase(db),preference:(userId,surface,contextKey)=>db.prepare("SELECT pinned_ids_json,version FROM quick_access_preferences WHERE user_id=? AND surface=? AND context_key=?").bind(userId,surface,contextKey).first(),receipt:key=>db.prepare(receiptByIdSql('user_id,surface,context_key,request_hash,result_json','quick_access_preference_receipts')).bind(key).first(),savePreference:async(userId,surface,contextKey,result,now)=>nativeOwnerWrite('wp_quick_access_save',[userId,surface,contextKey,result.pinnedIds===null?null:JSON.stringify(result.pinnedIds),result.version,now],`wp-quick:${userId}:${surface}:${contextKey}:${result.version}`,{now},noop),saveReceipt:async(key,userId,surface,contextKey,hash,result,now)=>nativeOwnerWrite('wp_quick_access_receipt_save',[key,userId,surface,contextKey,hash,JSON.stringify(result),now],`wp-quick-receipt:${key}`,{now},noop),transaction:action=>{if(owned)return action(unit(db,true));if(!isTransactionalSqlDatabase(db))throw new Error("CONTROL_TRANSACTION_REQUIRED");return db.transaction(tx=>action(unit(tx)));}};}
function service(database:SqlDatabase=getRuntime().CONTROL_DB,owned=false){return createQuickPreferenceService({repository:unit(database,owned),clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),hash:value=>createHash("sha256").update(value).digest("hex")});}
export function readQuickAccessPreferences(userId:string,surface:string,contextKey:string):Promise<QuickAccessPreference>{return service().readQuickAccessPreferences(userId,surface,contextKey);}
export function saveQuickAccessPreferences(input:SaveInput):Promise<QuickAccessPreference>{return service().saveQuickAccessPreferences(input);}
export function readSessionQuickAccessPreferences(database:SqlDatabase,userId:string,surface:string,contextKey:string):Promise<QuickAccessPreference>{assertPrivateSessionPreferenceUnit(database,userId);return service(database,true).readQuickAccessPreferences(userId,surface,contextKey);}
export function saveSessionQuickAccessPreferences(database:SqlDatabase,input:SaveInput):Promise<QuickAccessPreference>{assertPrivateSessionPreferenceUnit(database,input.userId);return service(database,true).saveQuickAccessPreferences(input);}
