import { afterEach, expect, it, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
const harness = vi.hoisted(() => ({ click: undefined as undefined | (() => void), submit: undefined as undefined | ((event: unknown) => void), refresh: vi.fn(), track: vi.fn(), completion: undefined as undefined | {form: HTMLFormElement; formEditVersion: string | null} }));
vi.mock('@erpx/ui/components/foundation', () => ({ Button: (props: {onClick: () => void}) => { harness.click = props.onClick; return null; }, InlineNotice: () => null, FormLayout: (props: {onSubmit: (event: unknown) => void}) => {harness.submit=props.onSubmit;return null;}, Field: () => null, Select: () => null, TextInput: () => null }));
vi.mock('@erpx/feature-shared/lib/destination-router', () => ({useRouter: () => ({refresh: harness.refresh})}));
vi.mock('@erpx/feature-finance/components/finance-draft-boundary', async () => { const actual = await vi.importActual<typeof import('@erpx/feature-finance/components/finance-draft-boundary')>('@erpx/feature-finance/components/finance-draft-boundary'); return {...actual, captureFinanceForm: () => harness.completion, useFinanceDraftCommandPending: () => harness.track}; });
import { NativeJournalReverse } from '../../../packages/feature-finance/src/components/native-journal-reverse';
import { NativeJournalPost } from '../../../packages/feature-finance/src/components/native-journal-post';
const context = {userId:'user',holdingId:'h',companyId:'c',branchId:null,departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'a',assignmentVersion:3,revision:7,role:'reviewer',assignmentScopeKind:'holding',modules:['finance'],permissions:['finance.read']};
afterEach(() => {vi.unstubAllGlobals();harness.refresh.mockReset();harness.track.mockReset();harness.completion=undefined;});
it('retries the original key and body after an uncertain post and retries only readback after a valid receipt', async () => {
 const node={isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}};
 const windowStub=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});
 vi.stubGlobal('window',windowStub);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[],documentElement:{dataset:{}}});
 const calls:{url:string;init:RequestInit}[]=[];
 vi.stubGlobal('fetch',vi.fn(async (url:RequestInfo|URL,init:RequestInit={})=>{calls.push({url:String(url),init});if(calls.length===1)throw Error('lost reply');if(calls.length===2)return Response.json({data:{id:'post-test',status:'posted',version:2}});if(calls.length===3)throw Error('readback unavailable');return Response.json({data:{journal:{id:'post-test',status:'posted',version:2}},context:{...context,expiresAt:9999999999999}});}));
 renderToStaticMarkup(createElement(NativeJournalPost,{url:'/api/v1/holdings/acme/finance/journals/post-test',id:'post-test',version:1,context,allowed:true}));
 const click=harness.click!;click();await vi.waitFor(()=>expect(calls).toHaveLength(1));await vi.waitFor(()=>expect(harness.track).toHaveBeenCalledTimes(1));
 // Wait for the first failed request to finish before the explicit retry.
 await new Promise(resolve=>setTimeout(resolve,0));click();await vi.waitFor(()=>expect(calls).toHaveLength(3));await new Promise(resolve=>setTimeout(resolve,0));click();await vi.waitFor(()=>expect(calls).toHaveLength(4));
 expect(calls.map(call=>call.init.method??'GET')).toEqual(['POST','POST','GET','GET']);
 expect(calls[0]!.init.body).toBe('{"expectedVersion":1}');expect(calls[1]!.init.body).toBe(calls[0]!.init.body);
 expect(new Headers(calls[0]!.init.headers).get('idempotency-key')).toBe(new Headers(calls[1]!.init.headers).get('idempotency-key'));
 expect(new Headers(calls[1]!.init.headers).get('x-erpx-context-assignment-version')).toBe('3');
 await vi.waitFor(()=>expect(harness.refresh).toHaveBeenCalledOnce());expect(harness.track.mock.calls.at(-1)?.[2]).toBe(false);
});
it('recovers an uncertain request after a same-context drawer remount and refuses a changed context', async () => {
 const boundary=()=>({isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}});
 let node=boundary();const windowStub=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});
 vi.stubGlobal('window',windowStub);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[],documentElement:{dataset:{}}});
 const calls:RequestInit[]=[];vi.stubGlobal('fetch',vi.fn(async (_url:RequestInfo|URL,init:RequestInit={})=>{calls.push(init);if(calls.length===1)throw Error('lost reply');if(calls.length===2)return Response.json({data:{id:'remount-test',status:'posted',version:2}});return Response.json({data:{journal:{id:'remount-test',status:'posted',version:2}},context:{...context,expiresAt:9999999999999}});}));
 const render=(value= context)=>{renderToStaticMarkup(createElement(NativeJournalPost,{url:'/api/v1/holdings/acme/finance/journals/remount-test',id:'remount-test',version:1,context:value,allowed:true}));return harness.click!;};
 render()();await vi.waitFor(()=>expect(calls).toHaveLength(1));await new Promise(resolve=>setTimeout(resolve,0));
 render({...context,role:'other'})();await new Promise(resolve=>setTimeout(resolve,0));expect(node.isConnected).toBe(true);expect(calls).toHaveLength(1);
 node.isConnected=false;node=boundary();
 render({...context,role:'other'})();await new Promise(resolve=>setTimeout(resolve,0));expect(calls).toHaveLength(1);
 render()();await vi.waitFor(()=>expect(calls).toHaveLength(3));expect(calls[0]!.body).toBe(calls[1]!.body);expect(new Headers(calls[0]!.headers).get('idempotency-key')).toBe(new Headers(calls[1]!.headers).get('idempotency-key'));
 await vi.waitFor(()=>expect(harness.refresh).toHaveBeenCalledOnce());
});

it('keeps a newer Finance form edit when a captured posting finishes', async () => {
 const node={isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}};
 let edit='original-edit', dirty=true, release!:()=>void, started!:()=>void;
 const gate=new Promise<void>(resolve=>release=resolve), readbackStarted=new Promise<void>(resolve=>started=resolve);
 const form={isConnected:true,getAttribute:(name:string)=>name==='data-erpx-edit-version'?edit:null} as HTMLFormElement;
 harness.completion={form,formEditVersion:edit};
 const windowStub=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});
 windowStub.addEventListener('erpx:finance-command-acknowledged',event=>{const detail=(event as CustomEvent<{form:HTMLFormElement;formEditVersion:string|null}>).detail;if(detail.form===form&&detail.formEditVersion===form.getAttribute('data-erpx-edit-version'))dirty=false;});
 vi.stubGlobal('window',windowStub);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[form],documentElement:{dataset:{}}});
 vi.stubGlobal('fetch',vi.fn(async (_url:RequestInfo|URL,init:RequestInit={})=>{if(init.method==='POST')return Response.json({data:{id:'draft-test',status:'posted',version:2}});started();await gate;return Response.json({data:{journal:{id:'draft-test',status:'posted',version:2}},context:{...context,expiresAt:9999999999999}});}));
 renderToStaticMarkup(createElement(NativeJournalPost,{url:'/api/v1/holdings/acme/finance/journals/draft-test',id:'draft-test',version:1,context,allowed:true}));
 harness.click!();await readbackStarted;edit='newer-edit';release();
 await vi.waitFor(()=>expect(harness.track.mock.calls.at(-1)?.[2]).toBe(false));
 expect(edit).toBe('newer-edit');expect(dirty).toBe(true);expect(harness.refresh).not.toHaveBeenCalled();
});

it('retains reversal input and key through a lost reply, then retries only both readbacks after the receipt', async () => {
 const node={isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}};
 let edit='original', dirty=true, readCount=0; const values={periodId:'period',entryDate:'2026-01-02',memo:''};
 const form={isConnected:true,getAttribute:()=>edit} as HTMLFormElement;
 const browser=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});
 browser.addEventListener('erpx:finance-command-acknowledged',event=>{const d=(event as CustomEvent<{form:HTMLFormElement;formEditVersion:string|null}>).detail;if(d.form===form&&d.formEditVersion===edit)dirty=false;});
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[form],documentElement:{dataset:{}}});
 vi.stubGlobal('FormData',class {get(name:keyof typeof values){return values[name]??null;}});
 const calls:{url:string;init:RequestInit}[]=[];let key='';
 vi.stubGlobal('fetch',vi.fn(async (url:RequestInfo|URL,init:RequestInit={})=>{calls.push({url:String(url),init});if(init.method==='POST'){key=new Headers(init.headers).get('idempotency-key')!;if(calls.length===1)throw Error('lost reply');return Response.json({data:{reversalJournalId:`reversal-${key}`,reversedJournalId:'reverse-test'}},{status:201});}readCount++;if(readCount===1)throw Error('readback unavailable');const child=String(url).includes(`reversal-${key}`);return Response.json({context:{...context,expiresAt:9999999999999},data:{journal:child?{id:`reversal-${key}`,status:'posted',version:1,source_type:'reversal',source_id:'reverse-test'}:{id:'reverse-test',status:'reversed',version:3}}});}));
 renderToStaticMarkup(createElement(NativeJournalReverse,{url:'/api/v1/holdings/acme/finance/journals/reverse-test',id:'reverse-test',version:2,context,allowed:true,periods:[]}));
 const submit=harness.submit!, event={preventDefault:vi.fn(),currentTarget:form};submit(event);await vi.waitFor(()=>expect(calls).toHaveLength(1));await new Promise(resolve=>setTimeout(resolve,0));values.memo='newer';edit='newer-edit';submit(event);await vi.waitFor(()=>expect(calls).toHaveLength(3));await new Promise(resolve=>setTimeout(resolve,0));submit(event);await vi.waitFor(()=>expect(calls).toHaveLength(5));
 expect(calls.map(c=>c.init.method??'GET')).toEqual(['POST','POST','GET','GET','GET']);expect(calls[0]!.init.body).toBe('{"periodId":"period","entryDate":"2026-01-02","memo":""}');expect(calls[1]!.init.body).toBe(calls[0]!.init.body);expect(new Headers(calls[0]!.init.headers).get('idempotency-key')).toBe(new Headers(calls[1]!.init.headers).get('idempotency-key'));await vi.waitFor(()=>expect(harness.track.mock.calls.at(-1)?.[2]).toBe(false));expect(edit).toBe('newer-edit');expect(dirty).toBe(true);expect(harness.refresh).not.toHaveBeenCalled();
});
