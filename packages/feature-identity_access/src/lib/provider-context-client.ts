import {providerContextSeed,providerContextSelection,providerContextAcknowledgement,type ProviderContextSeed,type ProviderContextSelection} from '@erpx/feature-identity_access/lib/provider-context';
export type ProviderContextState={seed:ProviderContextSeed;pending:boolean;retry:boolean;stale:boolean;error:string};
export const providerContextBlocked=(state:Pick<ProviderContextState,'stale'|'retry'|'pending'>)=>state.stale||state.retry||state.pending;
type Ports={read:(signal:AbortSignal)=>Promise<unknown>;write:(input:ProviderContextSelection,key:string,signal:AbortSignal)=>Promise<unknown>;key:()=>string;generation:()=>number};
/** SSR observation does no I/O. A failed or unverified acknowledgement keeps the
 * exact captured body/key. Focus/peer reads never replay a selection themselves. */
export function createProviderContextClient(initial:ProviderContextSeed,ports:Ports){
 let state:ProviderContextState={seed:providerContextSeed(initial),pending:false,retry:false,stale:false,error:''},capture:{input:ProviderContextSelection;key:string}|null=null,flight:AbortController|null=null,epoch=0,disposed=false;
 const listeners=new Set<()=>void>(),emit=(patch:Partial<ProviderContextState>)=>{state={...state,...patch};for(const listener of listeners)listener();};
 async function send(){
  if(!capture||flight||disposed)throw Error('The Platform selection is unavailable.');const submitted=capture,signal=new AbortController(),ticket=++epoch,generation=ports.generation();flight=signal;emit({pending:true,error:''});
  try{const value=await ports.write(submitted.input,submitted.key,signal.signal);if(disposed||signal.signal.aborted||ticket!==epoch||generation!==ports.generation())throw Error('The account context changed while confirming this selection.');const seed=providerContextAcknowledgement(value,submitted.input);capture=null;emit({seed,retry:false,stale:false});}
  catch(error){if(!disposed&&ticket===epoch&&generation===ports.generation()){const status=Number((error as {status?:number})?.status),retry=![401,403,404,409,422].includes(status);if(!retry)capture=null;emit({retry,stale:!retry,error:error instanceof Error?error.message:'The Platform role could not be selected.'});}throw error;}
  finally{if(flight===signal){flight=null;if(!disposed)emit({pending:false});}}
 }
 return {getSnapshot:()=>state,subscribe(listener:()=>void){listeners.add(listener);return()=>{listeners.delete(listener);};},
  observe(seed:ProviderContextSeed){if(disposed||seed.accountKey!==state.seed.accountKey||state.pending||seed.revision<state.seed.revision)return;if(seed.contextKey!==state.seed.contextKey){++epoch;flight?.abort();flight=null;}emit({seed:providerContextSeed(seed),stale:false,error:capture?state.error:''});},
  async select(id:string){if(capture)throw Error('Retry or review the captured Platform selection first.');flight?.abort();flight=null;capture={input:providerContextSelection(state.seed,id),key:ports.key()};return send();},
  retry:send,
  async refresh(){if(disposed||flight||capture)return;const signal=new AbortController(),ticket=++epoch,generation=ports.generation();flight=signal;try{const seed=providerContextSeed(await ports.read(signal.signal));if(disposed||signal.signal.aborted||ticket!==epoch||generation!==ports.generation())return;if(seed.accountKey!==state.seed.accountKey){emit({stale:true,error:'Your account changed. Reload Administration.'});return;}if(seed.revision<state.seed.revision)return;const changed=seed.contextKey!==state.seed.contextKey;emit({seed,stale:state.stale||changed,error:changed?'The Platform context changed. Review unsaved work before reloading.':state.error});}catch(error){if(!disposed&&ticket===epoch&&!signal.signal.aborted)emit({error:error instanceof Error?error.message:'Could not refresh the Platform context.',stale:true});}finally{if(flight===signal)flight=null;}},
  activate(){if(!disposed)return;disposed=false;++epoch;emit({pending:false,retry:!!capture,...(capture?{error:'The role selection was interrupted. Retry the captured request.'}:{})});},
  dispose(){disposed=true;++epoch;flight?.abort();flight=null;listeners.clear();},
 };
}
