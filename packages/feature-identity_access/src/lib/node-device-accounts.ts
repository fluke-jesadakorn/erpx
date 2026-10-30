'use client';
import { developmentProductOrigin } from '@erpx/contracts/product-registry';
import type { DeviceAccount, DeviceAccounts } from './device-accounts';

type Result = { data: DeviceAccounts | null; error: { code: string; message: string } | null };
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const string = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && value.length <= 4096;
function accounts(value: unknown): value is DeviceAccounts {
  if (!object(value) || Object.keys(value).sort().join(',') !== 'accounts,initialized,maximum,revision' || typeof value.initialized !== 'boolean' || !Number.isSafeInteger(value.revision) || (value.revision as number) < 0 || value.maximum !== 5 || !Array.isArray(value.accounts) || value.accounts.length > 5) return false;
  return value.accounts.every((item: unknown): item is DeviceAccount => object(item) && string(item.handle) && string(item.name) && string(item.email) && (item.image === null || string(item.image)) && typeof item.current === 'boolean' && Object.keys(item).every(k => ['handle','name','email','image','current'].includes(k)));
}
/** Opaque handles only; the Account host owns the credential and current revision. */
async function request(path: string, method: 'GET' | 'POST', body?: object): Promise<Result> {
  try {
    const response = await fetch(new URL('/api/v1/account/device-accounts' + path, developmentProductOrigin('account')), { method, credentials:'include', cache:'no-store', headers:{ accept:'application/json', ...(body ? {'content-type':'application/json'} : {}) }, ...(body ? {body:JSON.stringify(body)} : {}) });
    const value: unknown = await response.json();
    if (!response.ok) return {data:null,error:object(value) && object(value.error) && string(value.error.code) && string(value.error.message) ? {code:value.error.code,message:value.error.message} : {code:'ACCOUNT_REQUEST_FAILED',message:'Account request failed.'}};
    if (!object(value) || Object.keys(value).length !== 1 || !accounts(value.data)) return {data:null,error:{code:'ACCOUNT_RESPONSE_INVALID',message:'Account response did not match its contract.'}};
    return {data:value.data,error:null};
  } catch { return {data:null,error:{code:'ACCOUNT_NETWORK_UNAVAILABLE',message:'Could not connect to Account.'}}; }
}
export const nodeDeviceAccounts = {
  list: () => request('', 'GET'),
  activate: (handle: string, revision: number) => request('/active', 'POST', {handle,revision}),
  signIn: (email: string, password: string, revision?: number) => request('/sign-in', 'POST', {email,password,...(revision === undefined ? {} : {revision})}),
};
