import type {AccountSecurityPort} from '../../application/account-security-port';
import {accountSecuritySeedSchema} from '../../lib/account-security';
import type {AccountMfaEnrollment,AccountMfaVerified,AccountSecuritySeed} from '@erpx/contracts/account-auth';

type FetchPort=(input:string,init:RequestInit)=>Promise<Response>;
const record=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const keys=(v:Record<string,unknown>,names:string[])=>Object.keys(v).length===names.length&&names.every(name=>Object.hasOwn(v,name));
const integer=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
function enrollment(value:unknown):value is AccountMfaEnrollment {
  if(!record(value)||!keys(value,['secret','uri'])||typeof value.secret!=='string'||!/^[A-Z2-7]{32}$/.test(value.secret)||typeof value.uri!=='string'||value.uri.length>2048||!value.uri.startsWith('otpauth://totp/ERPX:'))return false;
  try{const uri=new URL(value.uri);return uri.username===''&&uri.password===''&&uri.hash===''&&[...uri.searchParams.keys()].length===5&&uri.searchParams.get('secret')===value.secret&&uri.searchParams.get('issuer')==='ERPX'&&uri.searchParams.get('algorithm')==='SHA1'&&uri.searchParams.get('digits')==='6'&&uri.searchParams.get('period')==='30';}catch{return false;}
}
const verified=(value:unknown):value is AccountMfaVerified=>record(value)&&keys(value,['verifiedUntil'])&&integer(value.verifiedUntil);
function seed(value:unknown):value is AccountSecuritySeed {
  const parsed=accountSecuritySeedSchema.safeParse(value);if(!parsed.success)return false;const v=parsed.data;
  return integer(v.legacySessions)&&(v.mfaFreshUntil===null||integer(v.mfaFreshUntil))&&v.families.every(f=>integer(f.createdAt)&&integer(f.expiresAt))&&new Set(v.families.map(f=>f.handle)).size===v.families.length&&v.families.filter(f=>f.current).length<=1;
}
/** Existing same-origin endpoints; strict Rust envelopes and no browser storage. */
export function createAccountSecurityHttpPort(fetcher:FetchPort=(input,init)=>globalThis.fetch(input,init)):AccountSecurityPort {
  async function request<T>(path:string,method:'GET'|'POST',validate:(value:unknown)=>value is T,input?:object):Promise<T>{
    const response=await fetcher(path,{method,credentials:'include',cache:'no-store',redirect:'error',headers:{Accept:'application/json',...(method==='POST'?{'Content-Type':'application/json'}:{})},...(input?{body:JSON.stringify(input)}:{})});
    let value:unknown;try{value=await response.json();}catch{throw Error('The Account response did not match its contract.');}
    if(!response.ok)throw Error(record(value)&&record(value.error)&&typeof value.error.message==='string'?value.error.message:'The security request was rejected.');
    if(!record(value)||!keys(value,['data'])||!validate(value.data))throw Error('The Account response did not match its contract.');
    return value.data;
  }
  return {enroll:()=>request('/api/v1/account/iam/mfa/enroll','POST',enrollment,{}),verify:input=>request('/api/v1/account/iam/mfa/verify','POST',verified,input),read:()=>request('/api/v1/account/security','GET',seed)};
}
