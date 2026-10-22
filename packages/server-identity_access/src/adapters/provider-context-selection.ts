import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** A stored selection is a hint only. The normal Platform reader still loads
 * the user's current eligible assignments and original IAM authority. */
export async function providerContextSelectedAssignment(database:SqlDatabase,userId:string,sessionId:string){
 try{return await database.prepare('SELECT assignment_id FROM admin_provider_contexts WHERE session_id=? AND user_id=?').bind(sessionId,userId).first<string>('assignment_id');}
 catch(error){let current=error;for(let depth=0;depth<5&&current&&typeof current==='object';depth++){const cause=current as {code?:string;cause?:unknown};if(cause.code==='42P01')return null;current=cause.cause;}throw error;}
}
