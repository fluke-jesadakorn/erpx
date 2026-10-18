import type {PreferenceRefreshOutcome} from './account-preferences-client';
type Language='en'|'th';
/** document.lang may already reflect a client preference while Server
 * Components still show the old language. Track that server receipt separately. */
export function createAccountLanguageRefresh(initial:Language,ports:{active:()=>boolean;dirty:()=>boolean;refresh:()=>void}){
 let rendered=initial,wanted=initial,requested:Language|null=null;
 const flush=()=>{if(wanted===rendered){requested=null;return;}if(!ports.active()||ports.dirty()||requested===wanted)return;requested=wanted;ports.refresh();};
 return {apply(language:Language){wanted=language;flush();},observe(language:Language){rendered=language;if(requested===language)requested=null;},flush,pending:()=>wanted!==rendered,cancel(){wanted=rendered;requested=null;}};
}
/** Cross-origin windows cannot share BroadcastChannel. Revalidate only their
 * own authenticated origin after a quiet 30s interval; no credentials or
 * preference content cross an origin boundary. Matching seeds do no mount I/O. */
export function createAccountPreferenceRefresh(ports:{visible:()=>boolean;active:()=>boolean;read:()=>Promise<PreferenceRefreshOutcome>;setTimer:(run:()=>void,delay:number)=>number;clearTimer:(id:number)=>void}){
 let disposed=false,blocked=false,failures=0,timer:number|null=null,running=false;
 const clear=()=>{if(timer!==null)ports.clearTimer(timer);timer=null;};
 const arm=()=>{clear();if(disposed||blocked||!ports.visible()||!ports.active())return;timer=ports.setTimer(()=>{timer=null;void read();},Math.min(300000,30000*2**Math.min(failures,4)));};
 const read=async()=>{if(disposed||blocked||running||!ports.visible()||!ports.active())return;clear();running=true;
  try{const result=await ports.read();if(disposed)return;if(result==='auth'||result==='context'){blocked=true;}else if(result==='error'){failures++;}else if(result==='ok'){failures=0;}}
  catch{if(!disposed)failures++;}
  finally{running=false;arm();}
 };
 return {start:arm,peer:()=>failures?Promise.resolve():read(),focus(){if(disposed||!ports.visible()||!ports.active())return;blocked=false;failures=0;return read();},hidden:clear,cancel(){blocked=true;clear();},dispose(){disposed=true;clear();}};
}
