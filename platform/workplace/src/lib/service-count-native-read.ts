import 'server-only';
import {nativeInitialCount,nativeInitialCountContext,NativeInitialCountError} from '@erpx/field-service-ui/public/native-initial-count';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import type {CountGenericDetail} from '@erpx/platform-contracts/field-service-initial-count';
import type {FlowView} from '@erpx/platform-shared-ui/lib/flow';

/** Service source ownership must already have been selected from persisted data. */
export function serviceCountRead(headers:Pick<Headers,'get'>,actor:WorkContext,slug:string,route:{kind:'flow';id:string}):Promise<FlowView>;
export function serviceCountRead(headers:Pick<Headers,'get'>,actor:WorkContext,slug:string,route:{kind:'generic-detail';id:string}):Promise<CountGenericDetail>;
export async function serviceCountRead(headers:Pick<Headers,'get'>,actor:WorkContext,slug:string,route:{kind:'flow'|'generic-detail';id:string}):Promise<FlowView|CountGenericDetail> {
 const context=nativeInitialCountContext(actor);
 if(!context)throw new ApiError(409,'WORK_CONTEXT_CHANGED','The work context changed.');
 try {
  const data=route.kind==='flow'?await nativeInitialCount(headers,slug,context,{kind:'flow',id:route.id}):await nativeInitialCount(headers,slug,context,{kind:'generic-detail',id:route.id});
  if(!data)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');
  return data;
 } catch(error) {
  if(error instanceof NativeInitialCountError)throw new ApiError(error.status,error.code,'Service initial physical stock is unavailable.');
  throw error;
 }
}
