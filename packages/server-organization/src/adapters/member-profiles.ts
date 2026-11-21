import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {activeHoldingProfileMember} from "@erpx/server-identity_access/public/profile-membership";
import {memberProfileAccount} from "@erpx/server-identity_access/public/profile-account";
import {memberProfileWork} from "@erpx/server-people_payroll/public/profile-work";
import {memberProfilePostIds} from "@erpx/server-collaboration/public/profile-posts";
import {postViews} from '@erpx/server-collaboration/public/social-home';
import {ApiError} from "@erpx/server-runtime/public/http";
import {replayCommand} from "@erpx/server-workflow/public/commands";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createMemberProfileApplication} from "@erpx/server-organization/application/member-profiles";
import type {ProfileRow,MemberProfileInput} from "@erpx/server-organization/domain/member-profiles";
import {NativeMemberProfileError,nativeSaveMemberProfile} from "./native-member-profile-command";
export {profileInput} from "@erpx/server-organization/domain/member-profiles";
function application(actor:WorkContext){let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);const app=createMemberProfileApplication({clock:{now:()=>Date.now()},actor,
 activeMember:async(id,now)=>activeHoldingProfileMember(await database(),id,now),account:memberProfileAccount,
 profile:async(id)=>(await database()).prepare('SELECT user_id,bio,links_json,avatar_key,cover_key,avatar_media_type,cover_media_type,version FROM member_profiles WHERE user_id=?').bind(id).first<ProfileRow>(),
 work:async(id)=>memberProfileWork(await database(),id),postIds:async(id,before,beforeId)=>memberProfilePostIds(await database(),actor,id,before,beforeId),posts:async(ids,limit)=>postViews(await database(),actor,ids,limit),
 replay:async(key,id,input)=>replayCommand<{version:number;bio:string;links:{label:string;url:string}[]}>(await database(),actor,key,'member.profile.update',id,input),
 // Native axum command owns the member_profiles write, optimistic version,
 // replay, receipt/audit/outbox and identity membership. No TS SQL, no fallback.
 save:async(commandId,_id,input)=>nativeSaveMemberProfile(actor,input,commandId),
 media:async(id,kind)=>(await database()).prepare(`SELECT ${kind}_key AS key,${kind}_media_type AS media_type FROM member_profiles WHERE user_id=?`).bind(id).first<{key:string|null;media_type:string|null}>(),});return {app,database};}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);if(error instanceof NativeMemberProfileError)throw new ApiError(error.status??503,"NATIVE_ORGANIZATION_UNAVAILABLE","The native organization profile service is unavailable.");throw error;}}
export const activeHoldingMember=(a:WorkContext,id:string)=>invoke(async()=>{const {app,database}=application(a);await app.active(id);return database();});
export const memberProfileView=(a:WorkContext,id:string,before?:number,beforeId?:string)=>invoke(()=>application(a).app.view(id,before,beforeId));
export const saveMemberProfile=(a:WorkContext,input:MemberProfileInput,key:string)=>invoke(()=>application(a).app.save(input,key));
export const profileMediaRow=(a:WorkContext,id:string,kind:'avatar'|'cover')=>invoke(()=>application(a).app.media(id,kind));
