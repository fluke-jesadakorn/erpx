import 'server-only';
import {headers} from 'next/headers';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {createNativeWorkRoleTransport} from '@erpx/server-identity_access/public/native-work-role-transport';
import type {ActiveWorkSelection,SelectWorkRole} from '../domain/work-role-store';
type Identity={sessionId:string;userId:string;companyId:string;expiresAt?:number};
async function read(input:Identity,mode:'current'|'restore'|'initialize'|'select',body?:string){
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;
 if(!raw)throw new ApiError(503,'NATIVE_WORK_ROLE_TARGET_UNAVAILABLE','The native work role selection is not configured.');
 const state=await createNativeWorkRoleTransport(raw)(await headers(),input,mode,body),s=state.selection;
 return s?{session_id:state.sessionId,user_id:state.userId,company_id:state.companyId,assignment_id:s.assignmentId,assignment_version:s.assignmentVersion,revision:s.revision,expires_at:s.expiresAt} satisfies ActiveWorkSelection:null;
}
// Existing non-Field callers retain the borrowed resource argument until their
// own transport boundaries move. It is never read, written or used as authority.
// Unconfigured hosts are explicitly unavailable; there is no legacy SQL path.
export function readWorkSelection(_db:SqlDatabase,sessionId:string,userId:string,companyId:string){return read({sessionId,userId,companyId},'current');}
export function selectWorkRole(_db:SqlDatabase,input:SelectWorkRole){void _db;return selectNativeWorkRole(input);}
export async function selectNativeWorkRole(input:SelectWorkRole){
 if(input.restoring)throw new ApiError(409,'WORK_CONTEXT_CHANGED','Restore a saved role through the native initialization endpoint.');
 const result=await read(input,'select',JSON.stringify({assignmentId:input.assignmentId,assignmentVersion:input.assignmentVersion,revision:input.revision,remember:input.rememberPreference!==false}));
 if(!result)throw new ApiError(409,'WORK_CONTEXT_CHANGED','Choose your current work role.');
 return result;
}
export function restoreWorkSelection(_db:SqlDatabase,input:Identity,_assignments:{id:string;version?:number}[]){void _assignments;return read(input,'restore');}
export function initializeWorkSelection(_db:SqlDatabase,input:Identity,_assignments:{id:string;version?:number}[],_eligible:{id:string;version?:number}[]){void _assignments;void _eligible;return read(input,'initialize');}
export type {ActiveWorkSelection,WorkAuthority,SelectWorkRole} from '../domain/work-role-store';
