import { afterEach, expect, it, vi } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
const harness = vi.hoisted(() => ({ click: undefined as undefined | (() => void), submit: undefined as undefined | ((event: unknown) => void), refresh: vi.fn(), push: vi.fn(), track: vi.fn(), completion: undefined as undefined | {form: HTMLFormElement; formEditVersion: string | null} }));
vi.mock('@erpx/ui/components/foundation', () => ({ Button: (props: {onClick: () => void}) => { harness.click = props.onClick; return null; }, InlineNotice: () => null, FormLayout: (props: {onSubmit: (event: unknown) => void}) => {harness.submit=props.onSubmit;return null;}, Field: () => null, Select: () => null, TextInput: () => null }));
vi.mock('@erpx/feature-shared/lib/destination-router', () => ({useRouter: () => ({refresh: harness.refresh,push:harness.push})}));
vi.mock('@erpx/feature-finance/components/finance-draft-boundary', async () => { const actual = await vi.importActual<typeof import('@erpx/feature-finance/components/finance-draft-boundary')>('@erpx/feature-finance/components/finance-draft-boundary'); return {...actual, captureFinanceForm: () => harness.completion, useFinanceDraftCommandPending: () => harness.track}; });
import { useNativeJournalCreate } from '../../../packages/feature-finance/src/components/use-native-journal-create';
import { NativeJournalReverse } from '../../../packages/feature-finance/src/components/native-journal-reverse';
import { NativeJournalPost } from '../../../packages/feature-finance/src/components/native-journal-post';
const context = {userId:'user',holdingId:'h',companyId:'c',branchId:null,departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'a',assignmentVersion:3,revision:7,role:'reviewer',assignmentScopeKind:'holding',modules:['finance'],permissions:['finance.read']};
afterEach(() => {vi.unstubAllGlobals();harness.refresh.mockReset();harness.push.mockReset();harness.track.mockReset();harness.completion=undefined;});
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
 const form={isConnected:true,getAttribute:(name:string)=>name==='data-erpx-edit-version'?edit:null} as unknown as HTMLFormElement;
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
 const form={isConnected:true,getAttribute:()=>edit} as unknown as HTMLFormElement;
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

it('retains original create body and key, and confirms native readback before navigating',async()=>{
 const node={isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}};
 let edit='initial',dirty=true;const form={isConnected:true,getAttribute:()=>edit} as unknown as HTMLFormElement;
 const browser=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});
 browser.addEventListener('erpx:finance-command-acknowledged',event=>{const d=(event as CustomEvent<{form:HTMLFormElement;formEditVersion:string|null}>).detail;if(d.form===form&&d.formEditVersion===edit)dirty=false;});
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[form],documentElement:{dataset:{}}});
 const calls:RequestInit[]=[];let key='';vi.stubGlobal('fetch',vi.fn(async (_url:RequestInfo|URL,init:RequestInit={})=>{calls.push(init);if(init.method==='POST'){key=new Headers(init.headers).get('idempotency-key')!;if(calls.length===1)throw Error('lost reply');return Response.json({data:{id:`journal-${key}`,number:`J-${key}`,status:'draft',version:1,debit:'1',credit:'1',fxRateMicros:1000000,baseCurrency:'THB'}},{status:201});}if(calls.length===3)throw Error('readback unavailable');return Response.json({context:{...context,expiresAt:9999999999999},data:{journal:{id:`journal-${key}`,source_type:'manual',status:'draft',version:1}}});}));
 let send!:ReturnType<typeof useNativeJournalCreate>['send'];function Observe(){send=useNativeJournalCreate('/holdings/acme',context,true).send;return null;}renderToStaticMarkup(createElement(Observe));
 const input={chartId:'chart',periodId:'period',entryDate:'2026-01-02',currency:'THB',memo:'original',sourceType:'manual',lines:[{accountId:'a',debit:'1',credit:'0'},{accountId:'b',debit:'0',credit:'1'}]};
 await send(form,input);edit='newer';await send(form,{...input,memo:'newer'});await send(form,{...input,memo:'newer'});
 expect(calls.map(c=>c.method??'GET')).toEqual(['POST','POST','GET','GET']);expect(calls[1]!.body).toBe(calls[0]!.body);expect(new Headers(calls[0]!.headers).get('idempotency-key')).toBe(new Headers(calls[1]!.headers).get('idempotency-key'));expect(edit).toBe('newer');expect(dirty).toBe(true);expect(harness.push).not.toHaveBeenCalled();expect(harness.refresh).not.toHaveBeenCalled();expect(harness.track.mock.calls.at(-1)?.[2]).toBe(false);
});
it('navigates only after the exact new manual draft and context are read back',async()=>{
 const node={isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}};const form={isConnected:true,getAttribute:()=>null} as unknown as HTMLFormElement;const browser=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[form],documentElement:{dataset:{}}});
 let key='',reads=0;vi.stubGlobal('fetch',vi.fn(async (_url:RequestInfo|URL,init:RequestInit={})=>{if(init.method==='POST'){key=new Headers(init.headers).get('idempotency-key')!;return Response.json({data:{id:`journal-${key}`,number:`J-${key}`,status:'draft',version:1,debit:'1',credit:'1',fxRateMicros:1000000,baseCurrency:'THB'}},{status:201});}reads++;return Response.json({context:{...context,userId:reads===1?'wrong-user':'user',expiresAt:9999999999999},data:{journal:{id:`journal-${key}`,source_type:'manual',status:'draft',version:1}}});}));
 let send!:ReturnType<typeof useNativeJournalCreate>['send'];function Observe(){send=useNativeJournalCreate('/holdings/confirmed',context,true).send;return null;}renderToStaticMarkup(createElement(Observe));await send(form,{chartId:'chart',periodId:'period',entryDate:'2026-01-02',currency:'THB',lines:[]});expect(harness.push).not.toHaveBeenCalled();await send(form,{chartId:'other'});expect(harness.push).toHaveBeenCalledOnce();expect(String(harness.push.mock.calls[0]![0])).toContain(`journal-${key}`);expect(reads).toBe(2);
});

it('retains an original creation after a malformed journal number until the exact receipt is returned',async()=>{
 const node={isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}};const form={isConnected:true,getAttribute:()=>null} as unknown as HTMLFormElement;const browser=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[form],documentElement:{dataset:{}}});
 const calls:RequestInit[]=[];let key='';vi.stubGlobal('fetch',vi.fn(async (_url:RequestInfo|URL,init:RequestInit={})=>{calls.push(init);if(init.method==='POST'){key=new Headers(init.headers).get('idempotency-key')!;return Response.json({data:{id:`journal-${key}`,number:calls.length===1?'wrong':`J-${key}`,status:'draft',version:1,debit:'1',credit:'1',fxRateMicros:1000000,baseCurrency:'THB'}},{status:201});}return Response.json({context:{...context,expiresAt:9999999999999},data:{journal:{id:`journal-${key}`,source_type:'manual',status:'draft',version:1}}});}));
 let send!:ReturnType<typeof useNativeJournalCreate>['send'];function Observe(){send=useNativeJournalCreate('/holdings/bad-number',context,true).send;return null;}renderToStaticMarkup(createElement(Observe));const input={chartId:'chart',periodId:'period',entryDate:'2026-01-02',currency:'THB',lines:[]};await send(form,input);expect(calls).toHaveLength(1);expect(harness.push).not.toHaveBeenCalled();expect(harness.track.mock.calls.at(-1)?.[2]).toBe(true);await send(form,input);expect(calls.map(c=>c.method??'GET')).toEqual(['POST','POST','GET']);expect(calls[1]!.body).toBe(calls[0]!.body);expect(new Headers(calls[1]!.headers).get('idempotency-key')).toBe(new Headers(calls[0]!.headers).get('idempotency-key'));expect(harness.push).toHaveBeenCalledOnce();
});
it.each([false,true])('releases only a first definite wire refusal; earlier uncertainty=%s',async uncertain=>{
 const node={isConnected:true,dataset:{workUser:'user',workCompany:'c',workAssignment:'a',workRevision:'7',workAssignmentVersion:'3',workStale:'false'}};const form={isConnected:true,getAttribute:()=>null} as unknown as HTMLFormElement;const browser=Object.assign(new EventTarget(),{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/finance'}});vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:()=>node,querySelectorAll:()=>[form],documentElement:{dataset:{}}});
 const calls:RequestInit[]=[];let key='';vi.stubGlobal('fetch',vi.fn(async (_url:RequestInfo|URL,init:RequestInit={})=>{calls.push(init);if(init.method==='POST'){key=new Headers(init.headers).get('idempotency-key')!;if(uncertain&&calls.length===1)throw Error('lost reply');if(calls.length===(uncertain?2:1))return Response.json({error:{code:'VALIDATION_FAILED',message:'Invalid journal.'}},{status:422});return Response.json({data:{id:`journal-${key}`,number:`J-${key}`,status:'draft',version:1,debit:'1',credit:'1',fxRateMicros:1000000,baseCurrency:'THB'}},{status:201});}return Response.json({context:{...context,expiresAt:9999999999999},data:{journal:{id:`journal-${key}`,source_type:'manual',status:'draft',version:1}}});}));
 let send!:ReturnType<typeof useNativeJournalCreate>['send'];function Observe(){send=useNativeJournalCreate(`/holdings/wire-${uncertain}`,context,true).send;return null;}renderToStaticMarkup(createElement(Observe));const original={chartId:'chart',periodId:'period',entryDate:'2026-01-02',currency:'THB',memo:'original',lines:[]},corrected={...original,memo:'corrected'};
 await send(form,original);if(uncertain)await send(form,corrected);
 expect(harness.track.mock.calls.at(-1)?.[2]).toBe(uncertain);expect(harness.push).not.toHaveBeenCalled();await send(form,corrected);
 const posts=calls.filter(c=>c.method==='POST'),firstKey=new Headers(posts[0]!.headers).get('idempotency-key'),lastKey=new Headers(posts.at(-1)!.headers).get('idempotency-key');
 if(uncertain){expect(lastKey).toBe(firstKey);expect(posts.every(p=>p.body===posts[0]!.body)).toBe(true);}else{expect(lastKey).not.toBe(firstKey);expect(posts.at(-1)!.body).toBe(JSON.stringify(corrected));}expect(harness.push).toHaveBeenCalledOnce();
});
