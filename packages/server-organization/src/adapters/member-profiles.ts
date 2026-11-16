import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {activeHoldingProfileMember} from "@erpx/server-identity_access/public/profile-membership";
import {memberProfileAccount} from "@erpx/server-identity_access/public/profile-account";
import {memberProfileWork} from "@erpx/server-people_payroll/public/profile-work";
import {memberProfilePostIds} from "@erpx/server-collaboration/public/profile-posts";
import {postViews} from '@erpx/server-collaboration/public/social-home';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createMemberProfileApplication} from "@erpx/server-organization/application/member-profiles";
import type {ProfileRow,MemberProfileInput} from "@erpx/server-organization/domain/member-profiles";
export {profileInput} from "@erpx/server-organization/domain/member-profiles";
function application(actor:WorkContext){let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);const app=createMemberProfileApplication({clock:{now:()=>Date.now()},actor,
 activeMember:async(id,now)=>activeHoldingProfileMember(await database(),id,now),account:memberProfileAccount,
 profile:async(id)=>(await database()).prepare('SELECT user_id,bio,links_json,avatar_key,cover_key,avatar_media_type,cover_media_type,version FROM member_profiles WHERE user_id=?').bind(id).first<ProfileRow>(),
 work:async(id)=>memberProfileWork(await database(),id),postIds:async(id,before,beforeId)=>memberProfilePostIds(await database(),actor,id,before,beforeId),posts:async(ids,limit)=>postViews(await database(),actor,ids,limit),
 replay:async(key,id,input)=>replayCommand<{version:number;bio:string;links:{label:string;url:string}[]}>(await database(),actor,key,'member.profile.update',id,input),
 save:async(commandId,_id,input,now)=>{const write=await database();
  const nextVersion = input.version + 1;
  const result = { version: nextVersion, bio: input.bio, links: input.links };
  return executeCommand(write,actor,commandId,"member.profile.update",input,actor.userId,nextVersion,result,[
    assertion(write,commandId,"profile-version",input.version===0
      ? "NOT EXISTS(SELECT 1 FROM member_profiles WHERE user_id=?)"
      : "EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=?)",
      input.version===0?[actor.userId]:[actor.userId,input.version]),
    input.version===0
      ? write.prepare("INSERT INTO member_profiles(user_id,bio,links_json,version,updated_at) VALUES(?,?,?::jsonb,?,?)")
        .bind(actor.userId,input.bio,JSON.stringify(input.links),nextVersion,now)
      : write.prepare("UPDATE member_profiles SET bio=?,links_json=?::jsonb,version=?,updated_at=? WHERE user_id=? AND version=?")
        .bind(input.bio,JSON.stringify(input.links),nextVersion,now,actor.userId,input.version),
  ]);
},media:async(id,kind)=>(await database()).prepare(`SELECT ${kind}_key AS key,${kind}_media_type AS media_type FROM member_profiles WHERE user_id=?`).bind(id).first<{key:string|null;media_type:string|null}>(),});return {app,database};}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const activeHoldingMember=(a:WorkContext,id:string)=>invoke(async()=>{const {app,database}=application(a);await app.active(id);return database();});
export const memberProfileView=(a:WorkContext,id:string,before?:number,beforeId?:string)=>invoke(()=>application(a).app.view(id,before,beforeId));
export const saveMemberProfile=(a:WorkContext,input:MemberProfileInput,key:string)=>invoke(()=>application(a).app.save(input,key));
export const profileMediaRow=(a:WorkContext,id:string,kind:'avatar'|'cover')=>invoke(()=>application(a).app.media(id,kind));
