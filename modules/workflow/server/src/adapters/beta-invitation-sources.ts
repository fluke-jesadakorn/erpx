import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeWorkflowWrite} from './native-workflow-write';
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';

export function betaInvitationSources(db:SqlDatabase){return {prepareAcceptanceAudit(value:string,key:string,id:string,value2:string,id2:string,now:number){return nativeWorkflowWrite('wf_native_beta_invitation_sources_2', [value, key, id, value2, id2, now,]);},
clearAcceptanceAssertions(key:string,companyId:string){return nativeClearCommandAssertions(db,companyId,key);}};}
