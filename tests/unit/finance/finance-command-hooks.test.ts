import {afterEach,expect,it,vi} from 'vitest';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
const router=vi.hoisted(()=>({refresh:vi.fn()}));
vi.mock('@/lib/shared/destination-router',()=>({useRouter:()=>router}));
import {useFinanceCommand} from '@/components/finance/use-finance-command';
import {useRefundCommand} from '@/components/finance/use-refund-command';

afterEach(()=>{vi.unstubAllGlobals();router.refresh.mockReset();});
type Kind='finance'|'refund';
/** Real React SSR supplies the checked-in hook and its closure. Only the
 * browser/network/router boundary is controlled; no hook/state replacement. */
function command(kind:Kind){
 let run:((input:unknown)=>Promise<unknown>)|undefined;
 function ObserveFinance(){const owner=useFinanceCommand('/holdings/acme');run=input=>owner.send('budgets',input);return createElement('span',null,'Actual Finance hook');}
 function ObserveRefund(){const owner=useRefundCommand('/holdings/acme',['finance.banking'],{ownerId:'actual-refund',instructionIds:[]});run=input=>owner.run('refunds/actual-refund/approve',input);return createElement('span',null,'Actual refund hook');}
 expect(renderToStaticMarkup(createElement(kind==='finance'?ObserveFinance:ObserveRefund))).toContain('Actual');
 if(!run)throw Error('The real React hook must expose its actual command.');
 return run;
}
function boundary(){
 const node={isConnected:true,dataset:{workUser:'reviewer',workRevision:'7',workCompany:'actual-company',workAssignment:'actual-finance-role',workStale:'false'}};
 let edit='submitted-edit',present=true,formPresent=true;
 const form={isConnected:true,getAttribute:(_name:string)=>edit};
 const browser=Object.assign(new EventTarget(),{location:{origin:'https://finance-dev-erpx.jesadakorn.com'}}),events:CustomEvent[]=[];
 for(const type of ['erpx:finance-command-acknowledged','erpx:record-command-saved','erpx:work-changed'])browser.addEventListener(type,event=>events.push(event as CustomEvent));
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{activeElement:{closest:()=>formPresent?form:null},querySelector:(selector:string)=>selector==='[data-work-revision]'&&present?node:null});
 return {node,form,events,setEdit:(value:string)=>{edit=value;},setFormPresent:(value:boolean)=>{formPresent=value;},remove:()=>{node.isConnected=false;present=false;},restore:()=>{node.isConnected=true;present=true;node.dataset.workRevision='7';node.dataset.workStale='false';}};
}

it.each(['finance','refund'] as const)('retains the original %s hook completion and newer draft across an exact lost-response retry',async kind=>{
 const scope=boundary(),run=command(kind),input={expectedVersion:1,reason:'Original captured review '+crypto.randomUUID()},calls:RequestInit[]=[];
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);if(calls.length===1)throw Error('Lost original response after commit');return Response.json({data:{id:'actual-reviewed-command',version:2}});}));
 expect(await run(input)).toBeNull();scope.setEdit('newer-local-draft');
 expect(await run(input)).toBeNull();
 expect(router.refresh).not.toHaveBeenCalled();
 expect(scope.events.filter(event=>event.type!=='erpx:work-changed').map(event=>event.detail.formEditVersion)).toEqual(['submitted-edit','submitted-edit']);
 expect(scope.form.getAttribute('data-erpx-edit-version')).toBe('newer-local-draft');
 expect(calls).toHaveLength(2);expect(calls.map(call=>call.body)).toEqual([JSON.stringify(input),JSON.stringify(input)]);
 expect(new Headers(calls[0].headers).get('Idempotency-Key')).toBe(new Headers(calls[1].headers).get('Idempotency-Key'));
});

it.each(['finance','refund'] as const)('retains a later unrelated form on an exact %s retry originally outside a form',async kind=>{
 const scope=boundary();scope.setFormPresent(false);const run=command(kind),input={reason:'Outside original form '+crypto.randomUUID()},calls:RequestInit[]=[];
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);if(calls.length===1)throw Error('Lost original no-form response');return Response.json({data:{id:'outside-form-command'}});}));
 expect(await run(input)).toBeNull();scope.setFormPresent(true);scope.setEdit('new-later-form');
 expect(await run(input)).toBeNull();expect(router.refresh).not.toHaveBeenCalled();expect(scope.events).toEqual([]);
 expect(scope.form.getAttribute('data-erpx-edit-version')).toBe('new-later-form');
 scope.setFormPresent(false);expect(await run(input)).toEqual({id:'outside-form-command'});expect(router.refresh).toHaveBeenCalledOnce();
 expect(scope.events.map(event=>event.type)).toEqual(['erpx:work-changed']);
 expect(calls.map(call=>call.body)).toEqual(Array(3).fill(JSON.stringify(input)));expect(new Set(calls.map(call=>new Headers(call.headers).get('Idempotency-Key'))).size).toBe(1);
});

it.each((['finance','refund'] as const).flatMap(kind=>(['revision','stale','removed'] as const).map(changed=>({kind,changed}))))('blocks $kind local success when the boundary becomes $changed after helper receipt decoding but during the hook original JSON body',async({kind,changed})=>{
 const scope=boundary(),run=command(kind),input={expectedVersion:1,reason:'Captured native JSON body '+crypto.randomUUID()},data={id:'actual-reviewed-command',version:2},calls:RequestInit[]=[];
 let signal!:()=>void,release!:()=>void;
 const started=new Promise<void>(resolve=>{signal=resolve;}),resume=new Promise<void>(resolve=>{release=resolve;});
 class LazyOriginalResponse extends Response{override async json(){const value=await super.json();signal();await resume;return value;}}
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);return calls.length===1?new LazyOriginalResponse(JSON.stringify({data}),{headers:{'content-type':'application/json'}}):Response.json({data});}));
 const result=run(input);await started;
 // The helper's clone was fully decoded while current. In caller mode, no
 // receipt/command identity may be cleared until this hook finishes too.
 const priorEvents=scope.events.slice();
 if(changed==='revision')scope.node.dataset.workRevision='8';else if(changed==='stale')scope.node.dataset.workStale='true';else scope.remove();
 release();expect(await result).toBeNull();expect(router.refresh).not.toHaveBeenCalled();expect(scope.events).toEqual(priorEvents);expect(priorEvents).toEqual([]);
 scope.restore();expect(await run(input)).toEqual(data);expect(router.refresh).toHaveBeenCalledOnce();
 expect(calls.map(call=>call.body)).toEqual([JSON.stringify(input),JSON.stringify(input)]);expect(new Headers(calls[0].headers).get('Idempotency-Key')).toBe(new Headers(calls[1].headers).get('Idempotency-Key'));
 expect(scope.events.filter(event=>event.type!=='erpx:work-changed').map(event=>event.detail.formEditVersion)).toEqual(['submitted-edit','submitted-edit']);
});

it.each(['finance','refund'] as const)('keeps unchanged %s hook parsed-success behavior behind its normal final context',async kind=>{
 const scope=boundary(),run=command(kind),data={id:'actual-reviewed-command',version:2};
 vi.stubGlobal('fetch',vi.fn(async()=>Response.json({data})));
 expect(await run({expectedVersion:1,reason:'Unchanged original '+crypto.randomUUID()})).toEqual(data);
 expect(router.refresh).toHaveBeenCalledOnce();expect(scope.events.filter(event=>event.type!=='erpx:work-changed').every(event=>event.detail.formEditVersion==='submitted-edit')).toBe(true);
});
