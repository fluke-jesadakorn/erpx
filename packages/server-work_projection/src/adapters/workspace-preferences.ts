import 'server-only';
import {headers} from 'next/headers';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {createNativeAccountPreferencesClient} from './native-account-preferences';
function client(){const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;if(!raw||!process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG)throw new ApiError(503,'NATIVE_ACCOUNT_PREFERENCE_TARGET_UNAVAILABLE','The native account preference is not configured.');return createNativeAccountPreferencesClient(raw);}
export async function readNativeAccountPreferences(incoming:Pick<Headers,'get'>,userId:string){return client()(incoming,userId,'GET');}
// Resource parameters remain only for callers awaiting signature migration.
// They perform no SQL and cannot provide authority or a fallback implementation.
export async function readWorkspacePreference(_db:SqlDatabase,userId:string){void _db;const p=(await client()(await headers(),userId,'GET')).workspace;return p?{workspace_path:p.workspacePath,platform_assignment_id:p.platformAssignmentId}:null;}
export async function readCompanyPreference(_db:SqlDatabase,userId:string,holdingId:string){void _db;const p=(await client()(await headers(),userId,'GET',undefined,holdingId)).company;return p?{company_id:p.companyId}:null;}
export async function selectNativeCompanyPreference(incoming:Pick<Headers,'get'>,userId:string,body:string,holdingSlug:string){const state=await client()(incoming,userId,'POST',body,undefined,holdingSlug);let raw:unknown;try{raw=JSON.parse(body);}catch{throw new ApiError(502,'NATIVE_ACCOUNT_PREFERENCE_RESPONSE_INVALID','The Company acknowledgement does not match this request.');}if(!raw||typeof raw!=='object'||!('companyId'in raw)||state.company?.companyId!==raw.companyId)throw new ApiError(502,'NATIVE_ACCOUNT_PREFERENCE_RESPONSE_INVALID','The Company acknowledgement does not match this request.');return state;}
export async function rememberWorkspace(_db:SqlDatabase,userId:string,path:string,company?:{holdingId:string;companyId:string}):Promise<void>{
 void _db;if(!company)throw new ApiError(503,'NATIVE_PLATFORM_PREFERENCE_UNAVAILABLE','The native platform workspace preference is not enabled yet.');
 const state=await client()(await headers(),userId,'POST',JSON.stringify({path,companyId:company.companyId}),company.holdingId,undefined,true);
 if(state.workspace?.workspacePath!==path||state.company?.companyId!==company.companyId)throw new ApiError(502,'NATIVE_ACCOUNT_PREFERENCE_RESPONSE_INVALID','The workspace acknowledgement does not match this request.');
}
export async function rememberPlatformRole(_db:SqlDatabase,_userId:string,_assignmentId:string):Promise<void>{void _db;void _userId;void _assignmentId;throw new ApiError(503,'NATIVE_PLATFORM_PREFERENCE_UNAVAILABLE','The native platform preference command is not enabled yet.');}
