import 'server-only';
import {createNativeAccountHttpTransport,NativeSessionReadError} from '@erpx/native-sdk/session';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {nativeAccountConfiguration} from './native-session';
export async function nativeAccountHttp(request:Request):Promise<Response>{
 try{
  const raw=process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;
  if(!raw)throw new ApiError(503,'NATIVE_ACCOUNT_CONFIGURATION_REQUIRED','The native Account runtime is not configured.');
  return await createNativeAccountHttpTransport(nativeAccountConfiguration(raw))(request);
 }catch(error){
  if(error instanceof NativeSessionReadError)return errorResponse(new ApiError(error.status??503,error.code,'The native Account request could not be completed.'));
  return errorResponse(error);
 }
}
