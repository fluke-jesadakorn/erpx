import {holdingFetch} from '@erpx/contracts/holding-fetch';
import type {DirectorySignInInput,DirectorySignInResponse} from '@erpx/contracts/account-auth';
import type {AccountAuthResult} from '../../application/account-auth-port';
export interface AccountDirectoryPort {signIn(input:DirectorySignInInput):Promise<AccountAuthResult<DirectorySignInResponse>>}
type Send=(input:string,init:RequestInit)=>Promise<Response>;
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0&&value.length<=4096;
const object=(value:unknown):value is Record<string,unknown>=>typeof value==='object'&&value!==null&&!Array.isArray(value);
/** The existing Account Directory wire carries intent only; bearer values stay in cookies. */
export function createAccountDirectoryHttpPort(send:Send=holdingFetch):AccountDirectoryPort{
 return{async signIn(input){
  if(input.revision!==undefined&&(!Number.isSafeInteger(input.revision)||input.revision<0))return{data:null,error:{code:'INVALID_DEVICE_REVISION',message:'Review your current saved accounts.'}};
  try{
   const response=await send('/api/auth/directory/sign-in',{method:'POST',credentials:'include',cache:'no-store',redirect:'error',headers:{accept:'application/json','content-type':'application/json'},body:JSON.stringify(input)});const value:unknown=await response.json();
   if(!response.ok){const error=object(value)&&object(value.error)?value.error:null;return{data:null,error:{code:error&&text(error.code)?error.code:'DIRECTORY_SIGN_IN_FAILED',message:error&&text(error.message)?error.message:'Directory sign-in failed. Check your credentials and approved access.'}};}
   if(!object(value)||Object.keys(value).length!==1||value.success!==true)return{data:null,error:{code:'ACCOUNT_RESPONSE_INVALID',message:'The Directory response did not match its contract.'}};
   return{data:{success:true},error:null};
  }catch{return{data:null,error:{code:'ACCOUNT_NETWORK_UNAVAILABLE',message:'Could not connect to Account. Please try again.'}};}
 }};
}
