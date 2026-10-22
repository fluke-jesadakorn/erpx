import {afterEach,expect,it,vi} from 'vitest';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
const pendingTracker=vi.hoisted(()=>({track:(token:object,completion:unknown,pending:boolean):void=>{void token;void completion;void pending;}}));
vi.mock("@erpx/feature-finance/components/finance-draft-boundary",async()=>{
 const actual=await vi.importActual<typeof import("@erpx/feature-finance/components/finance-draft-boundary")>("@erpx/feature-finance/components/finance-draft-boundary");
 return {...actual,useFinanceDraftCommandPending:()=>pendingTracker.track};
});
const router=vi.hoisted(()=>({refresh:vi.fn()}));
vi.mock("@erpx/feature-shared/lib/destination-router",()=>({useRouter:()=>router}));
import {contextSwitchBlockReason,registerContextSwitchBlocker} from '@erpx/feature-work_projection/lib/context-switch';
import {useFinanceCommand} from '@erpx/feature-finance/components/use-finance-command';
import {useRefundCommand} from '@erpx/feature-finance/components/use-refund-command';

const pendingBlockers:(()=>void)[]=[];
afterEach(()=>{for(const unregister of pendingBlockers.splice(0))unregister();pendingTracker.track=()=>{};vi.unstubAllGlobals();router.refresh.mockReset();});
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
 const pendingCommands=new Set<object>();
 pendingTracker.track=(token,_completion,pending)=>{if(pending)pendingCommands.add(token);else pendingCommands.delete(token);};
 pendingBlockers.push(registerContextSwitchBlocker(()=>pendingCommands.size?'An original Finance request is awaiting confirmation. Return to Finance and retry it before switching account, Company, or role.':null));
 const form={isConnected:true,getAttribute:(name:string)=>name==='data-erpx-edit-version'?edit:null};
 const browser=Object.assign(new EventTarget(),{location:{origin:'https://finance-dev-erpx.jesadakorn.com'}}),events:CustomEvent[]=[];
 for(const type of ['erpx:finance-command-acknowledged','erpx:record-command-saved','erpx:work-changed'])browser.addEventListener(type,event=>events.push(event as CustomEvent));
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{activeElement:{closest:()=>formPresent?form:null},querySelector:(selector:string)=>selector==='[data-work-revision]'&&present?node:null});
 return {node,form,events,contextSwitchBlockReason,setEdit:(value:string)=>{edit=value;},setFormPresent:(value:boolean)=>{formPresent=value;},remove:()=>{node.isConnected=false;present=false;},restore:()=>{node.isConnected=true;present=true;node.dataset.workRevision='7';node.dataset.workStale='false';}};
}

it.each(['finance','refund'] as const)('retains the original %s hook completion and newer draft across an exact lost-response retry',async kind=>{
 const scope=boundary(),run=command(kind),input={expectedVersion:1,reason:'Original captured review '+crypto.randomUUID()},calls:RequestInit[]=[];
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);if(calls.length===1)throw Error('Lost original response after commit');return Response.json({data:{id:'actual-reviewed-command',version:2}});}));
 expect(await run(input)).toBeNull();expect(scope.contextSwitchBlockReason()).toContain('original Finance request is awaiting confirmation');scope.setEdit('newer-local-draft');
 expect(await run(input)).toBeNull();
 expect(scope.contextSwitchBlockReason()).toBeNull();
 expect(router.refresh).not.toHaveBeenCalled();
 expect(scope.events.filter(event=>event.type!=='erpx:work-changed').map(event=>event.detail.formEditVersion)).toEqual(['submitted-edit','submitted-edit']);
 expect(scope.form.getAttribute('data-erpx-edit-version')).toBe('newer-local-draft');
 expect(calls).toHaveLength(2);expect(calls.map(call=>call.body)).toEqual([JSON.stringify(input),JSON.stringify(input)]);
 expect(new Headers(calls[0].headers).get('Idempotency-Key')).toBe(new Headers(calls[1].headers).get('Idempotency-Key'));
});

it('blocks a different Finance payload until the uncertain original is acknowledged, then releases the blocker',async()=>{
 const scope=boundary(),run=command('finance'),original={expectedVersion:1,reason:'Original uncertain Finance action '+crypto.randomUUID()},different={expectedVersion:2,reason:'Different later Finance action '+crypto.randomUUID()},calls:RequestInit[]=[];
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);if(calls.length===1)throw Error('Lost original response after commit');return Response.json({data:{id:`actual-finance-command-${calls.length}`}});}));
 expect(await run(original)).toBeNull();expect(scope.contextSwitchBlockReason()).toContain('original Finance request is awaiting confirmation');
 expect(await run(different)).toBeNull();expect(calls).toHaveLength(1);expect(scope.contextSwitchBlockReason()).toContain('original Finance request is awaiting confirmation');
 expect(await run(original)).toEqual({id:'actual-finance-command-2'});expect(scope.contextSwitchBlockReason()).toBeNull();
 expect(await run(different)).toEqual({id:'actual-finance-command-3'});expect(calls).toHaveLength(3);
 expect(calls.map(call=>call.body)).toEqual([JSON.stringify(original),JSON.stringify(original),JSON.stringify(different)]);
 const keys=calls.map(call=>new Headers(call.headers).get('Idempotency-Key'));expect(keys[0]).toBe(keys[1]);expect(keys[2]).not.toBe(keys[0]);
});

it.each(['finance','refund'] as const)('keeps the %s context switch blocked after an uncertain retry refusal until a valid acknowledgement',async kind=>{
 const scope=boundary(),run=command(kind),input={expectedVersion:1,reason:'Retained uncertain command '+crypto.randomUUID()},calls:RequestInit[]=[];
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);if(calls.length===1)throw Error('Lost original response after commit');if(calls.length===2)return Response.json({error:{code:'CURRENT_AUTHORITY_UNAVAILABLE',message:'Retry could not confirm the original command.'}},{status:409});return Response.json({data:{id:'actual-reviewed-command',version:2}});}));
 expect(await run(input)).toBeNull();expect(scope.contextSwitchBlockReason()).toContain('original Finance request is awaiting confirmation');
 expect(await run(input)).toBeNull();expect(scope.contextSwitchBlockReason()).toContain('original Finance request is awaiting confirmation');
 expect(await run(input)).toEqual({id:'actual-reviewed-command',version:2});expect(scope.contextSwitchBlockReason()).toBeNull();
 expect(calls).toHaveLength(3);expect(calls.map(call=>call.body)).toEqual(Array(3).fill(JSON.stringify(input)));expect(new Set(calls.map(call=>new Headers(call.headers).get('Idempotency-Key'))).size).toBe(1);
});

it.each(['finance','refund'] as const)('releases the %s context blocker after an initial definitive refusal',async kind=>{
 const scope=boundary(),run=command(kind),input={expectedVersion:1,reason:'Definitive refusal '+crypto.randomUUID()};
 vi.stubGlobal('fetch',vi.fn(async()=>Response.json({error:{code:'PACKAGE_WORKFLOW_UNAVAILABLE',message:'This package cannot run the selected command.'}},{status:403})));
 expect(await run(input)).toBeNull();expect(scope.contextSwitchBlockReason()).toBeNull();
});

it.each(['finance','refund'] as const)('retains a later unrelated form on an exact %s retry originally outside a form',async kind=>{
 const scope=boundary();scope.setFormPresent(false);const run=command(kind),input={reason:'Outside original form '+crypto.randomUUID()},calls:RequestInit[]=[];
 vi.stubGlobal('fetch',vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);if(calls.length===1)throw Error('Lost original no-form response');return Response.json({data:{id:'outside-form-command'}});}));
 expect(await run(input)).toBeNull();expect(scope.contextSwitchBlockReason()).toContain('original Finance request is awaiting confirmation');scope.setFormPresent(true);scope.setEdit('new-later-form');
 expect(await run(input)).toBeNull();expect(scope.contextSwitchBlockReason()).toContain('original Finance request is awaiting confirmation');expect(router.refresh).not.toHaveBeenCalled();expect(scope.events).toEqual([]);
 expect(scope.form.getAttribute('data-erpx-edit-version')).toBe('new-later-form');
 scope.setFormPresent(false);expect(await run(input)).toEqual({id:'outside-form-command'});expect(scope.contextSwitchBlockReason()).toBeNull();expect(router.refresh).toHaveBeenCalledOnce();
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

it('keeps a source-owned invoice posting on its exact path until original confirmation and retains newer edits',async()=>{
 const ts=await import('typescript'),{readFile}=await import('node:fs/promises'),{resolve}=await import('node:path'),source=await readFile(resolve('packages/feature-finance/src/components/invoice-controls.tsx'),'utf8'),ast=ts.createSourceFile('invoice-controls.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),declaration=ast.statements.find(statement=>ts.isFunctionDeclaration(statement)&&statement.name?.text==='useInvoiceCommand');
 if(!declaration)throw Error('ACTUAL_INVOICE_COMMAND_HOOK_REQUIRED');
 const compiled=ts.transpileModule(declaration.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,cells:{value:unknown}[]=[],form={isConnected:true,getAttribute:(name:string)=>name==='data-erpx-edit-version'?completionEdit:null},completion={form,formEditVersion:'submitted-invoice-edit'},calls:{url:RequestInfo|URL;init:RequestInit|undefined;completion:unknown;mode:unknown}[]=[],acknowledge=vi.fn(),announce=vi.fn(),trackPending=vi.fn(),invoiceRouter={push:vi.fn(),refresh:vi.fn()};let completionEdit='submitted-invoice-edit',hookIndex=0;
 const useState=(initial:unknown)=>{const index=hookIndex++;cells[index]??={value:initial};return[cells[index].value,(value:unknown)=>{cells[index].value=typeof value==='function'?Reflect.apply(value,undefined,[cells[index].value]):value;}];},useRef=(initial:unknown)=>{const index=hookIndex++;cells[index]??={value:{current:initial}};return cells[index].value;};
 const workFetch=vi.fn(async(url:RequestInfo|URL,init?:RequestInit,receivedCompletion?:unknown,mode?:unknown)=>{calls.push({url,init,completion:receivedCompletion,mode});if(calls.length===1)throw Error('Original response was lost after commit');if(calls.length===2)return Response.json({error:{code:'WORK_ROLE_UNAVAILABLE',message:'The original role is no longer available.'}},{status:409});return Response.json({data:{id:'original-invoice'}});});
 const build=new Function('useFinanceActionAllowed','useRouter','useRef','useState','useFinanceDraftCommandPending','financeDocumentCommandContext','financeDocumentCommandAllowed','financeDocumentFormUnchanged','captureFinanceForm','acknowledgeFinanceForm','announceWorkChange','workFetch','canonicalUiHref',`${compiled}\nreturn useInvoiceCommand;`) as (...ports:unknown[])=>(base:string,purpose:'new-work'|'settle-existing',post?:{path:string;canPost:boolean})=>Record<string,unknown>;
 const useInvoiceCommand=build(()=>false,()=>invoiceRouter,useRef,useState,()=>trackPending,()=> 'company:revision:assignment',(allowed:boolean,path:string,post?:{path:string;canPost:boolean})=>post?.path===path?post.canPost:allowed,(captured:typeof completion)=>captured.form.isConnected&&captured.formEditVersion===captured.form.getAttribute('data-erpx-edit-version'),()=>completion,acknowledge,announce,workFetch,(href:string)=>href),post={path:'/invoice-owned/post',canPost:true};
 function InvoiceCommandHarness(){return useInvoiceCommand('/holdings/acme','settle-existing',post) as {send:(path:string,input:unknown)=>Promise<boolean>;retryOriginal:()=>Promise<boolean>;unresolved:boolean;canSend:boolean;notice:string;error:string};}
 const render=()=>{hookIndex=0;return InvoiceCommandHarness();};
 const original={kind:'receivable',invoiceNumber:'INV-ORIGINAL',subtotal:'100.00'},newer={...original,invoiceNumber:'INV-NEWER'};
 try{
  expect(await render().send('/invoice-owned/post',original)).toBe(false);
  let owner=render();expect(owner.unresolved).toBe(true);expect(owner.canSend).toBe(false);
  expect(await owner.send('/invoice-owned/post',newer)).toBe(false);expect(workFetch).toHaveBeenCalledOnce();
  owner=render();expect(await owner.retryOriginal()).toBe(false);owner=render();expect(owner.unresolved).toBe(true);expect(owner.canSend).toBe(false);
  completionEdit='newer-invoice-edit';owner=render();expect(await owner.retryOriginal()).toBe(false);
  owner=render();expect(owner.unresolved).toBe(false);expect(owner.notice).toContain('newer draft is retained');
  expect(await owner.send('/another-invoice/post',newer)).toBe(false);expect(calls).toHaveLength(3);expect(new Set(calls.map(call=>call.url)).size).toBe(1);expect(calls.map(call=>call.init?.body)).toEqual([JSON.stringify(original),JSON.stringify(original),JSON.stringify(original)]);
  expect(new Set(calls.map(call=>new Headers(call.init?.headers).get('Idempotency-Key'))).size).toBe(1);
  expect(calls.map(call=>call.url)).toEqual(Array(3).fill('/api/v1/holdings/acme/finance/invoices/invoice-owned/post'));expect(calls.map(call=>call.completion)).toEqual([completion,completion,completion]);expect(calls.every(call=>call.mode==='caller')).toBe(true);expect(trackPending.mock.calls.map(call=>call[2])).toEqual([true,false]);
  expect(acknowledge).toHaveBeenCalledExactlyOnceWith(completion);expect(announce).toHaveBeenCalledOnce();expect(invoiceRouter.push).not.toHaveBeenCalled();expect(invoiceRouter.refresh).not.toHaveBeenCalled();
 }finally{vi.unstubAllGlobals();}
});
