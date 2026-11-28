import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeWorkflowWrite} from './native-workflow-write';
export function assertion(db:SqlDatabase,id:string,step:string,query:string,args:(string|number|null)[]=[]){return nativeWorkflowWrite('wf_native_command_assertions_3', [id,step,...args, query]);}
