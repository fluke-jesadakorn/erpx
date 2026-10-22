import 'server-only';
import {requirePlatformContext} from '@erpx/server-identity_access/adapters/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';

/** Provider discovery is independent of Company purchases and assignments.
 * Each destination still invokes its original full provider authorization. */
export async function providerEntryOverview(){
 try{
  const actor=await requirePlatformContext();
  return {assignmentId:actor.assignmentId,name:actor.name,role:actor.role.replaceAll('_',' '),commercial:actor.iam?.permissions.includes('platform.commercial.read')===true};
 }catch(error){
  if(error instanceof Error&&error.message==='FORBIDDEN'||error instanceof ApiError&&error.status===403)return null;
  throw error;
 }
}
