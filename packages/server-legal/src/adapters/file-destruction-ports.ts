import 'server-only';
import type {SqlDatabase,FileDestructionAuthority} from '@erpx/server-runtime/public/runtime/types';
import {runtimePreservationAuthority} from '@erpx/server-legal/adapters/runtime-preservation-authority';
import {runtimeMinioPreservationAuthority} from '@erpx/server-legal/adapters/runtime-minio-preservation-authority';
/** Composition stays with the Legal deletion owner, scoped to its exact unit. */
export function fileDestructionAuthority(database:SqlDatabase,destructionId:string):FileDestructionAuthority{
 const bound=(target:SqlDatabase,id:string)=>{if(target!==database||id!==destructionId)throw new Error('LEGAL_DESTRUCTION_NOT_AUTHORIZED');};
 return Object.freeze({
  assertLocal:async(target:SqlDatabase,id:string,keys:string[])=>{bound(target,id);await runtimePreservationAuthority(database).assertDestruction(keys,id);},
  readRemote:async(target:SqlDatabase,id:string)=>{bound(target,id);return runtimeMinioPreservationAuthority(database).destruction(id);}
 });
}
