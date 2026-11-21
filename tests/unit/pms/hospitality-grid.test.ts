import {expect,it} from 'vitest';
import {expandRoomNumbers,roomSaveKey,mergeSetupSave,type SetupSave} from "@erpx/feature-shared/lib/hospitality-grid";
import {assertHospitalityDevelopmentTarget} from "@erpx/feature-shared/lib/hospitality-seed-target";
it('expands padded room ranges and rejects overlapping or unbounded grids',()=>{
 expect(expandRoomNumbers('001-003, 201')).toEqual(['001','002','003','201']);
 expect(()=>expandRoomNumbers('101-103, 102')).toThrow('Duplicate');
 expect(()=>expandRoomNumbers('10-1')).toThrow('ascending');
 expect(()=>expandRoomNumbers('1-1000')).toThrow('200');
});
it('preserves successful and interrupted requests while allowing rejected rows to be corrected',()=>{
 const row=(key:string,name:string):SetupSave=>({key,url:'/table/create',body:{name},label:name,state:'pending'});
 const saved={...row('saved','Original'),state:'saved' as const,resultId:'table-1'};
 const interrupted={...row('interrupted','Original'),state:'failed' as const};
 const rejected:SetupSave={...row('rejected','Invalid'),state:'failed',rejected:true};
 const queue=[saved,interrupted,rejected];
 mergeSetupSave(queue,row('saved','Edited'));
 mergeSetupSave(queue,row('interrupted','Edited'));
 mergeSetupSave(queue,row('rejected','Corrected'));
 expect(saved.body).toEqual({name:'Original'});expect(saved.resultId).toBe('table-1');
 expect(interrupted.body).toEqual({name:'Original'});
 expect(rejected.body).toEqual({name:'Corrected'});expect(rejected.requestKey).toBeTruthy();
 expect(rejected.key).toBe('rejected');expect(rejected.state).toBe('pending');
 const dependent={...row('menu','Original'),dependsOn:'product',state:'failed' as const};
 mergeSetupSave([dependent],{...row('menu','Corrected'),dependsOn:'product'});
 expect(dependent.body).toEqual({name:'Corrected'});
});
it('uses stable command keys for room labels with spaces or non-Latin characters',async()=>{
 const key=await roomSaveKey('room-row','Annex / ห้อง 1');
 expect(key).toMatch(/^[a-zA-Z0-9:_.-]{8,180}$/);
 expect(await roomSaveKey('room-row','Annex / ห้อง 1')).toBe(key);
 expect(await roomSaveKey('room-row','Annex / ห้อง 2')).not.toBe(key);
});
it('rejects every non-development database before connections',()=>{
 const manifest=(url:string)=>({databases:{H001_DB:url}});
 expect(()=>assertHospitalityDevelopmentTarget(manifest('postgresql://test@127.0.0.1:5432/erpx_dev_h001_write'),'local')).not.toThrow();
 for(const url of ['postgresql://test@127.0.0.1:5432/erpx_h001_write','postgresql://test@127.0.0.1:5432/erpx_stage_h001_write','postgresql://test@remote:5432/erpx_dev_h001_write','postgresql://test@127.0.0.1:5433/erpx_dev_h001_write'])expect(()=>assertHospitalityDevelopmentTarget(manifest(url),'local')).toThrow('HOSPITALITY_DEVELOPMENT_ONLY');
 expect(()=>assertHospitalityDevelopmentTarget(manifest('postgresql://test@127.0.0.1:5432/erpx_dev_h001_write'),'staging')).toThrow();
});

import {afterEach,vi} from 'vitest';
import {workFetch} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {createLatestRead} from '@erpx/feature-shared/lib/latest-read';
import {setContextSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {plusDays} from '@erpx/server-pms/public/rules/pms-model';

afterEach(()=>{if(typeof document!=='undefined')setContextSwitching(false);vi.unstubAllGlobals();});
const retainedPmsOwnerSources={
 front:{path:'../../../fixtures/pms/source-baselines/pms-front-workspace-pre-standard.tsx.txt',sha256:'61d78d6df643999f4190f20224ea53261fcbf7d383eb5edc011f0cfc299c752f'},
 backoffice:{path:'../../../fixtures/pms/source-baselines/pms-backoffice-workspace-pre-standard.tsx.txt',sha256:'87f620301e1ec4b955fef5a5c41343e7c938c70d46d58aed1ca548e2542adc58'},
};
const actualPmsFrontSource=process.env.ERPX_PMS_WORKSPACE_BASELINE==='1'?retainedPmsOwnerSources.front.path:"../../../packages/feature-pms/src/components/standard-pms-front-workspace.tsx";
const actualPmsBackofficeSource=process.env.ERPX_PMS_WORKSPACE_BASELINE==='1'?retainedPmsOwnerSources.backoffice.path:"../../../packages/feature-pms/src/components/standard-pms-backoffice-workspace.tsx";
/** Only explicit historical probes read text fixtures. Ordinary tests still read
 * the real current owners. Pins bind both native pre-standard source bytes and
 * their provenance manifest before any of the existing callback extraction. */
async function readPmsOwnerSource(path:string){
 const {readFile}=await import('node:fs/promises'),retained=Object.values(retainedPmsOwnerSources).find(source=>source.path===path);
 if(!retained)return readFile(new URL(path,import.meta.url),'utf8');
 const {createHash}=await import('node:crypto');
 const manifest=await readFile(new URL('../../../fixtures/pms/source-baselines/manifest.json',import.meta.url));
 if(createHash('sha256').update(manifest).digest('hex')!=='d17607a807a6cf319466cfeade81498c7247f4111df14f1fe2b986c1268f799b')throw Error('The retained PMS owner source manifest changed.');
 const source=await readFile(new URL(retained.path,import.meta.url));
 if(createHash('sha256').update(source).digest('hex')!==retained.sha256)throw Error('The retained pre-standard PMS owner source changed.');
 return source.toString('utf8');
}
function pmsReadBrowser(){
 const work={isConnected:true,dataset:{workUser:'operator',workRevision:'7',workCompany:'original-company',workAssignment:'housekeeper',workStale:'false'}},front={isConnected:true,dataset:{frontApp:'pms',frontLocation:'holding'}};
 const browser=Object.assign(new EventTarget(),{location:{origin:'https://pms-dev-erpx.jesadakorn.com'}}),doc=Object.assign(new EventTarget(),{visibilityState:'visible',documentElement:{dataset:{}},activeElement:{closest:()=>null},querySelector:(selector:string)=>selector==='[data-work-revision]'&&work.isConnected?work:selector==='[data-front-app]'&&front.isConnected?front:null});
 vi.stubGlobal('window',browser);vi.stubGlobal('document',doc);vi.stubGlobal('navigator',{onLine:true});
 return {work,front,doc};
}
function delayedPmsRead(){
 let entered!:()=>void,release!:()=>void;
 const started=new Promise<void>(resolve=>{entered=resolve;}),released=new Promise<void>(resolve=>{release=resolve;});
 const snapshot={property:{id:'original-hotel'},records:[]},workspace={snapshot,permissions:['pms.read','pms.housekeeping']};
 class OriginalBody extends Response{override async json(){entered();await released;return super.json();}}
 const fetcher=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>new OriginalBody(JSON.stringify({data:workspace}),{headers:{'content-type':'application/json'}}));
 vi.stubGlobal('fetch',fetcher);
 return {started,release,fetcher,snapshot,workspace};
}
async function pmsReadEffectSource(path:string,kind:'calendar'|'housekeeping'){
 const ts=await import('typescript'),text=await readPmsOwnerSource(path),file=ts.createSourceFile('actual-pms-read.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),effects:import('typescript').ArrowFunction[]=[];
 const visit=(node:import('typescript').Node)=>{if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&node.expression.text==='useEffect'&&node.arguments[0]&&ts.isArrowFunction(node.arguments[0])){const body=node.arguments[0].getText(file);if(kind==='calendar'?body.includes('const abort=new AbortController()'):body.includes('async function refresh()')&&body.includes('visibilitychange'))effects.push(node.arguments[0]);}ts.forEachChild(node,visit);};visit(file);
 if(effects.length!==1)throw Error('Expected the exact original PMS read effect.');
 return {ts,file,effect:effects[0]};
}
async function actualCalendarRead(base='/front/acme/acme-co/pms'){
 const {ts,file,effect}=await pmsReadEffectSource("../../../packages/feature-pms/src/components/operations-calendar.tsx",'calendar');
 if(!ts.isBlock(effect.body))throw Error('Expected original Calendar effect block.');
 const statements=effect.body.statements,chain=statements.find(ts.isExpressionStatement),cleanup=statements.find(ts.isReturnStatement),declarations=statements.filter(ts.isVariableStatement);
 if(!chain||!cleanup?.expression)throw Error('Expected original Calendar read chain and cleanup.');
 // Execute the original request/JSON handler and cleanup bodies. Only expose
 // their promises to the test; React hooks are neither called nor mocked.
 const body=`${declarations.map(node=>node.getText(file)).join('\n')}\nconst complete=${chain.expression.getText(file)};return {complete,cleanup:${cleanup.expression.getText(file)}};`;
 const emitted=ts.transpileModule(body,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const setLoaded=vi.fn(),setError=vi.fn(),build=new Function('workFetch','captureWorkCommandContext','base','propertyId','from','plusDays','setLoaded','setError',emitted) as (...ports:unknown[])=>{complete:Promise<void>;cleanup:()=>void};
 return {...build(workFetch,captureWorkCommandContext,base,'original-hotel','2026-10-16',plusDays,setLoaded,setError),setLoaded,setError};
}
async function actualHousekeepingRead(owner:'front'|'backoffice',timeout=20_000){
 const path=owner==='front'?actualPmsFrontSource:actualPmsBackofficeSource, {ts,file,effect}=await pmsReadEffectSource(path,'housekeeping');
 const emitted=ts.transpileModule(`return (${effect.getText(file)})();`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const reads=createLatestRead<unknown>(timeout),readEpoch={current:0},setData=vi.fn(),setError=vi.fn(),setNotice=vi.fn(),clear=vi.fn(),invalidateReads=vi.fn(()=>{readEpoch.current++;reads.cancel();});
 let refresh:(()=>Promise<void>)|undefined;
 const schedule=vi.fn((callback:()=>Promise<void>,delay:number)=>{refresh=callback;expect(delay).toBe(8000);return 1;});
 const names=['workFetch','captureWorkCommandContext','base','propertyId','section','pendingWrites','reads','tr','listReadKey','setError','setNotice','setData','setInterval','clearInterval','invalidateReads'];
 const build=new Function(...names,emitted) as (...ports:unknown[])=>()=>void;
 const cleanup=build(workFetch,captureWorkCommandContext,owner==='front'?'/front/acme/acme-co/pms':'/holdings/acme','original-hotel','housekeeping',{current:0},reads,(value:string)=>value,JSON.stringify({view:'list',section:'housekeeping',from:'2026-10-16',to:'2026-10-30',page:'1',q:'',status:'',building:'',floor:'',typeId:''}),setError,setNotice,setData,schedule,clear,invalidateReads);
 if(!refresh)throw Error('Expected the original Housekeeping visibility/poll callback.');
 return {refresh,cleanup,setData,setError,setNotice,clear,invalidateReads};
}
type LatePmsContext='role'|'revision'|'stale'|'Front location';
function changePmsContext(browser:ReturnType<typeof pmsReadBrowser>,kind:LatePmsContext){
 if(kind==='role')browser.work.dataset.workAssignment='another-assignment';
 else if(kind==='revision')browser.work.dataset.workRevision='8';
 else if(kind==='stale')browser.work.dataset.workStale='true';
 else browser.front.dataset.frontLocation='branches/another-branch';
}
it.each(['role','revision','stale','Front location'] as const)('actual PMS Calendar refuses a late %s after original JSON decoding before publishing the room grid',async kind=>{
 const browser=pmsReadBrowser(),body=delayedPmsRead(),read=await actualCalendarRead();
  try{await body.started;changePmsContext(browser,kind);body.release();await read.complete;expect(read.setLoaded).not.toHaveBeenCalled();expect(read.setError).not.toHaveBeenCalled();}finally{body.release();read.cleanup();}
});
it('actual PMS Calendar cleanup aborts a lazy original body before publishing its old property window',async()=>{
 pmsReadBrowser();const body=delayedPmsRead(),read=await actualCalendarRead();
 await body.started;read.cleanup();body.release();await read.complete;
 expect(body.fetcher.mock.calls[0][1]?.signal?.aborted).toBe(true);expect(read.setLoaded).not.toHaveBeenCalled();expect(read.setError).not.toHaveBeenCalled();
});
it('actual PMS Calendar publishes its requested original property and dates in the unchanged mounted context',async()=>{
 pmsReadBrowser();const body=delayedPmsRead(),read=await actualCalendarRead();
 try{await body.started;body.release();await read.complete;expect(read.setLoaded).toHaveBeenCalledExactlyOnceWith({base:'/front/acme/acme-co/pms',propertyId:'original-hotel',from:'2026-10-16',snapshot:body.snapshot});expect(String(body.fetcher.mock.calls[0][0])).toContain('property=original-hotel&view=calendar&from=2026-10-16&to=2026-10-30');}finally{read.cleanup();}
});
it('actual PMS Calendar keeps its normal Backoffice read valid without a Front node',async()=>{
 const browser=pmsReadBrowser();browser.front.isConnected=false;
 const body=delayedPmsRead(),read=await actualCalendarRead('/holdings/acme');
 try{await body.started;body.release();await read.complete;expect(read.setLoaded).toHaveBeenCalledExactlyOnceWith({base:'/holdings/acme',propertyId:'original-hotel',from:'2026-10-16',snapshot:body.snapshot});expect(read.setError).toHaveBeenCalledExactlyOnceWith('');expect(String(body.fetcher.mock.calls[0][0])).toContain('/api/v1/holdings/acme/pms?');}finally{read.cleanup();}
});
it.each((['front','backoffice'] as const).flatMap(owner=>(['role','revision','stale','Front location'] as const).map(kind=>[owner,kind] as const)))('actual %s Housekeeping polling refuses late %s after original JSON before replacing loaded rooms/tasks',async(owner,kind)=>{
 const browser=pmsReadBrowser(),body=delayedPmsRead(),read=await actualHousekeepingRead(owner),pending=read.refresh();
 try{await body.started;changePmsContext(browser,kind);body.release();await pending;expect(read.setData).not.toHaveBeenCalled();expect(read.setNotice).not.toHaveBeenCalled();}finally{body.release();read.cleanup();}
});
it.each(['front','backoffice'] as const)('actual %s Housekeeping cleanup cancels the in-flight original read and removes the visibility listener',async owner=>{
 const browser=pmsReadBrowser(),remove=vi.spyOn(browser.doc,'removeEventListener'),body=delayedPmsRead(),read=await actualHousekeepingRead(owner),pending=read.refresh();
 await body.started;read.cleanup();body.release();await pending;
 expect(read.invalidateReads).toHaveBeenCalledOnce();expect(read.clear).toHaveBeenCalledExactlyOnceWith(1);expect(remove).toHaveBeenCalledWith('visibilitychange',read.refresh);expect(read.setData).not.toHaveBeenCalled();expect(read.setNotice).not.toHaveBeenCalled();
});
it.each(['front','backoffice'] as const)('actual %s Housekeeping preserves the latest-read deadline even when the original body ignores abort',async owner=>{
 pmsReadBrowser();const body=delayedPmsRead(),read=await actualHousekeepingRead(owner,10),pending=read.refresh();
 try{await body.started;const signal=body.fetcher.mock.calls[0][1]?.signal;if(!signal)throw Error('Expected original latest-read signal.');await new Promise<void>(resolve=>signal.aborted?resolve():signal.addEventListener('abort',()=>resolve(),{once:true}));body.release();await pending;expect(read.setData).not.toHaveBeenCalled();expect(read.setNotice).toHaveBeenCalledWith('Connection lost · Showing the latest loaded data');}finally{body.release();read.cleanup();}
});
it.each(['front','backoffice'] as const)('actual %s Housekeeping retains its initial owner seed until a visibility/poll refresh and then publishes current data',async owner=>{
 const browser=pmsReadBrowser();if(owner==='backoffice')browser.front.isConnected=false;
 const body=delayedPmsRead(),read=await actualHousekeepingRead(owner);
 try{expect(body.fetcher).not.toHaveBeenCalled();const pending=read.refresh();await body.started;body.release();await pending;expect(read.setData).toHaveBeenCalledExactlyOnceWith(body.workspace);expect(read.setNotice).not.toHaveBeenCalled();const url=new URL(String(body.fetcher.mock.calls[0][0]),'https://pms.invalid');expect(url.searchParams.get('property')).toBe('original-hotel');expect(url.searchParams.get('section')).toBe('housekeeping');expect(url.searchParams.get('view')).toBe('list');}finally{body.release();read.cleanup();}
});

import * as React from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {FrontLocalizedTree} from '@erpx/front/components/front-language';
import {Button,Field,Table,TableFrame,TextInput} from '@erpx/ui/components/foundation';
import {CollectionRecordLink} from '@erpx/feature-records/components/record-collection';
import {dates,records,defaultConfig,type Snapshot} from '@erpx/server-pms/public/rules/pms-model';
const calendarRoute=vi.hoisted(()=>({pathname:'/holdings/acme/pms/front-desk',query:'property=original-hotel'}));
vi.mock('next/navigation',()=>({usePathname:()=>calendarRoute.pathname,useSearchParams:()=>new URLSearchParams(calendarRoute.query)}));
const retainedCalendar:Snapshot={
 property:{id:'original-hotel',companyId:'original-company',branchId:'original-branch',name:'Originally readable hotel',businessDate:'2026-10-16',state:'open',version:3,config:defaultConfig},
 records:[
  {id:'retained-room',kind:'room',version:1,updatedAt:1,data:{number:'Retained private room 701',typeId:'original-type',building:'Original building',floor:'7',active:true,cleanliness:'inspected',dnd:false,observedOccupied:true}},
  {id:'retained-private-stay',kind:'stay',version:1,updatedAt:1,data:{reservationId:'original-reservation',guestId:'original-guest',typeId:'original-type',rateId:'original-rate',roomId:'retained-room',arrival:'2026-10-16',departure:'2026-10-18',adults:1,state:'in_house',guestAccountId:'original-guest-account',prices:[],segments:[]}},
 ],
};
async function actualCalendarRender({base='/holdings/acme',propertyId='original-hotel',from='2026-10-16'}:{base?:string;propertyId?:string;from?:string}={}){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),text=await readFile(new URL("../../../packages/feature-pms/src/components/operations-calendar.tsx",import.meta.url),'utf8'),file=ts.createSourceFile('actual-pms-calendar-render.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const owner=file.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='PmsOperationsCalendar');
 if(!owner||!ts.isFunctionDeclaration(owner)||!owner.body)throw Error('Expected original PMS Calendar owner.');
 const statements=owner.body.statements,effect=statements.findIndex(node=>ts.isExpressionStatement(node)&&ts.isCallExpression(node.expression)&&ts.isIdentifier(node.expression.expression)&&node.expression.expression.text==='useEffect');
 if(effect<0)throw Error('Expected original Calendar read/render boundary.');
 // Render the exact original projection after its effect declaration, with a
 // retained owner state. Real React and child widgets render; no hook is mocked.
 const emitted=ts.transpileModule(statements.slice(effect+1).map(node=>node.getText(file)).join('\n'),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React}}).outputText;
 const loaded={base:'/holdings/acme',propertyId:'original-hotel',from:'2026-10-16',snapshot:retainedCalendar};
 const names=['React','FrontLocalizedTree','Button','Field','Table','TableFrame','TextInput','CollectionRecordLink','dates','records','plusDays','loaded','from','error','retry','setFrom','setRetry','base','propertyId'];
 const render=new Function(...names,emitted) as (...ports:unknown[])=>React.ReactNode;
 calendarRoute.pathname=base+'/pms/front-desk';calendarRoute.query=new URLSearchParams({property:propertyId}).toString();
 return renderToStaticMarkup(render(React,FrontLocalizedTree,Button,Field,Table,TableFrame,TextInput,CollectionRecordLink,dates,records,plusDays,loaded,from,'',0,vi.fn(),vi.fn(),base,propertyId));
}
it.each(['property','base'] as const)('actual PMS Calendar render hides a retained old %s owner before the new request completes',async changed=>{
 const fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);
 const html=await actualCalendarRender(changed==='property'?{propertyId:'new-hotel'}:{base:'/holdings/other'});
 expect(html).not.toContain('Retained private room 701');expect(html).not.toContain('/records/pms_record/retained-private-stay/overview');expect(html).toContain('Loading calendar…');expect(fetcher).not.toHaveBeenCalled();
});
it('actual PMS Calendar render retains a clearly labelled earlier date window for the same base and hotel while loading new dates',async()=>{
 const html=await actualCalendarRender({from:'2026-11-01'});
 expect(html).toContain('Retained private room 701');expect(html).toContain('/records/pms_record/retained-private-stay/overview');expect(html).toContain('>10-16<');expect(html).not.toContain('>11-01<');expect(html).toContain('value="2026-11-01"');expect(html).toContain('Loading calendar…');
});
it('actual PMS Calendar render preserves current loaded room widgets without a redundant loading state',async()=>{
 const html=await actualCalendarRender();expect(html).toContain('Retained private room 701');expect(html).toContain('/records/pms_record/retained-private-stay/overview');expect(html).not.toContain('Loading calendar…');
});

import {BookingPaymentReturn,type BookingPaymentReturnStatus} from '@erpx/feature-pms/components/booking-payment-return';
const bookingReturnObserver=vi.hoisted(()=>({active:false,stateIndex:0,effects:[] as Array<()=>void|(()=>void)>,dependencies:[] as Array<React.DependencyList|undefined>,updates:[] as unknown[],errors:[] as unknown[]}));
vi.mock('react',async importOriginal=>{
 const actual=await importOriginal<typeof import('react')>();
 return {...actual,
  useState<T>(initial:T|(()=>T)){
   const state=actual.useState(initial),index=bookingReturnObserver.active?bookingReturnObserver.stateIndex++:-1,updates=bookingReturnObserver.updates,errors=bookingReturnObserver.errors;
   return [state[0],(next:React.SetStateAction<T>)=>{if(index===0)updates.push(next);if(index===1)errors.push(next);state[1](next);}] as const;
  },
  useEffect(effect:React.EffectCallback,dependencies?:React.DependencyList){actual.useEffect(effect,dependencies);if(bookingReturnObserver.active){bookingReturnObserver.effects.push(effect);bookingReturnObserver.dependencies.push(dependencies);}},
 };
});
afterEach(()=>{bookingReturnObserver.active=false;if(vi.isFakeTimers()){vi.clearAllTimers();vi.useRealTimers();}});
const bookingReturnIdentity={slug:'acme',propertyId:'original-property',attempt:'original-payment.original-signature'};
const bookingReturnStatus=(state:BookingPaymentReturnStatus['state'],refundState:string|null=null):BookingPaymentReturnStatus=>({number:'ORIGINAL-BOOKING-1',state,paymentState:state==='confirmed'?'received':'pay_at_hotel',checkoutUrl:state==='payment_pending'?'https://checkout.stripe.com/c/pay/original':null,managementToken:undefined,refundState});
function bookingReturnResponse(status:BookingPaymentReturnStatus,identity:Partial<typeof bookingReturnIdentity>={}){return Response.json({data:{...status,attemptToken:identity.attempt??bookingReturnIdentity.attempt,hotel:{id:identity.propertyId??bookingReturnIdentity.propertyId}}});}
function bookingReturnTimers(){
 vi.useFakeTimers();
 // The actual Holding transport reads these browser ports. Only native fetch
 // IO is substituted; namespace selection and receipt validation stay real.
 vi.stubGlobal('window',{setInterval,clearInterval,location:new URL('https://acme.jesadakorn.com/book/original-property')});
 vi.stubGlobal('document',{documentElement:{dataset:{erpxApp:'booking'}}});
}
/** Actual React supplies both state/effect hooks and renders the original
 * component/widgets. Transparent observers record its real closure and state
 * publications; tests explicitly drive effect setup/cleanup, timers and native
 * Response decoding. This is not a browser/hydration or provider acceptance. */
function observeBookingReturn(initial:BookingPaymentReturnStatus){
 bookingReturnObserver.stateIndex=0;bookingReturnObserver.effects=[];bookingReturnObserver.dependencies=[];bookingReturnObserver.updates=[];bookingReturnObserver.errors=[];
 function OriginalReturn(){bookingReturnObserver.active=true;try{return BookingPaymentReturn({...bookingReturnIdentity,initial});}finally{bookingReturnObserver.active=false;}}
 const html=renderToStaticMarkup(React.createElement(OriginalReturn)),effects=bookingReturnObserver.effects,dependencies=bookingReturnObserver.dependencies[0],updates=bookingReturnObserver.updates,errors=bookingReturnObserver.errors;
 expect(effects).toHaveLength(1);const cleanup=effects[0]();return {html,dependencies,updates,errors,cleanup:()=>{if(typeof cleanup==='function')cleanup();}};
}
it('actual Booking return keeps expired receipts observable through late refund review and stops only after owner-confirmed refund settlement',async()=>{
 bookingReturnTimers();const fetcher=vi.fn<typeof fetch>().mockResolvedValueOnce(bookingReturnResponse(bookingReturnStatus('refund_review','requested'))).mockResolvedValueOnce(bookingReturnResponse(bookingReturnStatus('refund_review','succeeded')));vi.stubGlobal('fetch',fetcher);
 let mounted=observeBookingReturn(bookingReturnStatus('expired'));
 try{
  expect(mounted.html).toContain('Payment period expired');expect(fetcher).not.toHaveBeenCalled();await vi.advanceTimersByTimeAsync(4999);expect(fetcher).not.toHaveBeenCalled();await vi.advanceTimersByTimeAsync(1);expect(fetcher).toHaveBeenCalledOnce();expect(mounted.updates).toEqual([bookingReturnStatus('refund_review','requested')]);
  mounted.cleanup();mounted=observeBookingReturn(bookingReturnStatus('refund_review','requested'));expect(mounted.html).toContain('Payment needs review');expect(fetcher).toHaveBeenCalledTimes(1);await vi.advanceTimersByTimeAsync(5000);expect(fetcher).toHaveBeenCalledTimes(2);expect(mounted.updates).toEqual([bookingReturnStatus('refund_review','succeeded')]);
  const pendingDependencies=mounted.dependencies;mounted.cleanup();mounted=observeBookingReturn(bookingReturnStatus('refund_review','succeeded'));expect(mounted.dependencies).not.toEqual(pendingDependencies);expect(mounted.html).toContain('Refund: succeeded');await vi.advanceTimersByTimeAsync(20000);expect(fetcher).toHaveBeenCalledTimes(2);expect(mounted.updates).toEqual([]);
  for(const [url,init] of fetcher.mock.calls){expect(String(url)).toBe('https://acme.jesadakorn.com/__erpx/booking/api/public/booking/acme/reservations?attempt=original-payment.original-signature');expect(init?.cache).toBe('no-store');expect(init?.method).toBeUndefined();expect(init?.signal).toBeInstanceOf(AbortSignal);}
 }finally{mounted.cleanup();}
});
it('actual Booking return follows pending then expired to a late confirmed receipt without duplicate mount GETs',async()=>{
 bookingReturnTimers();const fetcher=vi.fn<typeof fetch>().mockResolvedValueOnce(bookingReturnResponse(bookingReturnStatus('expired'))).mockResolvedValueOnce(bookingReturnResponse(bookingReturnStatus('confirmed')));vi.stubGlobal('fetch',fetcher);
 let mounted=observeBookingReturn(bookingReturnStatus('payment_pending'));
 try{
  expect(fetcher).not.toHaveBeenCalled();await vi.advanceTimersByTimeAsync(5000);expect(fetcher).toHaveBeenCalledOnce();expect(mounted.updates).toEqual([bookingReturnStatus('expired')]);mounted.cleanup();mounted=observeBookingReturn(bookingReturnStatus('expired'));expect(fetcher).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(5000);expect(fetcher).toHaveBeenCalledTimes(2);expect(mounted.updates).toEqual([bookingReturnStatus('confirmed')]);mounted.cleanup();mounted=observeBookingReturn(bookingReturnStatus('confirmed'));expect(mounted.html).toContain('Booking confirmed');await vi.advanceTimersByTimeAsync(20000);expect(fetcher).toHaveBeenCalledTimes(2);
 }finally{mounted.cleanup();}
});
it('actual Booking return cleanup rejects a late native body and never overlaps its pending receipt reads',async()=>{
 bookingReturnTimers();let entered!:()=>void,release!:()=>void;const started=new Promise<void>(resolve=>{entered=resolve;}),resume=new Promise<void>(resolve=>{release=resolve;});
 class LazyBookingResponse extends Response{override async json(){entered();await resume;return super.json();}}
 const fetcher=vi.fn<typeof fetch>().mockResolvedValue(new LazyBookingResponse(JSON.stringify({data:{...bookingReturnStatus('confirmed'),attemptToken:bookingReturnIdentity.attempt,hotel:{id:bookingReturnIdentity.propertyId}}}),{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetcher);const mounted=observeBookingReturn(bookingReturnStatus('payment_pending'));
 try{await vi.advanceTimersByTimeAsync(5000);await started;await vi.advanceTimersByTimeAsync(10000);expect(fetcher).toHaveBeenCalledOnce();mounted.cleanup();expect(fetcher.mock.calls[0][1]?.signal?.aborted).toBe(true);release();await vi.advanceTimersByTimeAsync(0);expect(mounted.updates).toEqual([]);expect(mounted.errors).toEqual([]);await vi.advanceTimersByTimeAsync(10000);expect(fetcher).toHaveBeenCalledOnce();}finally{release();mounted.cleanup();}
});
it.each(['attempt','propertyId'] as const)('actual Booking return rejects another %s response without replacing the retained original receipt',async changed=>{
 bookingReturnTimers();const fetcher=vi.fn<typeof fetch>().mockResolvedValue(bookingReturnResponse(bookingReturnStatus('confirmed'),{[changed]:'another-original-source'}));vi.stubGlobal('fetch',fetcher);const mounted=observeBookingReturn(bookingReturnStatus('payment_pending'));
 try{await vi.advanceTimersByTimeAsync(5000);expect(mounted.updates).toEqual([]);expect(mounted.errors).toEqual(['Payment status unavailable']);expect(fetcher).toHaveBeenCalledOnce();}finally{mounted.cleanup();}
});

import {pmsPackageCommandAllowed} from '@erpx/feature-pms/lib/package-access';
import {capturePmsWorkspaceCommandContext,sendPmsWorkspaceCommand,pendingPmsWorkspaceCommand,type PmsWorkspaceCommandAttempt,type PmsWorkspaceCommandSource} from '@erpx/feature-pms/lib/workspace-command';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {TaskCompletionDrafts} from '@erpx/feature-pms/components/task-completion-drafts';
import {changedPmsTaskCompletionAttachments} from '@erpx/feature-pms/components/workspace-types';
import {attachmentContentUrl} from '@erpx/feature-files/lib/attachment-client';

type PmsSendOwner='front'|'backoffice';
/** Execute the real send and its Form onSubmit closure with explicit state,
 * form and request ports. No React hook is called or replaced by this seam. */
async function actualPmsWorkspaceSend(owner:PmsSendOwner,options:{holdRefresh?:boolean;failRefresh?:boolean;operation?:string;failFirstBody?:boolean;reflectCreatedSource?:boolean;completionChecklist?:string[];taskCompletesOnRefresh?:boolean}={}){
 const ts=await import('typescript');
 const path=owner==='front'?actualPmsFrontSource:actualPmsBackofficeSource;
 const text=await readPmsOwnerSource(path),file=ts.createSourceFile('actual-pms-send.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 let runCommand:import('typescript').ArrowFunction|undefined,retryOriginal:import('typescript').ArrowFunction|undefined,send:import('typescript').ArrowFunction|undefined,submit:import('typescript').ArrowFunction|undefined,change:import('typescript').ArrowFunction|undefined,activityChange:import('typescript').Expression|undefined,ownerAttachmentActivity:import('typescript').ArrowFunction|undefined;
 function visit(node:import('typescript').Node){
  if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text==='send'&&node.initializer&&ts.isArrowFunction(node.initializer))send=node.initializer;
  if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text==='runCommand'&&node.initializer&&ts.isArrowFunction(node.initializer))runCommand=node.initializer;
  if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text==='retryOriginal'&&node.initializer&&ts.isArrowFunction(node.initializer))retryOriginal=node.initializer;
  if(ts.isJsxAttribute(node)&&node.name.getText(file)==='onCompletionAttachmentActivity'&&node.initializer&&ts.isJsxExpression(node.initializer)&&node.initializer.expression){const find=(child:import('typescript').Node)=>{if(ts.isArrowFunction(child)&&child.parameters.length===2&&child.parameters[0]?.name.getText(file)==='purposeKey'&&child.parameters[1]?.name.getText(file)==='active')ownerAttachmentActivity=child;ts.forEachChild(child,find);};find(node.initializer.expression);}
  if(ts.isFunctionDeclaration(node)&&node.name?.text==='Form'){
   const form=(child:import('typescript').Node)=>{if(ts.isJsxAttribute(child)&&child.name.getText(file)==='onSubmit'&&child.initializer&&ts.isJsxExpression(child.initializer)&&child.initializer.expression&&ts.isArrowFunction(child.initializer.expression))submit=child.initializer.expression;if(ts.isJsxAttribute(child)&&child.name.getText(file)==='onChange'&&child.initializer&&ts.isJsxExpression(child.initializer)&&child.initializer.expression&&ts.isArrowFunction(child.initializer.expression))change=child.initializer.expression;if(ts.isJsxAttribute(child)&&child.name.getText(file)==='onActivityChange'&&child.initializer&&ts.isJsxExpression(child.initializer)&&child.initializer.expression)activityChange=child.initializer.expression;ts.forEachChild(child,form);};form(node);
  }
  ts.forEachChild(node,visit);
 }
 visit(file);if(!send||!runCommand||!retryOriginal||!submit||!change||!activityChange||!ownerAttachmentActivity)throw Error('Expected actual PMS send/retry/Form/owner attachment callbacks.');
 const browser=pmsReadBrowser(),base=owner==='front'?'/front/acme/acme-co':'/holdings/acme';
 const attempts={current:new Map<string,PmsWorkspaceCommandAttempt>()},readEpoch={current:0},pendingWrites={current:0};
 const commandSource:{current:PmsWorkspaceCommandSource|null}={current:null},formSource:{current:PmsWorkspaceCommandSource|null}={current:null},formEdits={current:0};
 const layouts:import('typescript').ArrowFunction[]=[];
 const collectLayouts=(node:import('typescript').Node)=>{if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&node.expression.text==='useLayoutEffect'&&node.arguments[0]&&ts.isArrowFunction(node.arguments[0]))layouts.push(node.arguments[0]);ts.forEachChild(node,collectLayouts);};collectLayouts(file);
 const setup=(name:'commandSource'|'formSource')=>{const effect=layouts.find(node=>node.getText(file).includes(name+'.current'));if(!effect)throw Error('Expected the actual PMS mounted source effect.');const ports={commandSource,formSource,base,propertyId:'original-hotel',section:'housekeeping',frontMode:owner==='front',choiceApi:`/api/v1${base}/pms`};const body=ts.transpileModule(`return (${effect.getText(file)})();`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;return new Function(...Object.keys(ports),body)(...Object.values(ports)) as ()=>void;};
 const disposeCommand=setup('commandSource'),disposeForm=setup('formSource');
 const taskForm={isConnected:true};
 const setData=vi.fn((updated:{snapshot?:{property:{id:string};records?:unknown[]}})=>{if(options.reflectCreatedSource&&updated.snapshot&&commandSource.current){commandSource.current={...commandSource.current,propertyId:updated.snapshot.property.id};formSource.current={...formSource.current!,propertyId:updated.snapshot.property.id};}if(options.taskCompletesOnRefresh&&taskForm)taskForm.isConnected=false;}),setCommandState=vi.fn(),setError=vi.fn(),setNotice=vi.fn(),router={push:vi.fn(),refresh:vi.fn()},events:string[]=[],completions:unknown[]=[];
 for(const name of ['erpx:record-command-saved','erpx:work-changed'])window.addEventListener(name,event=>{events.push(name);if(event instanceof CustomEvent)completions.push(event.detail);});
 const data={snapshot:{property:{id:'original-hotel',version:3},records:[]},permissions:['pms.housekeeping']};
 let entered!:()=>void,release!:()=>void;
 const started=new Promise<void>(resolve=>{entered=resolve;}),resumed=new Promise<void>(resolve=>{release=resolve;});
 class OriginalPmsCommandBody extends Response{override async json(){entered();await resumed;if(options.failFirstBody)throw Error("Original committed acknowledgement was lost.");return super.json();}}
 let nativeCreateId:string|undefined,useNativeCreateReceipt=true;
 let nextBody:unknown=options.operation==='property.create'?{data:{id:'new-hotel',propertyId:'new-hotel',version:1}}:{data:{id:'original-task',propertyId:'original-hotel',version:4}},posts=0,gets=0,replyStatus=200;
 let readEntered!:()=>void,readRelease!:()=>void;const readStarted=new Promise<void>(resolve=>{readEntered=resolve;}),readResumed=new Promise<void>(resolve=>{readRelease=resolve;});
 class OriginalPmsRefreshBody extends Response{override async json(){readEntered();await readResumed;return super.json();}}
 const fetcher=vi.fn<(input:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_input,init)=>{
  if(init?.method==='POST'){
   let responseBody=nextBody;
   if(options.operation==='property.create'){
    const key=new Headers(init.headers).get('Idempotency-Key');if(!key)throw Error('The actual original create key is required.');
    const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(key));
    nativeCreateId='property-'+Array.from(new Uint8Array(digest),byte=>byte.toString(16).padStart(2,'0')).join('').slice(0,32);
    if(useNativeCreateReceipt)responseBody={data:{id:nativeCreateId,propertyId:nativeCreateId,version:1}};
   }
   return posts++===0?new OriginalPmsCommandBody(JSON.stringify(responseBody),{status:replyStatus,headers:{'content-type':'application/json'}}):Response.json(responseBody,{status:replyStatus});
  }
  if(options.failRefresh)throw Error('Connection lost during optional PMS refresh.');
  const body={data:options.operation==='property.create'?{...data,snapshot:{property:{id:nativeCreateId,version:1}}}:options.taskCompletesOnRefresh?{...data,snapshot:{...data.snapshot,records:[{kind:'task',id:'original-task',version:5,data:{state:'clean'}}]}}:data};
  return options.holdRefresh&&gets++===0?new OriginalPmsRefreshBody(JSON.stringify(body),{headers:{'content-type':'application/json'}}):Response.json(body);
 });vi.stubGlobal('fetch',fetcher);
 const reads=createLatestRead<unknown>();
 const bindings={workFetch,captureWorkCommandContext,capturePmsWorkspaceCommandContext,sendPmsWorkspaceCommand,pendingPmsWorkspaceCommand,setCommandState,commandSource,pmsPackageCommandAllowed,canonicalUiHref,displayCurrency:'THB',frontMode:owner==='front',base,erpBase:'/holdings/acme',propertyId:'original-hotel',data,attempts,readEpoch,pendingWrites,reads,language:'en',tr:(value:string)=>value,setError,setNotice,setData,router,nativeRouter:router,section:'housekeeping',listReadKey:JSON.stringify({view:'list',section:'housekeeping',page:'1'})};
 const emitted=ts.transpileModule(`const runCommand=(${runCommand.getText(file)});const send=(${send.getText(file)});const retryOriginal=(${retryOriginal.getText(file)});return {send,retryOriginal};`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const callbacks=new Function(...Object.keys(bindings),emitted)(...Object.values(bindings)) as {send:(op:string,input:unknown,version?:number,attachmentSetIds?:string[],onAcknowledged?:(refreshed:boolean)=>void,onEditableRejected?:()=>void)=>Promise<boolean>;retryOriginal:()=>Promise<boolean>},execute=callbacks.send;
 let draft='Original housekeeping note',editVersion='original-edit',checkboxValues=(options.completionChecklist??[]).map(()=>false);
 const reset=vi.fn(()=>{draft='';checkboxValues=checkboxValues.map(()=>false);}),form=Object.assign(taskForm,{reset,getAttribute:(name:string)=>name==='data-erpx-edit-version'?editVersion:null}),formElement=form,editingVersion={current:3 as number|undefined},completionAttachmentActivities={current:new Set<string>()},completionAttachments={current:new Map()},setCompletionAttachmentPending=vi.fn(),setSuccess=vi.fn(),setBusy=vi.fn(),setRoomSets=vi.fn(),formError=vi.fn(),captureCompletionDraft=vi.fn(),completionAcknowledged=vi.fn();
 class OwnerFormData extends FormData{constructor(){super();this.set('notes',draft);checkboxValues.forEach((checked,index)=>{if(checked)this.set(`check_${index}`,'on');});}}
 Object.defineProperty(browser.doc,'activeElement',{value:{closest:()=>form}});
 const taskCompletionAttachmentActivities={current:new Set<string>()},ownerActivityJs=ts.transpileModule(`return (${ownerAttachmentActivity.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,updateOwnerAttachmentActivity=new Function('base','propertyId','hidden','taskCompletionAttachmentActivities',ownerActivityJs)(base,'original-hotel',{id:'original-task'},taskCompletionAttachmentActivities) as (purposeKey:string,active:boolean)=>void;
 const formBindings={capturePmsWorkspaceCommandContext,changedPmsTaskCompletionAttachments,formSource,formEdits,FormData:OwnerFormData,setError:formError,setBusy,setSuccess,setRoomSets,completionAttachmentActivities,completionAttachments,setCompletionAttachmentPending,attachment:{purposeKey:'room_photo'},onCompletionAttachmentActivity:updateOwnerAttachmentActivity,pending:false,hidden:{id:'original-task'},displayFields:[...(options.completionChecklist??[]).map((label,index)=>({name:`check_${index}`,label,type:'checkbox'})),{name:'notes',type:'textarea'}],transform:undefined,op:options.operation??'task.complete',send:execute,editingVersion,version:3,roomSets:1,roomKeys:[],decimalToMinor:(value:string)=>Number(value),onCompletionDraft:captureCompletionDraft,onCompletionAcknowledged:completionAcknowledged};
 const formJs=ts.transpileModule(`return (${submit.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const changeJs=ts.transpileModule(`return (${change.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const activityChangeJs=ts.transpileModule(`return (${activityChange.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 let onChange=new Function(...Object.keys(formBindings),changeJs)(...Object.values(formBindings)) as ()=>void;
 let onSubmit=new Function(...Object.keys(formBindings),formJs)(...Object.values(formBindings)) as (event:{preventDefault:()=>void;currentTarget:typeof form})=>Promise<void>;
 return {browser,attempts,readEpoch,pendingWrites,setCommandState,retry:callbacks.retryOriginal,execute,dispose:()=>{disposeCommand();disposeForm();readEpoch.current++;form.isConnected=false;},reads,setData,setError,setNotice,router,events,completions,fetcher,started,release,readStarted,readRelease,reset,form,editingVersion,setSuccess,formError,
  ownerAttachmentActivities:taskCompletionAttachmentActivities,ownerAttachmentActivity:updateOwnerAttachmentActivity,
  ownerVersion:(version:number)=>{data.snapshot.property.version=version;formBindings.version=version;onChange=new Function(...Object.keys(formBindings),changeJs)(...Object.values(formBindings));onSubmit=new Function(...Object.keys(formBindings),formJs)(...Object.values(formBindings));},submit:()=>onSubmit({preventDefault:vi.fn(),currentTarget:formElement}),setAttachmentActivity:(active:boolean,purposeKey='room_photo')=>{const bindings={...formBindings,attachment:{purposeKey}},callback=new Function(...Object.keys(bindings),activityChangeJs)(...Object.values(bindings)) as ((active:boolean)=>void)|undefined;callback?.(active);},attachmentPending:()=>completionAttachmentActivities.current.size>0,setPending:(value:boolean)=>{formBindings.pending=value;onSubmit=new Function(...Object.keys(formBindings),formJs)(...Object.values(formBindings));},draft:()=>draft,edit:(value:string)=>{if(value===draft)return;draft=value;editVersion='later-edit';onChange();},editCompletion:(values:{checklist:boolean[];notes:string})=>{draft=values.notes;checkboxValues=values.checklist;editVersion='later-edit';onChange();},nativeCreateId:()=>nativeCreateId,nativeCreateReceipt:()=>{useNativeCreateReceipt=true;},body:(value:unknown)=>{useNativeCreateReceipt=false;nextBody=value;},response:(status:number,value:unknown)=>{replyStatus=status;nextBody=value;},captureCompletionDraft,completionAcknowledged,
 };
}
it.each((['front','backoffice'] as const).flatMap(owner=>(['revision','stale','Front location','disposed'] as const).map(kind=>[owner,kind] as const)))('actual %s PMS send preserves its form and pending key after a late %s during the original command body',async(owner,kind)=>{
 const f=await actualPmsWorkspaceSend(owner),pending=f.submit();
 try{
  await f.started;
  if(kind==='disposed')f.dispose();else changePmsContext(f.browser,kind);
  f.release();await pending;
  expect(f.reset).not.toHaveBeenCalled();expect(f.draft()).toBe('Original housekeeping note');expect(f.attempts.current.size).toBe(1);expect(f.events).toEqual([]);expect(f.setData).not.toHaveBeenCalled();expect(f.fetcher).toHaveBeenCalledTimes(1);
 }finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS exact retry reuses the captured key and body after returning to the original role',async owner=>{
 const f=await actualPmsWorkspaceSend(owner),pending=f.submit();
 try{
  await f.started;const original=f.fetcher.mock.calls[0][1];f.browser.work.dataset.workRevision='8';f.release();await pending;
  f.browser.work.dataset.workRevision='7';f.edit('Original housekeeping note');await f.submit();
  const calls=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(calls).toHaveLength(2);expect(calls[1][1]?.body).toBe(original?.body);expect(new Headers(calls[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(original?.headers).get('Idempotency-Key'));expect(f.reset).toHaveBeenCalledOnce();expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);expect(f.completions).toEqual([{form:f.form,formEditVersion:'original-edit'}]);expect(f.attempts.current.size).toBe(0);
 }finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS Form preserves notes edited while the submitted body is decoding',async owner=>{
 const f=await actualPmsWorkspaceSend(owner),pending=f.submit();
 try{await f.started;f.edit('Later housekeeping note');f.release();await pending;expect(f.draft()).toBe('Later housekeeping note');expect(f.reset).not.toHaveBeenCalled();expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);expect(f.attempts.current.size).toBe(0);}finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS task completion retains newer checklist and notes through a successful clean refresh as a local draft',async owner=>{
 const checklist=['Wiped surfaces','Restocked towels'],latest={checklist:[true,false],notes:'Found a loose hinge'};
 const f=await actualPmsWorkspaceSend(owner,{holdRefresh:true,completionChecklist:checklist,taskCompletesOnRefresh:true}),pending=f.submit();
 try{
  await f.started;const originalAttempt=f.attempts.current.values().next().value;if(!originalAttempt)throw Error('Expected the original task completion attempt.');f.release();await f.readStarted;f.editCompletion(latest);f.readRelease();await pending;
  expect(f.captureCompletionDraft).toHaveBeenCalledTimes(1);expect(f.captureCompletionDraft).toHaveBeenCalledWith({checklist:[{label:checklist[0],checked:true},{label:checklist[1],checked:false}],notes:latest.notes,attachments:[]});
  expect(f.completionAcknowledged).toHaveBeenCalledOnce();
  expect(f.captureCompletionDraft.mock.invocationCallOrder[0]).toBeLessThan(f.setData.mock.invocationCallOrder[0]);
  expect(f.setData).toHaveBeenCalledOnce();expect(f.setData.mock.calls[0][0].snapshot?.records).toEqual([{kind:'task',id:'original-task',version:5,data:{state:'clean'}}]);
  expect(f.form.isConnected).toBe(false);expect(f.reset).not.toHaveBeenCalled();expect(f.attempts.current.size).toBe(0);
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(1);expect(posts[0][1]?.body).toBe(originalAttempt.body);expect(new Headers(posts[0][1]?.headers).get('Idempotency-Key')).toBe(originalAttempt.key);
 }finally{f.release();f.readRelease();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS task completion blocks while an attachment set is being created or uploaded',async owner=>{
 const f=await actualPmsWorkspaceSend(owner);f.setAttachmentActivity(true,'room_photo');f.setAttachmentActivity(true,'damage_photo');f.setAttachmentActivity(false,'room_photo');
 try{
  expect(f.attachmentPending()).toBe(true);await f.submit();expect(f.fetcher).not.toHaveBeenCalled();expect(f.formError).toHaveBeenCalledWith('Wait for attachment setup or upload to finish before completing this task.');expect(f.attempts.current.size).toBe(0);
  f.setAttachmentActivity(false,'damage_photo');expect(f.attachmentPending()).toBe(false);const pending=f.submit();await f.started;f.release();await pending;expect(f.fetcher).toHaveBeenCalledTimes(2);expect(f.reset).toHaveBeenCalledOnce();
 }finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS task form blocks a second submission while any original command is unresolved',async owner=>{
 const f=await actualPmsWorkspaceSend(owner);f.setPending(true);
 try{
  await f.submit();expect(f.fetcher).not.toHaveBeenCalled();expect(f.attempts.current.size).toBe(0);expect(f.formError).toHaveBeenCalledWith('Resolve the pending command before completing another task.');
  f.setPending(false);const pending=f.submit();await f.started;f.release();await pending;expect(f.fetcher).toHaveBeenCalledTimes(2);
 }finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS task completion reuses its exact request and preserves later edits after retry acknowledgement',async owner=>{
 const latest={checklist:[true,false],notes:'Retained while the result was unknown'},f=await actualPmsWorkspaceSend(owner,{failFirstBody:true,completionChecklist:['Wiped surfaces','Restocked towels']}),first=f.submit();
 try{
  await f.started;f.setPending(true);f.editCompletion(latest);f.release();await first;expect(f.attempts.current.size).toBe(1);const original=f.fetcher.mock.calls.find(([,init])=>init?.method==='POST')?.[1];
  await f.submit();expect(f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST')).toHaveLength(1);expect(await f.retry()).toBe(true);
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(original?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(original?.headers).get('Idempotency-Key'));expect(f.completionAcknowledged).toHaveBeenCalledOnce();expect(f.captureCompletionDraft).toHaveBeenCalledTimes(1);expect(f.captureCompletionDraft).toHaveBeenCalledWith({checklist:[{label:'Wiped surfaces',checked:true},{label:'Restocked towels',checked:false}],notes:latest.notes,attachments:[]});expect(f.attempts.current.size).toBe(0);
 }finally{f.release();f.reads.cancel();}
});
it('renders completion drafts with the captured values and explicit keep-editing/discard controls, without a submit form',()=>{
 const html=renderToStaticMarkup(React.createElement(TaskCompletionDrafts,{drafts:[{key:'draft',base:'/front/acme/acme-co',attachmentBase:'/front/acme/acme-co/pms',propertyId:'original-hotel',taskId:'original-task',taskLabel:'Mark task complete',checklist:[{label:'Wiped surfaces',checked:true},{label:'Restocked towels',checked:false}],notes:'Found a loose hinge',attachments:[{purposeKey:'room_photo',setId:'attachment-set',readyCount:1,requiredCount:0,bindable:true,links:[{id:'attachment-link',filename:'room-after.jpg',mediaType:'image/jpeg',bytes:1024,sourceState:'available',linkState:'draft',caption:null,previewAvailable:true}]}],editing:false}],onUpdate:vi.fn(),onDiscard:vi.fn()}));
 expect(html).toContain('Local reference · Later changes were not submitted');expect(html).toContain('data-work-dirty="false"');expect(html).toContain('Wiped surfaces');expect(html).toContain('Restocked towels');expect(html).toContain('Found a loose hinge');expect(html).toContain('room-after.jpg');expect(html).toContain('attachment-set');expect(html).toContain('attachment-link');expect(html).toContain('/api/v1/front/acme/acme-co/pms/attachment-links/attachment-link/content');expect(html).toContain('Open sanitized preview');expect(html).toContain('Download private original');expect(html).toContain('Copy local reference');
 expect(html).toContain('checked=""');expect(html).toContain('disabled=""');expect(html).toContain('Keep editing');expect(html).toContain('Discard draft');expect(html).not.toContain('<form');expect(html.match(/type="button"/g)).toHaveLength(3);
});
it.each(['front','backoffice'] as const)('actual %s housekeeping owner exposes retained completion drafts through its rendered screen',async owner=>{
 const ts=await import('typescript'),{readFile}=await import('node:fs/promises');
 const ownerSource=await readPmsOwnerSource(owner==='front'?actualPmsFrontSource:actualPmsBackofficeSource);
 const screenPath=owner==='front'?"../../../packages/feature-pms/src/components/pms-front-housekeeping-screen.tsx":"../../../packages/feature-pms/src/components/pms-backoffice-rooms-housekeeping-screen.tsx";
 const screenSource=await readFile(new URL(screenPath,import.meta.url),'utf8'),screen=ts.createSourceFile('actual-pms-housekeeping-screen.tsx',screenSource,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const rendered:import('typescript').JsxSelfClosingElement[]=[];
 const visit=(node:import('typescript').Node)=>{if(ts.isJsxSelfClosingElement(node)&&node.tagName.getText(screen)==='TaskCompletionDrafts')rendered.push(node);ts.forEachChild(node,visit);};visit(screen);
 expect(rendered).toHaveLength(1);
 const props=Object.fromEntries(rendered[0].attributes.properties.filter(ts.isJsxAttribute).map(attribute=>[
  attribute.name.getText(screen),attribute.initializer&&ts.isJsxExpression(attribute.initializer)?attribute.initializer.expression?.getText(screen):undefined,
 ]));
 expect(props).toMatchObject({drafts:'taskCompletionDrafts'});
 expect(props.onUpdate).toBe(owner==='front'?'onUpdateTaskCompletionDraft':'updateTaskCompletionDraft');
 expect(props.onDiscard).toBe(owner==='front'?'onDiscardTaskCompletionDraft':'discardTaskCompletionDraft');
 if(owner==='front'){
  expect(ownerSource).toContain('taskCompletionDrafts={visibleTaskCompletionDrafts}');
  expect(ownerSource).toContain('onUpdateTaskCompletionDraft={updateTaskCompletionDraft}');
  expect(ownerSource).toContain('onDiscardTaskCompletionDraft={discardTaskCompletionDraft}');
  expect(ownerSource).toContain('section === "housekeeping" && <FrontPmsDailyHousekeepingScreen context={context}/>');
 }else{
  expect(ownerSource).toContain('taskCompletionDrafts:visibleTaskCompletionDrafts');
  expect(ownerSource).toContain('updateTaskCompletionDraft, discardTaskCompletionDraft');
  expect(ownerSource).toContain('<BackofficePmsWorkspaceRoomsHousekeepingScreen context={context}/>');
 }
});
it.each(['front','backoffice'] as const)('actual %s PMS retains the mapped attachment API base in a completion draft',async owner=>{
 const ts=await import('typescript'),source=await readPmsOwnerSource(owner==='front'?actualPmsFrontSource:actualPmsBackofficeSource),file=ts.createSourceFile('actual-pms-completion-base.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);let mapBase:import('typescript').ArrowFunction|undefined,capture:import('typescript').ArrowFunction|undefined;
 const visit=(node:import('typescript').Node)=>{if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text==='pmsFileBase'&&node.initializer&&ts.isArrowFunction(node.initializer))mapBase=node.initializer;if(ts.isJsxAttribute(node)&&node.name.getText(file)==='onCompletionDraft'&&node.initializer&&ts.isJsxExpression(node.initializer)&&node.initializer.expression){const find=(child:import('typescript').Node)=>{if(ts.isArrowFunction(child)&&child.parameters[0]?.name.getText(file)==='values'&&child.getText(file).includes('attachmentBase:'))capture=child;ts.forEachChild(child,find);};find(node.initializer.expression);}ts.forEachChild(node,visit);};visit(file);
 if(!mapBase||!capture)throw Error('The actual PMS completion-draft mapper and capture callback are required.');
 const mapBaseJs=ts.transpileModule(`return (${mapBase.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,mapBasePort=new Function(mapBaseJs)() as (base:string)=>string,base=owner==='front'?'/front/acme/acme-co':'/holdings/acme',attachments=[{base}],choiceApi=`/api/v1${base}/pms`,hidden={id:'original-task'};let drafts:Record<string,Record<string,unknown>>={};
 const setTaskCompletionDrafts=(update:(current:Record<string,Record<string,unknown>>)=>Record<string,Record<string,unknown>>)=>{drafts=update(drafts);};
 const captureDraftJs=ts.transpileModule(`return (${capture!.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,captureDraft=new Function('hidden','propertyId','base','attachments','choiceApi','setTaskCompletionDrafts','title','pmsFileBase',captureDraftJs)(hidden,'original-hotel',base,attachments,choiceApi,setTaskCompletionDrafts,'Mark task complete',mapBasePort) as (values:unknown)=>void;
 captureDraft({checklist:[],notes:'Later note',attachments:[]});const saved=drafts[JSON.stringify([base,'original-hotel','original-task'])];
 expect(saved?.attachmentBase).toBe(owner==='front'?'/front/acme/acme-co/pms':'/holdings/acme');
 expect(attachmentContentUrl(String(saved?.attachmentBase),'attachment-link','preview')).toContain(owner==='front'?'/api/v1/front/acme/acme-co/pms/attachment-links/attachment-link/content':'/api/v1/holdings/acme/attachment-links/attachment-link/content');
});
it('retains only attachment states changed after the original completion request by purpose',()=>{
 const submitted=new Map([['room_photo',{purposeKey:'room_photo',setId:'original-set',readyCount:1,requiredCount:0,bindable:true,links:[{id:'old-link',filename:'before.jpg',mediaType:'image/jpeg',bytes:10,sourceState:'available',linkState:'draft',caption:null,previewAvailable:true}]}]]);
 const current=new Map([['room_photo',{purposeKey:'room_photo',setId:'original-set',readyCount:2,requiredCount:0,bindable:true,links:[{id:'old-link',filename:'before.jpg',mediaType:'image/jpeg',bytes:10,sourceState:'available',linkState:'draft',caption:null,previewAvailable:true},{id:'new-link',filename:'after.jpg',mediaType:'image/jpeg',bytes:20,sourceState:'available',linkState:'draft',caption:null,previewAvailable:true}]}],['damage_photo',{purposeKey:'damage_photo',setId:'damage-set',readyCount:0,requiredCount:0,bindable:true,links:[]}]]);
 expect(changedPmsTaskCompletionAttachments(submitted,current)).toEqual([current.get('room_photo'),current.get('damage_photo')]);
});
it.each(['front','backoffice'] as const)('actual %s PMS task completion keeps later edits after the acknowledged command when its optional refresh fails',async owner=>{
 const checklist=['Wiped surfaces','Restocked towels'],latest={checklist:[true,false],notes:'Found a loose hinge'},f=await actualPmsWorkspaceSend(owner,{failRefresh:true,completionChecklist:checklist}),pending=f.submit();
 try{
  await f.started;const originalAttempt=f.attempts.current.values().next().value;if(!originalAttempt)throw Error('Expected the original task completion attempt.');f.editCompletion(latest);f.release();await pending;
  expect(f.completionAcknowledged).toHaveBeenCalledOnce();
  expect(f.captureCompletionDraft).toHaveBeenCalledTimes(1);expect(f.captureCompletionDraft).toHaveBeenCalledWith({checklist:[{label:checklist[0],checked:true},{label:checklist[1],checked:false}],notes:latest.notes,attachments:[]});
  expect(f.setData).not.toHaveBeenCalled();expect(f.form.isConnected).toBe(true);expect(f.draft()).toBe(latest.notes);expect(f.reset).not.toHaveBeenCalled();expect(f.attempts.current.size).toBe(0);
  const post=f.fetcher.mock.calls.find(([,init])=>init?.method==='POST');expect(post?.[1]?.body).toBe(originalAttempt.body);expect(new Headers(post?.[1]?.headers).get('Idempotency-Key')).toBe(originalAttempt.key);
 }finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS send refuses a successful malformed receipt without acknowledging the submitted form',async owner=>{
 const f=await actualPmsWorkspaceSend(owner);f.body({data:{}});const pending=f.submit();
 try{await f.started;f.release();await pending;expect(f.reset).not.toHaveBeenCalled();expect(f.events).toEqual([]);expect(f.attempts.current.size).toBe(1);expect(f.fetcher).toHaveBeenCalledTimes(1);}finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS send acknowledges a valid current receipt once and refreshes the original hotel',async owner=>{
 const f=await actualPmsWorkspaceSend(owner),pending=f.submit();
 try{await f.started;const beforeBody={events:[...f.events],attempts:f.attempts.current.size};f.release();await pending;expect(beforeBody).toEqual({events:[],attempts:1});expect(f.reset).toHaveBeenCalledOnce();expect(f.setSuccess).toHaveBeenCalledWith(true);expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);expect(f.attempts.current.size).toBe(0);expect(f.setData).toHaveBeenCalledOnce();expect(new URL(String(f.fetcher.mock.calls[1][0]),window.location.origin).searchParams.get('property')).toBe('original-hotel');expect(f.pendingWrites.current).toBe(0);}finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS send retains its committed exact attempt when context changes during the optional refresh original body',async owner=>{
 const f=await actualPmsWorkspaceSend(owner,{holdRefresh:true}),pending=f.submit();
 try{
  await f.started;f.release();await f.readStarted;const original=f.fetcher.mock.calls[0][1];
  changePmsContext(f.browser,owner==='front'?'Front location':'revision');f.readRelease();await pending;
  expect(f.reset).not.toHaveBeenCalled();expect(f.setData).not.toHaveBeenCalled();expect(f.events).toEqual([]);expect(f.attempts.current.size).toBe(1);
  f.browser.front.dataset.frontLocation='holding';f.browser.work.dataset.workRevision='7';await f.submit();
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(original?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(original?.headers).get('Idempotency-Key'));expect(f.reset).toHaveBeenCalledOnce();expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);
 }finally{f.release();f.readRelease();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS send keeps a current committed command saved when its optional refresh fails',async owner=>{
 const f=await actualPmsWorkspaceSend(owner,{failRefresh:true}),pending=f.submit();
 try{await f.started;f.release();await pending;expect(f.reset).toHaveBeenCalledOnce();expect(f.attempts.current.size).toBe(0);expect(f.setData).not.toHaveBeenCalled();expect(f.setNotice).toHaveBeenLastCalledWith('Saved successfully · Reload to see the latest result');expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);expect(f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST')).toHaveLength(1);}finally{f.release();f.reads.cancel();}
});
it('actual Backoffice property creation accepts the native new-Property receipt and routes only after final acknowledgement',async()=>{
 const f=await actualPmsWorkspaceSend('backoffice',{operation:'property.create'}),pending=f.submit();
 try{await f.started;expect(f.router.push).not.toHaveBeenCalled();expect(f.events).toEqual([]);f.release();await pending;expect(f.reset).toHaveBeenCalledOnce();expect(JSON.parse(String(f.fetcher.mock.calls[0][1]?.body))).not.toHaveProperty('expectedVersion');expect(f.router.push).toHaveBeenCalledExactlyOnceWith('/holdings/acme/pms/settings?property='+f.nativeCreateId());expect(f.router.refresh).toHaveBeenCalledOnce();expect(f.attempts.current.size).toBe(0);expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);}finally{f.release();f.reads.cancel();}
});
it('actual PMS Form handles an already-stale work context locally without sending or resetting',async()=>{
 const f=await actualPmsWorkspaceSend('front');f.browser.work.dataset.workStale='true';
 try{await f.submit();expect(f.fetcher).not.toHaveBeenCalled();expect(f.reset).not.toHaveBeenCalled();expect(f.events).toEqual([]);expect(f.formError).toHaveBeenLastCalledWith('Not saved. Check the message above; your form data is preserved.');expect(f.pendingWrites.current).toBe(0);expect(f.draft()).toBe('Original housekeeping note');}finally{f.release();f.reads.cancel();}
});


it.each(['front','backoffice'] as const)('actual %s PMS blocks a newer body after unknown and explicitly recovers only the first request without resetting newer edits',async owner=>{
 const f=await actualPmsWorkspaceSend(owner,{failFirstBody:true}),first=f.submit();
 try{
  await f.started;const original=f.fetcher.mock.calls[0][1];f.release();await first;expect(f.attempts.current.size).toBe(1);
  f.edit('Newer unsent housekeeping note');await f.submit();
  expect(f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST')).toHaveLength(1);expect(f.reset).not.toHaveBeenCalled();expect(f.setError).toHaveBeenLastCalledWith('Confirm the original hotel action first. Your newer entries are retained.');
  expect(await f.retry()).toBe(true);
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(original?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(original?.headers).get('Idempotency-Key'));
  expect(f.draft()).toBe('Newer unsent housekeeping note');expect(f.reset).not.toHaveBeenCalled();expect(f.editingVersion.current).toBeUndefined();expect(f.attempts.current.size).toBe(0);expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);expect(f.completions).toEqual([{form:f.form,formEditVersion:'original-edit'}]);
 }finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS first native room rejection permits correction but a later rejection cannot erase a previous unknown',async owner=>{
 const f=await actualPmsWorkspaceSend(owner);f.response(409,{error:{code:'PMS_ROOM_OVERLAP',message:'This room is assigned to another stay.'}});
 const first=f.execute('stay.assign',{id:'original-stay',roomId:'occupied-room',reason:'Original room choice'},3);
 try{
  await f.started;const original=f.fetcher.mock.calls[0][1];f.release();await first;expect(f.attempts.current.size).toBe(0);
  f.response(200,{data:{id:'original-stay',propertyId:'original-hotel',version:4}});expect(await f.execute('stay.assign',{id:'original-stay',roomId:'inspected-room',reason:'Reviewed room choice'},3)).toBe(true);
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).not.toBe(new Headers(original?.headers).get('Idempotency-Key'));expect(JSON.parse(String(posts[1][1]?.body)).input.roomId).toBe('inspected-room');
 }finally{f.release();f.reads.cancel();}
 const unknown=await actualPmsWorkspaceSend(owner,{failFirstBody:true}),pending=unknown.execute('stay.assign',{id:'original-stay',roomId:'occupied-room',reason:'Original room choice'},3);
 try{
  await unknown.started;const original=unknown.fetcher.mock.calls[0][1];unknown.release();await pending;unknown.response(409,{error:{code:'PMS_ROOM_OVERLAP',message:'Current room overlap'}});expect(await unknown.retry()).toBe(false);expect(unknown.attempts.current.size).toBe(1);
  const posts=unknown.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(original?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(original?.headers).get('Idempotency-Key'));
 }finally{unknown.release();unknown.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS first closed native schema rejection permits edited fields and unclassified first denial remains pending',async owner=>{
 const invalid=await actualPmsWorkspaceSend(owner);invalid.response(422,{error:{code:'VALIDATION_FAILED',message:'Enter valid housekeeping fields.'}});const first=invalid.submit();
 try{await invalid.started;invalid.release();await first;expect(invalid.attempts.current.size).toBe(0);expect(invalid.draft()).toBe('Original housekeeping note');expect(invalid.reset).not.toHaveBeenCalled();}finally{invalid.release();invalid.reads.cancel();}
 const denied=await actualPmsWorkspaceSend(owner);denied.response(403,{error:{code:'FORBIDDEN',message:'Current owner refused.'}});const pending=denied.submit();
 try{await denied.started;denied.release();await pending;expect(denied.attempts.current.size).toBe(1);denied.edit('Newer retained note');await denied.submit();expect(denied.fetcher.mock.calls.filter(([,init])=>init?.method==='POST')).toHaveLength(1);expect(denied.draft()).toBe('Newer retained note');expect(denied.events).toEqual([]);}finally{denied.release();denied.reads.cancel();}
});
it.each((['front','backoffice'] as const).flatMap(owner=>(['pathname','query'] as const).map(change=>[owner,change] as const)))('actual %s PMS keeps its original pending request after %s changes during original JSON',async(owner,change)=>{
 const f=await actualPmsWorkspaceSend(owner),first=f.submit();
 try{await f.started;if(change==='pathname')window.location.pathname='/another-hotel';else window.location.search='?page=2';f.release();await first;expect(f.attempts.current.size).toBe(1);expect(f.reset).not.toHaveBeenCalled();expect(f.setData).not.toHaveBeenCalled();expect(f.events).toEqual([]);}finally{f.release();f.reads.cancel();}
});
it('actual Backoffice property creation acknowledges its original form before publishing the new Property source identity',async()=>{
 const f=await actualPmsWorkspaceSend('backoffice',{operation:'property.create',reflectCreatedSource:true}),first=f.submit();
 try{await f.started;f.release();await first;expect(f.reset).toHaveBeenCalledOnce();expect(f.attempts.current.size).toBe(0);expect(f.setData).toHaveBeenCalledOnce();expect(f.setSuccess.mock.invocationCallOrder[0]).toBeLessThan(f.setData.mock.invocationCallOrder[0]);expect(f.router.push).toHaveBeenCalledExactlyOnceWith('/holdings/acme/pms/settings?property='+f.nativeCreateId());expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);}finally{f.release();f.reads.cancel();}
});
it.each([
 ['payment.request_transfer',{guestAccountId:'original-account',shiftId:'original-shift',amount:1000,reference:'Original transfer',kind:'receipt'},{id:'new-request',propertyId:'original-hotel',version:3,state:'pending',transferVersion:1}],
 ['payment.cancel_transfer',{id:'original-request',expectedTransferVersion:2},{id:'original-request',propertyId:'original-hotel',version:3,state:'cancelled',transferVersion:3}],
 ['payment.verify_transfer',{id:'original-request',expectedTransferVersion:2,bankTransactionId:'original-bank-row',bankVersion:1},{id:'original-request',propertyId:'original-hotel',version:4,state:'verified',transferVersion:3,entryId:'new-entry',bankTransactionId:'original-bank-row'}],
] as const)('PMS %s validates its exact captured native receipt version and identity',async(operation,input,result)=>{
 pmsReadBrowser();const attempts=new Map<string,PmsWorkspaceCommandAttempt>(),source={base:'/holdings/acme',propertyId:'original-hotel',section:'cashier'},body=JSON.stringify({operation,propertyId:source.propertyId,expectedVersion:3,input,attachmentSetIds:[]});
 let reply:unknown={...result,version:99};const fetcher=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>Response.json({data:reply}));vi.stubGlobal('fetch',fetcher);
 await expect(sendPmsWorkspaceCommand(attempts,()=>source,'native-signature',body,async()=>true)).rejects.toThrow('version');expect(attempts.size).toBe(1);
 reply=result;const completed=await sendPmsWorkspaceCommand(attempts,()=>source,'native-signature',body,async()=>true);expect(completed.acknowledge()).toBe(true);expect(attempts.size).toBe(0);expect(new Headers(fetcher.mock.calls[1]?.[1]?.headers).get('Idempotency-Key')).toBe(new Headers(fetcher.mock.calls[0]?.[1]?.headers).get('Idempotency-Key'));
});


it.each(['front','backoffice'] as const)('actual %s PMS editor keeps original recovery inside its retained pending disclosure',async owner=>{
 const {createRequire}=await import('node:module'),ts=await import('typescript'),require=createRequire(import.meta.url);
 const path=owner==='front'?actualPmsFrontSource:actualPmsBackofficeSource,source=await readPmsOwnerSource(path),file=ts.createSourceFile('actual-pms-editor-recovery.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),Form=file.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='Form');
 if(!Form)throw Error('The actual PMS editor is required.');
 const observed:Record<string,unknown>[]=[],ports={useFrontLanguage:()=> 'en',frontText:(text:string)=>text,useState:React.useState,useRef:React.useRef,useLayoutEffect:React.useLayoutEffect,operationPermission:()=> 'pms.housekeeping',FormDisclosure:(props:Record<string,unknown>)=>{observed.push(props);return props.children as React.ReactNode;},Plus:()=>null,PmsChoiceSelect:()=>null,AttachmentPanel:()=>null,UiField:({children}:{children:React.ReactNode})=>React.createElement('label',null,children),Select:'select',TextArea:'textarea',TextInput:'input',Button:({children,type,disabled}:{children:React.ReactNode;type?:'button'|'submit';disabled?:boolean})=>React.createElement('button',{type,disabled},children),styles:new Proxy({}, {get:(_target,key)=>String(key)})};
 const compiled=ts.transpileModule(Form.getText(file),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}}).outputText;
 const Component=new Function('require','exports',...Object.keys(ports),compiled+';return Form;')(require,{},...Object.values(ports)) as React.ComponentType<Record<string,unknown>>;
 const html=renderToStaticMarkup(React.createElement(Component,{title:'Complete room task',op:'task.complete',fields:[{name:'first',label:'First',type:'text'},{name:'second',label:'Second',type:'text'},{name:'notes',label:'Notes',type:'textarea'}],permissions:['pms.housekeeping'],send:vi.fn(async()=>false),pending:true,recovery:React.createElement('p',null,'Captured original recovery')}));
 expect(observed).toHaveLength(1);expect(observed[0]).toMatchObject({inline:false,dismissDisabled:true,retainFormContents:true});expect(html).toContain('Captured original recovery');expect(html).toContain('name="notes"');
});
it('actual record drawer keeps optional owner recovery inside the detail boundary during its initial read while the default content remains unchanged',async()=>{
 const {readFile}=await import('node:fs/promises'),{createRequire}=await import('node:module'),ts=await import('typescript'),require=createRequire(import.meta.url),source=await readFile(new URL("../../../packages/feature-records/src/components/record-collection.tsx",import.meta.url),'utf8'),file=ts.createSourceFile('actual-record-recovery.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),declaration=file.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='RecordCollectionDrawer');
 if(!declaration)throw Error('The real owner drawer is required.');
 const observed:Record<string,unknown>[]=[],fetcher=vi.fn(),ports={useState:React.useState,useRef:React.useRef,useEffect:React.useEffect,usePathname:()=>'/original-record',useSearchParams:()=>new URLSearchParams({detailKind:'pms_entry',detailId:'original-entry'}),workFetch:fetcher,RecordDrawer:(props:Record<string,unknown>)=>{observed.push(props);return React.createElement('section',null,props.children as React.ReactNode);}};
 const compiled=ts.transpileModule(declaration.getText(file),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}}).outputText;
 const vm={exports:{} as Record<string,unknown>};new Function('require','module','exports',...Object.keys(ports),compiled)(require,vm,vm.exports,...Object.values(ports));
 const Drawer=vm.exports.RecordCollectionDrawer as React.ComponentType<Record<string,unknown>>,props={kind:'pms_entry',title:'Original entry',endpoint:'/original-owner',select:()=>null};
 const normal=renderToStaticMarkup(React.createElement(Drawer,props));expect(normal).toContain('Loading record details');expect(React.isValidElement(observed[0].children)&&observed[0].children.type).toBe('p');
 const pending=renderToStaticMarkup(React.createElement(Drawer,{...props,recovery:React.createElement('p',null,'Captured original Retry')}));expect(pending).toContain('Captured original Retry');expect(pending).toContain('Loading record details');expect(fetcher).not.toHaveBeenCalled();
});

it.each(['front','backoffice'] as const)('actual %s PMS first native version rejection releases only its captured request version and preserves corrected fields',async owner=>{
 const f=await actualPmsWorkspaceSend(owner);f.response(409,{error:{code:'PMS_VERSION_CONFLICT',message:'Hotel changed. Review the latest version.'}});const first=f.submit();
 try{
  await f.started;f.release();await first;expect(f.attempts.current.size).toBe(0);expect(f.editingVersion.current).toBeUndefined();expect(f.reset).not.toHaveBeenCalled();expect(f.events).toEqual([]);
  f.ownerVersion(5);f.edit('Corrected note after owner refresh');f.response(200,{data:{id:'original-task',propertyId:'original-hotel',version:6}});await f.submit();
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(JSON.parse(String(posts[1][1]?.body)).expectedVersion).toBe(5);expect(JSON.parse(String(posts[1][1]?.body)).input.notes).toBe('Corrected note after owner refresh');expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).not.toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));
 }finally{f.release();f.reads.cancel();}
});

it.each((['front','backoffice'] as const).flatMap(owner=>([
 ['reservation.create','PMS_RATE_UNAVAILABLE',409,{rooms:[]},'new-reservation'],
 ['accounting.save','PMS_ACCOUNT_INVALID',422,{chart:'original-chart'},'original-hotel'],
 ['supply.consume','PMS_STOCK_SHORT',409,{itemId:'original-item',quantity:20},'new-movement'],
] as const).map(([operation,code,status,input,resultId])=>[owner,operation,code,status,input,resultId] as const)))('actual %s PMS %s first native %s permits a corrected body without a saved event for the rejected request',async(owner,operation,code,status,input,resultId)=>{
 const f=await actualPmsWorkspaceSend(owner);f.response(status,{error:{code,message:'Review the native source and correct the fields.'}});const first=f.execute(operation,input,3);
 try{
  await f.started;f.release();await first;expect(f.attempts.current.size).toBe(0);expect(f.events).toEqual([]);expect(f.reset).not.toHaveBeenCalled();
  f.response(200,{data:{id:resultId,propertyId:'original-hotel',version:4}});expect(await f.execute(operation,{...input,reason:'Corrected original owner input'},3)).toBe(true);
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(new Headers(posts[0][1]?.headers).get('Idempotency-Key')).not.toBe(new Headers(posts[1][1]?.headers).get('Idempotency-Key'));expect(JSON.parse(String(posts[1][1]?.body)).input.reason).toBe('Corrected original owner input');
 }finally{f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s PMS refuses another native task ID at the captured property version and retries the exact original target',async owner=>{
 const f=await actualPmsWorkspaceSend(owner);f.body({data:{id:'another-task',propertyId:'original-hotel',version:4}});const first=f.submit();
 try{
  await f.started;f.release();await first;expect(f.attempts.current.size).toBe(1);expect(f.reset).not.toHaveBeenCalled();expect(f.events).toEqual([]);
  f.body({data:{id:'original-task',propertyId:'original-hotel',version:4}});expect(await f.retry()).toBe(true);
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));expect(f.reset).toHaveBeenCalledOnce();expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);
 }finally{f.release();f.reads.cancel();}
});

it('actual Backoffice creation retains a wrong Property acknowledgement and recovers only the original key-derived native target',async()=>{
 const f=await actualPmsWorkspaceSend('backoffice',{operation:'property.create'});f.body({data:{id:'unrelated-property',propertyId:'unrelated-property',version:1}});const first=f.submit();
 try{
  await f.started;const original=f.fetcher.mock.calls[0][1];f.release();await first;expect(f.nativeCreateId()).toMatch(/^property-[a-f0-9]{32}$/);expect(f.attempts.current.size).toBe(1);expect(f.reset).not.toHaveBeenCalled();expect(f.router.push).not.toHaveBeenCalled();expect(f.events).toEqual([]);
  f.nativeCreateReceipt();expect(await f.retry()).toBe(true);
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(original?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(original?.headers).get('Idempotency-Key'));expect(f.attempts.current.size).toBe(0);expect(f.reset).toHaveBeenCalledOnce();expect(f.router.push).toHaveBeenCalledExactlyOnceWith('/holdings/acme/pms/settings?property='+f.nativeCreateId());expect(f.events).toEqual(['erpx:record-command-saved','erpx:work-changed']);
 }finally{f.release();f.reads.cancel();}
});

import {installPmsPendingNavigation,pmsPendingDepartureAllowed,samePmsCommandView} from '@erpx/feature-pms/lib/pending-navigation';
import {hasUnsavedWork,confirmContextSwitch,contextSwitchBlockReason} from '@erpx/feature-work_projection/lib/context-switch';
import {destinationHref} from '@erpx/feature-shared/lib/destination-router';
import {isProductHandoffHref} from '@erpx/feature-work_projection/lib/product-navigation';

/** Run the actual owner pending callback and actual navigation hook with explicit
 * commit/cleanup ports, alongside the existing real native send-handler seam. */
async function actualPmsLifetimeMount(owner:PmsSendOwner,f:Awaited<ReturnType<typeof actualPmsWorkspaceSend>>){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript');
 const source=await readPmsOwnerSource(owner==='front'?actualPmsFrontSource:actualPmsBackofficeSource),file=ts.createSourceFile('actual-owner-lifetime.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 let pending:import('typescript').ArrowFunction|undefined,localDraftPending:import('typescript').ArrowFunction|undefined;
 const visit=(node:import('typescript').Node)=>{if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.initializer&&ts.isCallExpression(node.initializer)&&node.initializer.arguments[0]&&ts.isArrowFunction(node.initializer.arguments[0])){if(node.name.text==='hasPendingCommand')pending=node.initializer.arguments[0];if(node.name.text==='hasLocalTaskCompletionDraft')localDraftPending=node.initializer.arguments[0];}ts.forEachChild(node,visit);};visit(file);
 if(!pending||!localDraftPending)throw Error('The actual owner pending and local-draft lifetime callbacks are required.');
 const pendingJs=ts.transpileModule(`return (${pending.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const isPending=new Function('attempts','pendingWrites',pendingJs)(f.attempts,f.pendingWrites) as ()=>boolean;
 const draftCount={current:0},localDraftPendingJs=ts.transpileModule(`return (${localDraftPending.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const isLocalDraftPending=new Function('taskCompletionDraftCount','taskCompletionAttachmentActivities',localDraftPendingJs)(draftCount,f.ownerAttachmentActivities) as ()=>boolean;
 const text=await readFile(new URL("../../../packages/feature-pms/src/components/pending-command-navigation.tsx",import.meta.url),'utf8'),hookFile=ts.createSourceFile('actual-lifetime-hook.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),hook=hookFile.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='usePmsPendingNavigation');
 if(!hook)throw Error('The actual owner navigation hook is required.');
 const cleanups:(()=>void)[]=[],router={...f.router,replace:vi.fn()},ports={useCallback:(callback:unknown)=>callback,useMemo:(create:()=>unknown)=>create(),useLayoutEffect:(commit:()=>()=>void)=>{cleanups.push(commit());},useProductNavigationScope:()=>({audience:null}),destinationHref,isProductHandoffHref,installPmsPendingNavigation,pmsPendingDepartureAllowed};
 const js=ts.transpileModule(hook.getText(hookFile),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const guarded=new Function('exports',...Object.keys(ports),js+';return usePmsPendingNavigation;')({},...Object.values(ports))(router,isPending,'en',isLocalDraftPending) as {push:(href:string)=>void;replace:(href:string)=>void};
 return {guarded,router,isPending,isLocalDraftPending,draftCount,discardDraft:()=>{draftCount.current=0;},cleanup:()=>{cleanups.splice(0).forEach(cleanup=>cleanup());}};
}
async function actualAttachmentPanelSetActivity(retryUploads:{current:Map<string,unknown>},onActivityChange:(active:boolean)=>void){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),source=await readFile(new URL("../../../packages/feature-files/src/components/attachment-panel.tsx",import.meta.url),'utf8'),file=ts.createSourceFile('actual-attachment-panel.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 let callback:import('typescript').ArrowFunction|undefined;
 const visit=(node:import('typescript').Node)=>{if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text==='setActivity'&&node.initializer&&ts.isCallExpression(node.initializer)&&node.initializer.arguments[0]&&ts.isArrowFunction(node.initializer.arguments[0]))callback=node.initializer.arguments[0];ts.forEachChild(node,visit);};visit(file);
 if(!callback)throw Error('The actual AttachmentPanel activity callback is required.');
 const emitted=ts.transpileModule(`return (${callback.getText(file)});`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,busyRef={current:false},mounted={current:true},setBusy=vi.fn();
 const {pendingFieldWithdrawal}=await import('@erpx/feature-files/lib/native-field-withdraw'),{pendingFieldBind}=await import('@erpx/feature-files/lib/native-field-bind');
 return new Function('busyRef','mounted','setBusy','onActivityChange','retryUploads','pendingFieldWithdrawal','pendingFieldBind','withdrawalScope',emitted)(busyRef,mounted,setBusy,onActivityChange,retryUploads,pendingFieldWithdrawal,pendingFieldBind,'pms-test-scope') as (active:boolean)=>void;
}
function pmsLifetimeBrowser(f:Awaited<ReturnType<typeof actualPmsWorkspaceSend>>,withNavigation=true){
 const url=new URL('https://pms-dev-erpx.jesadakorn.com/acme/acme-co/holding/housekeeping?property=original-hotel&page=1');
 Object.assign(window.location,{href:url.href,origin:url.origin,pathname:url.pathname,search:url.search});
 const confirm=vi.fn(()=>false),navigation=new EventTarget(),history={pushState:vi.fn(),replaceState:vi.fn(),go:vi.fn()};
 Object.assign(window,{confirm,history,...(withNavigation?{navigation}:{})});
 class LifetimeAnchor{
  constructor(readonly href:string,readonly attributes:Record<string,string>={}){}
  get target(){return this.attributes.target??'';}
  hasAttribute(name:string){return Object.hasOwn(this.attributes,name);}
  closest(){return this;}
 }
 vi.stubGlobal('Element',LifetimeAnchor);
 const click=(href:string,attributes:Record<string,string>={},modifiers:Record<string,unknown>={})=>{
  const event=new Event('click',{cancelable:true});Object.defineProperties(event,Object.fromEntries(Object.entries({target:new LifetimeAnchor(new URL(href,url).href,attributes),button:0,...modifiers}).map(([key,value])=>[key,{value}])));f.browser.doc.dispatchEvent(event);return event;
 };
 const traverse=(href:string,options:{cancelable?:boolean;sameDocument?:boolean;navigationType?:string}={})=>{
  const event=Object.assign(new Event('navigate',{cancelable:options.cancelable??true}),{navigationType:options.navigationType??'traverse',destination:{url:new URL(href,url).href,sameDocument:options.sameDocument??true}});navigation.dispatchEvent(event);return event;
 };
 return {url,confirm,navigation,history,click,traverse};
}
it.each(['front','backoffice'] as const)('actual %s zero-field PMS task retains its mounted lifetime through original JSON, unknown outcome and exact recovery',async owner=>{
 const f=await actualPmsWorkspaceSend(owner),browser=pmsLifetimeBrowser(f),mounted=await actualPmsLifetimeMount(owner,f);
 const first=f.execute('task.start',{id:'original-task'},3);
 try{
  await f.started;expect(f.pendingWrites.current).toBe(1);expect(hasUnsavedWork()).toBe(true);expect(confirmContextSwitch()).toBe(false);
  expect(contextSwitchBlockReason()).toBe('Resolve the original hotel command before changing company, account, or role.');
  browser.confirm.mockReturnValue(true);expect(confirmContextSwitch()).toBe(false);expect(browser.confirm).not.toHaveBeenCalled();browser.confirm.mockReturnValue(false);
  expect(f.reset).not.toHaveBeenCalled();expect(f.draft()).toBe('Original housekeeping note');
  const unload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unload);expect(unload.defaultPrevented).toBe(true);
  mounted.guarded.push('?property=another-hotel');mounted.guarded.replace('?property=original-hotel&page=2');expect(mounted.router.push).not.toHaveBeenCalled();expect(mounted.router.replace).not.toHaveBeenCalled();
  expect(browser.click('/acme/acme-co/holding/front-desk?property=original-hotel').defaultPrevented).toBe(true);expect(browser.traverse('?property=original-hotel&page=2').defaultPrevented).toBe(true);
  // A late original body must keep the same native request after the actual
  // Work source changes; no editable field/onChange was involved.
  f.browser.work.dataset.workRevision='8';const original=f.fetcher.mock.calls[0][1];f.release();await first;
  expect(f.pendingWrites.current).toBe(0);expect(f.attempts.current.size).toBe(1);expect(hasUnsavedWork()).toBe(true);expect(f.events).toEqual([]);
  f.browser.work.dataset.workRevision='7';expect(await f.retry()).toBe(true);expect(hasUnsavedWork()).toBe(false);expect(contextSwitchBlockReason()).toBeNull();
  const posts=f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(original?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(original?.headers).get('Idempotency-Key'));
  mounted.guarded.push('?property=another-hotel');expect(mounted.router.push).toHaveBeenCalledExactlyOnceWith('?property=another-hotel',undefined);expect(browser.history.pushState).not.toHaveBeenCalled();expect(browser.history.replaceState).not.toHaveBeenCalled();expect(browser.history.go).not.toHaveBeenCalled();
 }finally{mounted.cleanup();f.release();f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s pending PMS navigation keeps native modified links, record overlays and global product-switch ownership',async owner=>{
 const f=await actualPmsWorkspaceSend(owner),browser=pmsLifetimeBrowser(f),mounted=await actualPmsLifetimeMount(owner,f),first=f.execute('task.start',{id:'original-task'},3);
 try{
  await f.started;
  for(const modifier of [{ctrlKey:true},{metaKey:true},{shiftKey:true},{altKey:true},{button:1}])expect(browser.click('?property=another-hotel',{},modifier).defaultPrevented).toBe(false);
  const excludedLinks:Record<string,string>[]=[{target:'_blank'},{download:''},{'data-product-switch':'true'}];
  for(const attributes of excludedLinks)expect(browser.click('?property=another-hotel',attributes).defaultPrevented).toBe(false);
  expect(browser.click('?property=original-hotel&page=1&detailKind=pms_entry&detailId=original-entry&returnTo=%2Foriginal').defaultPrevented).toBe(false);
  expect(browser.click('/acme/acme-co/holding/housekeeping/records/pms_record/original-task/overview?property=original-hotel&page=1').defaultPrevented).toBe(false);
  expect(browser.click('?page=1&property=original-hotel#original-task').defaultPrevented).toBe(false);expect(browser.confirm).not.toHaveBeenCalled();
  expect(browser.click('?property=another-hotel&page=1&detailKind=pms_entry&detailId=original-entry').defaultPrevented).toBe(true);
  browser.confirm.mockReturnValue(true);browser.confirm.mockClear();expect(browser.click('/acme/acme-co/holding/cashier?property=original-hotel').defaultPrevented).toBe(true);expect(browser.confirm).not.toHaveBeenCalled();expect(f.attempts.current.size).toBe(1);expect(f.events).toEqual([]);
 }finally{mounted.cleanup();f.release();await first;f.reads.cancel();}
});
it.each(['front','backoffice'] as const)('actual %s visible task-completion draft blocks same-view departure until explicitly discarded',async owner=>{
 const f=await actualPmsWorkspaceSend(owner),browser=pmsLifetimeBrowser(f),mounted=await actualPmsLifetimeMount(owner,f),sameView='?property=original-hotel&page=2';
 try{
  mounted.draftCount.current=1;
  expect(mounted.isPending()).toBe(false);expect(mounted.isLocalDraftPending()).toBe(true);expect(hasUnsavedWork()).toBe(true);expect(confirmContextSwitch()).toBe(false);browser.confirm.mockClear();
  const activeDraftUnload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(activeDraftUnload);expect(activeDraftUnload.defaultPrevented).toBe(true);
  mounted.guarded.push(sameView);expect(mounted.router.push).not.toHaveBeenCalled();expect(browser.confirm).toHaveBeenCalledExactlyOnceWith('The task-completion draft or attachment retry file may be lost if you leave this view. Leave this view?');
  expect(browser.click(sameView).defaultPrevented).toBe(true);expect(mounted.router.push).not.toHaveBeenCalled();expect(browser.confirm).toHaveBeenCalledTimes(2);
  mounted.discardDraft();expect(mounted.isLocalDraftPending()).toBe(false);expect(hasUnsavedWork()).toBe(false);
  const discardedDraftUnload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(discardedDraftUnload);expect(discardedDraftUnload.defaultPrevented).toBe(false);
  mounted.guarded.push(sameView);expect(mounted.router.push).toHaveBeenCalledExactlyOnceWith(sameView,undefined);expect(browser.confirm).toHaveBeenCalledTimes(2);
 }finally{mounted.cleanup();}
});
it.each(['front','backoffice'] as const)('actual %s PMS blocks navigation while an attachment upload or retained retry File is active',async owner=>{
 const f=await actualPmsWorkspaceSend(owner),browser=pmsLifetimeBrowser(f),mounted=await actualPmsLifetimeMount(owner,f),retryFiles={current:new Map<string,unknown>()},setActivity=await actualAttachmentPanelSetActivity(retryFiles,active=>f.setAttachmentActivity(active,'room_photo')),sameView='?property=original-hotel&page=2';
 try{
  setActivity(true);expect(f.attachmentPending()).toBe(true);expect(f.ownerAttachmentActivities.current.size).toBe(1);expect(mounted.isLocalDraftPending()).toBe(true);expect(hasUnsavedWork()).toBe(true);
  const uploadingUnload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(uploadingUnload);expect(uploadingUnload.defaultPrevented).toBe(true);
  mounted.guarded.push(sameView);expect(mounted.router.push).not.toHaveBeenCalled();expect(browser.confirm).toHaveBeenCalledExactlyOnceWith('The task-completion draft or attachment retry file may be lost if you leave this view. Leave this view?');
  expect(browser.click(sameView).defaultPrevented).toBe(true);expect(mounted.router.push).not.toHaveBeenCalled();
  // AttachmentPanel's finally reports active=false after an upload error, but
  // its retry map still owns the File; that local work must continue to guard.
  retryFiles.current.set('failed-link',{file:'in-memory-file'});setActivity(false);expect(f.attachmentPending()).toBe(true);expect(mounted.isLocalDraftPending()).toBe(true);
  retryFiles.current.clear();setActivity(false);expect(f.attachmentPending()).toBe(false);expect(mounted.isLocalDraftPending()).toBe(false);expect(hasUnsavedWork()).toBe(false);
  const resolvedUnload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(resolvedUnload);expect(resolvedUnload.defaultPrevented).toBe(false);browser.confirm.mockClear();mounted.guarded.push(sameView);expect(mounted.router.push).toHaveBeenCalledExactlyOnceWith(sameView,undefined);expect(browser.confirm).not.toHaveBeenCalled();
 }finally{mounted.cleanup();f.reads.cancel();}
});
it('PMS traversal cancellation uses only native cancelable same-document events and leaves unsupported fallback history untouched',async()=>{
 const f=await actualPmsWorkspaceSend('front'),browser=pmsLifetimeBrowser(f),mounted=await actualPmsLifetimeMount('front',f),first=f.execute('task.start',{id:'original-task'},3);
 try{
  await f.started;
  expect(browser.traverse('?property=another-hotel').defaultPrevented).toBe(true);
  expect(browser.traverse('?property=another-hotel',{cancelable:false}).defaultPrevented).toBe(false);
  expect(browser.traverse('?property=another-hotel',{sameDocument:false}).defaultPrevented).toBe(false);
  expect(browser.traverse('?property=another-hotel',{navigationType:'push'}).defaultPrevented).toBe(false);
  expect(browser.confirm).not.toHaveBeenCalled();
  const state={nativeNextState:'untouched'};Object.assign(window.history,{state});window.dispatchEvent(new Event('popstate'));expect(window.history.state).toBe(state);expect(browser.history.pushState).not.toHaveBeenCalled();expect(browser.history.replaceState).not.toHaveBeenCalled();expect(browser.history.go).not.toHaveBeenCalled();
  mounted.cleanup();expect(hasUnsavedWork()).toBe(false);expect(contextSwitchBlockReason()).toBeNull();expect(browser.traverse('?property=another-hotel').defaultPrevented).toBe(false);const unloaded=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unloaded);expect(unloaded.defaultPrevented).toBe(false);
 }finally{mounted.cleanup();f.release();await first;f.reads.cancel();}
});
it('PMS without Navigation API retains native full-document departure protection but does not pretend popstate can be cancelled',async()=>{
 const f=await actualPmsWorkspaceSend('backoffice'),browser=pmsLifetimeBrowser(f,false),mounted=await actualPmsLifetimeMount('backoffice',f),first=f.execute('task.start',{id:'original-task'},3);
 try{
  await f.started;const pop=new Event('popstate',{cancelable:false});window.dispatchEvent(pop);expect(pop.defaultPrevented).toBe(false);expect(browser.confirm).not.toHaveBeenCalled();
  const unload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unload);expect(unload.defaultPrevented).toBe(true);expect(hasUnsavedWork()).toBe(true);expect(browser.history.pushState).not.toHaveBeenCalled();expect(browser.history.replaceState).not.toHaveBeenCalled();
  setContextSwitching(true);const switching=new Event('beforeunload',{cancelable:true});window.dispatchEvent(switching);expect(switching.defaultPrevented).toBe(false);
 }finally{mounted.cleanup();f.release();await first;f.reads.cancel();}
});
it('PMS record view identity keeps business filters, query multiplicity and property scope while allowing the native drawer suffix',()=>{
 const current='https://pms-dev-erpx.jesadakorn.com/acme/co/holding/housekeeping?property=hotel&page=1&q=101&q=102';
 expect(samePmsCommandView(current,current.replace('/housekeeping?','/housekeeping/records/pms_record/task/overview?')+'&detailKind=pms_record&detailId=task')).toBe(true);
 for(const next of [current.replace('property=hotel','property=other'),current.replace('page=1','page=2'),current.replace('&q=102',''),current.replace('/housekeeping?','/rooms?'),current.replace('pms-dev','pos-dev')])expect(samePmsCommandView(current,next)).toBe(false);
});
it.each(['front','backoffice'] as const)('actual %s first proven PMS validation rejection ends the pending lifetime without changing unsent fields',async owner=>{
 const f=await actualPmsWorkspaceSend(owner);f.response(422,{error:{code:'VALIDATION_FAILED',message:'Review the original task source.'}});const browser=pmsLifetimeBrowser(f),mounted=await actualPmsLifetimeMount(owner,f),first=f.execute('task.start',{id:'original-task'},3);
 try{await f.started;expect(hasUnsavedWork()).toBe(true);expect(contextSwitchBlockReason()).toContain('Resolve the original hotel command');f.release();await first;expect(f.attempts.current.size).toBe(0);expect(hasUnsavedWork()).toBe(false);expect(contextSwitchBlockReason()).toBeNull();expect(f.reset).not.toHaveBeenCalled();expect(f.draft()).toBe('Original housekeeping note');expect(f.events).toEqual([]);expect(browser.click('?property=another-hotel').defaultPrevented).toBe(false);expect(browser.confirm).not.toHaveBeenCalled();}finally{mounted.cleanup();f.release();f.reads.cancel();}
});

// Closed setup queue ports exercise the real queue and original PMS engine.
// POS/Commerce fixtures below expose their actual documented receipt shapes;
// they do not claim native DB, money, permission or purchase acceptance.
import {createHospitalitySetupCommands,captureHospitalitySetupCompletion,hospitalitySetupQueueKey,legacyHospitalitySetupQueueKey,type HospitalitySetupSource,type HospitalitySetupDraft} from '@erpx/feature-shared/lib/hospitality-setup-command';
import {executePms as setupExecutePms,newProperty as setupNewProperty} from '@erpx/feature-pms/lib/pms-engine';
import {decimalToMinor as setupDecimalToMinor} from '@erpx/feature-finance/lib/finance-money';
import {createHash as setupHash} from 'node:crypto';
import {createRequire as setupRequire} from 'node:module';
import {readFileSync as setupRead} from 'node:fs';
import {resolve as setupResolve} from 'node:path';
import type {HospitalitySetup} from '@erpx/server-pms/public/hospitality-setup';
import {registerUnsavedWork as setupRegisterUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
function setupData():HospitalitySetup{
 const hotel=setupNewProperty('setup-hotel','setup-company','setup-branch','Reviewed hotel','2026-10-18');setupExecutePms(hotel,'room_type.save',{name:'Standard',capacity:2},{userId:'setup-user',now:1,uuid:()=> 'setup-type',independent:()=>true});
 return{companyId:'setup-company',companyName:'Reviewed Company',branchId:'setup-branch',branches:[{id:'setup-branch',name:'Reviewed branch'}],properties:[{id:hotel.property.id,name:hotel.property.name,state:'setup',business_date:hotel.property.businessDate}],propertyId:hotel.property.id,date:'2026-10-18',permissions:{hotel:true,restaurant:true,productCreate:true,financePrepare:true,financeApprove:false},steps:[{id:'finance',title:'Prepare Finance and stock',owner:'Finance preparer',href:'/finance/setup',blockers:['Configure Commerce accounts']},{id:'hotel',title:'Configure hotel',owner:'Hotel administrator',href:'/pms/settings',blockers:[]}],ready:false,hotel,posConfigured:true,posCurrency:'THB',warehouses:[{id:'setup-warehouse',name:'Kitchen'}],registers:[{id:'setup-register',name:'Restaurant',warehouse_id:'setup-warehouse',mode:'restaurant'}],products:[],tables:[],charts:[{id:'setup-chart',name:'THB native chart',base_currency:'THB'}],accounts:[],suggestions:[],roomChargeRequirements:['Same company and branch'],activationRestrictions:['Rehearsal does not approve production']};
}
function setupBrowser(data=setupData()){
 const values=new Map<string,string>(),storage:Storage={get length(){return values.size;},key:index=>[...values.keys()][index]??null,clear:()=>values.clear(),getItem:key=>values.get(key)??null,setItem:(key,value)=>{values.set(key,value);},removeItem:key=>{values.delete(key);}},work={isConnected:true,dataset:{workUser:'setup-user',workCompany:'setup-company',workAssignment:'setup-assignment',workRevision:'11',workStale:'false'}},location={origin:'https://console.example.test',pathname:'/holdings/setup/hospitality/setup',search:'?branch=setup-branch&property=setup-hotel'},surface=Object.assign(new EventTarget(),{location,confirm:vi.fn(()=>true)}),source:HospitalitySetupSource={base:'/holdings/setup',owner:{userId:'setup-user',companyId:'setup-company',branchId:null},data,canBootstrapPos:true,canReadPos:true};let live:HospitalitySetupSource|null=source;
 vi.stubGlobal('window',surface);vi.stubGlobal('document',{documentElement:{dataset:{}},activeElement:null,querySelector:(query:string)=>query==='[data-work-revision]'?work:null});vi.stubGlobal('localStorage',storage);setContextSwitching(false);const events:Event[]=[];surface.addEventListener('erpx:record-command-saved',event=>events.push(event));surface.addEventListener('erpx:work-changed',event=>events.push(event));return{source,storage,values,work,location,surface,events,read:()=>live,bind:(next:HospitalitySetupSource|null)=>{live=next;},commands:createHospitalitySetupCommands(()=>live,workFetch,()=>storage),key:hospitalitySetupQueueKey(source)};
}
function setupNative(c:ReturnType<typeof setupBrowser>){let view=structuredClone(c.source.data),count=0;const receipts=new Map<string,{body:string;data:Record<string,unknown>}>(),posts:{url:string;init:RequestInit}[]=[];
 async function request(url:RequestInfo|URL,init?:RequestInit):Promise<Response>{if(init?.method!=='POST')return Response.json({data:view});posts.push({url:String(url),init});const key=new Headers(init.headers).get('Idempotency-Key')!,body=String(init.body),prior=receipts.get(key);if(prior){expect(body).toBe(prior.body);return Response.json({data:prior.data});}const input=JSON.parse(body),path=String(url).split('/api/v1/holdings/setup/')[1];let result:Record<string,unknown>;
  if(path==='pms'){if(input.operation==='property.create'){const id='property-'+setupHash('sha256').update(key).digest('hex').slice(0,32),hotel=setupNewProperty(id,'setup-company','setup-branch',input.input.name,input.input.businessDate);view={...view,hotel,propertyId:id,properties:[{id,name:hotel.property.name}]};result={id,propertyId:id,version:1};}else{if(!view.hotel||input.propertyId!==view.hotel.property.id)return Response.json({error:{code:'PMS_NOT_FOUND',message:'Original hotel missing'}},{status:404});if(input.expectedVersion!==view.hotel.property.version)return Response.json({error:{code:'PMS_VERSION_CONFLICT',message:'Review latest original hotel version'}},{status:409});const outcome=setupExecutePms(view.hotel,input.operation,input.input,{userId:'setup-user',now:2,uuid:()=> 'setup-record-'+key,independent:()=>true});result={id:outcome.id,propertyId:view.hotel.property.id,version:view.hotel.property.version};}}
  else if(path==='commerce/products'){const id='products-'+key;view.products.push({id,sku:input.sku,name:input.name,price_minor:setupDecimalToMinor(input.price),version:1,kind:null,catalog_version:0});result={id,version:1};}
  else if(path==='pos/catalog/save'){view.products=view.products.map(row=>row.id===input.productId?{...row,kind:input.kind,station:input.station,recipe_json:JSON.stringify(input.recipe),catalog_version:1}:row);result={id:input.productId};}
  else if(path==='pos/table/create'){const id='setup-table-'+key;view.tables.push({id,register_id:input.registerId,name:input.name,seats:input.seats});result={id};}
  else if(path==='pos/register/save'){const id='setup-register-'+key;view.registers.push({id,name:input.name,warehouse_id:input.warehouseId,mode:input.mode});result={id};}
  else if(path==='pos/settings/bootstrap'){const id='setup-register-'+key;view.posConfigured=true;view.registers.push({id,name:input.registerName,warehouse_id:input.warehouseId,mode:'restaurant'});result={id:'setup-company',mode:'restaurant',registerId:id,registerName:input.registerName,warehouseId:input.warehouseId,bankAccountId:'setup-bank',registerReused:false,accountsCreated:[]};}
  else throw Error('Unexpected closed setup fixture endpoint '+path);++count;receipts.set(key,{body,data:structuredClone(result)});return Response.json({data:result});
 }
 return{request,posts,effects:()=>count,view:()=>view,replace:(next:HospitalitySetup)=>{view=next;}};
}
const setupRoom=(key:string,number:string,version=2,after?:string):HospitalitySetupDraft=>({rowKey:key,label:'Room '+number,plan:{kind:'pms',operation:'room.save',input:{number,typeId:'setup-type',floor:'1',active:true},...(after?{after}:{version})}});
const setupMenu:HospitalitySetupDraft[]=[{rowKey:'menu-product',label:'Coffee',plan:{kind:'product',input:{sku:'COFFEE',name:'Coffee',price:'50.00',unit:'each',taxBps:0}}},{rowKey:'menu-catalog',label:'Coffee menu',plan:{kind:'catalog',productRow:'menu-product',input:{kind:'stock',category:'Restaurant',station:'Kitchen',barcode:'',recipe:[],modifiers:[],taxInclusive:false,expectedVersion:0}}}];
function setupDeferred(){let resolve!:()=>void;const promise=new Promise<void>(done=>{resolve=done;});return{promise,resolve};}
it('closed setup confirms each real PMS engine room version before the next row and replays only the interrupted row',async()=>{
 const c=setupBrowser(),native=setupNative(c);let lose=true;vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);if(init?.method==='POST'&&native.posts.length===2&&lose){lose=false;throw Error('Second native room committed; reply lost');}return response;});c.commands.enqueue([setupRoom('first-room','101'),setupRoom('second-room','102',2,'first-room')]);await expect(c.commands.run()).rejects.toThrow('lost');const first=c.commands.queue()!.jobs;expect(first.map(row=>row.state)).toEqual(['saved','uncertain']);expect(JSON.parse(first[1].sent!.body).expectedVersion).toBe(first[0].receipt!.version);const original=first[1];const result=await c.commands.run();expect(result.acknowledge()).toEqual({newer:true});expect(c.commands.queue()!.jobs.map(row=>row.state)).toEqual(['saved','saved']);expect(native.posts).toHaveLength(3);expect(native.posts[2].init.body).toBe(original.sent!.body);expect(new Headers(native.posts[2].init.headers).get('Idempotency-Key')).toBe(original.key);expect(native.effects()).toBe(2);expect(records(result.view.hotel!,'room').map(room=>room.data.number)).toEqual(['101','102']);expect(c.events).toEqual([]);
});
it('closed setup resolves a menu dependency only from its confirmed original product receipt and retains saved product work after catalogue interruption',async()=>{
 const c=setupBrowser(),native=setupNative(c);let lose=true;vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);if(String(url).includes('/pos/catalog/save')&&lose){lose=false;throw Error('Catalogue committed; reply lost');}return response;});c.commands.enqueue(setupMenu);await expect(c.commands.run()).rejects.toThrow('lost');const original=c.commands.queue()!;expect(original.jobs.map(row=>row.state)).toEqual(['saved','uncertain']);expect(JSON.parse(original.jobs[1].sent!.body).productId).toBe(original.jobs[0].receipt!.id);expect(()=>c.commands.enqueue([{...setupMenu[0],label:'Newer coffee'}])).toThrow('original');const recovered=await c.commands.run();recovered.acknowledge();expect(native.effects()).toBe(2);expect(native.posts.filter(row=>row.url.endsWith('/commerce/products'))).toHaveLength(1);expect(native.posts[2].init.body).toBe(original.jobs[1].sent!.body);expect(new Headers(native.posts[2].init.headers).get('Idempotency-Key')).toBe(original.jobs[1].key);
});
it('closed setup never silently rekeys or rebases a PMS version conflict and cancels only explicitly unsent remainder',async()=>{
 const c=setupBrowser(),native=setupNative(c);vi.stubGlobal('fetch',native.request);c.commands.enqueue([setupRoom('conflict','101',1),setupRoom('not-sent','102',1,'conflict')]);const original=c.commands.queue()!.jobs[0];await expect(c.commands.run()).rejects.toThrow('latest');expect(native.posts).toHaveLength(1);expect(native.effects()).toBe(0);expect(c.commands.queue()!.jobs.map(row=>row.state)).toEqual(['rejected','planned']);expect(c.commands.queue()!.jobs[0].key).toBe(original.key);expect(JSON.parse(c.commands.queue()!.jobs[0].sent!.body).expectedVersion).toBe(1);c.commands.cancelUnsent();expect(c.commands.queue()!.jobs.map(row=>row.state)).toEqual(['rejected','cancelled-before-send']);expect(c.commands.pending()).toBe(false);c.commands.enqueue([setupRoom('conflict','101',2)]);expect(c.commands.queue()!.jobs[2].key).not.toBe(original.key);expect(c.commands.queue()!.jobs[0].key).toBe(original.key);expect((await c.commands.run()).view.hotel?.property.version).toBe(3);
});
it.each([403,409] as const)('closed setup keeps a first generic%d uncertain and forbids cancelling it as unsent',async status=>{
 const c=setupBrowser();vi.stubGlobal('fetch',async()=>Response.json({error:{code:'WORK_ROLE_UNAVAILABLE',message:'Possible post-commit authority error'}},{status}));c.commands.enqueue([setupRoom('room','101')]);const key=c.commands.queue()!.jobs[0].key;await expect(c.commands.run()).rejects.toThrow('Possible');expect(c.commands.queue()!.jobs[0]).toMatchObject({key,state:'uncertain',sends:1});expect(()=>c.commands.cancelUnsent()).toThrow('uncertain');expect(()=>c.commands.archive()).toThrow('unfinished');
});
it('closed setup retains a later PMS conflict after any earlier unknown instead of releasing a possibly committed original',async()=>{
 const c=setupBrowser();vi.stubGlobal('fetch',async()=>{throw Error('First original unknown');});c.commands.enqueue([setupRoom('room','101')]);await expect(c.commands.run()).rejects.toThrow('unknown');const original=c.commands.queue()!.jobs[0];vi.stubGlobal('fetch',async()=>Response.json({error:{code:'PMS_VERSION_CONFLICT',message:'Same code after unknown'}},{status:409}));await expect(c.commands.run()).rejects.toThrow('after unknown');expect(c.commands.queue()!.jobs[0]).toMatchObject({key:original.key,state:'uncertain',sends:2});expect(c.commands.queue()!.jobs[0].sent).toEqual(original.sent);
});
it.each(['POST','GET'] as const)('closed setup rejects delayed ORIGINAL %s JSON after Work changes, retains its exact request and recovers on return',async phase=>{
 const c=setupBrowser(),native=setupNative(c),started=setupDeferred(),release=setupDeferred();let slow=true;class OriginalJSON extends Response{override async json(){started.resolve();await release.promise;return super.json();}}vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);return slow&&(init?.method==='POST')===(phase==='POST')?new OriginalJSON(await response.text()):response;});c.commands.enqueue([setupRoom('room','101')]);const running=c.commands.run(),rejected=expect(running).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});try{await started.promise;const original=c.commands.queue()!.jobs[0];c.work.dataset.workRevision='12';release.resolve();await rejected;expect(c.events).toEqual([]);c.work.dataset.workRevision='11';slow=false;const result=await c.commands.run();result.acknowledge();expect(native.effects()).toBe(1);expect(native.posts[1].init.body).toBe(original.sent!.body);expect(new Headers(native.posts[1].init.headers).get('Idempotency-Key')).toBe(original.key);}finally{release.resolve();}
});
it.each(['Company','branch','property','assignment','URL','unmount'] as const)('closed setup refuses a late %s source change after ORIGINAL JSON without marking the native row saved',async changed=>{
 const c=setupBrowser(),native=setupNative(c),started=setupDeferred(),release=setupDeferred();class OriginalJSON extends Response{override async json(){started.resolve();await release.promise;return super.json();}}vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);return new OriginalJSON(await response.text());});c.commands.enqueue([setupRoom('room','101')]);const running=c.commands.run(),rejected=expect(running).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});try{await started.promise;const stored=c.values.get(c.key)!;if(changed==='Company')c.bind({...c.source,owner:{...c.source.owner,companyId:'foreign'}});else if(changed==='branch')c.bind({...c.source,data:{...c.source.data,branchId:'foreign'}});else if(changed==='property')c.bind({...c.source,data:{...c.source.data,propertyId:'foreign'}});else if(changed==='assignment')c.work.dataset.workAssignment='foreign';else if(changed==='URL')c.location.search='?property=foreign';else c.bind(null);release.resolve();await rejected;expect(c.values.get(c.key)).toBe(stored);expect(c.events).toEqual([]);}finally{release.resolve();}
});
it('closed setup quarantines old unbound queues and malformed bound bytes without assigning them to the current owner or erasing them',()=>{
 const c=setupBrowser(),legacy=legacyHospitalitySetupQueueKey(c.source),raw=JSON.stringify([{key:'old',url:'/foreign',state:'saved'}]);c.values.set(legacy,raw);expect(c.commands.legacy()).toEqual({key:legacy,bytes:raw.length});expect(()=>c.commands.enqueue([setupRoom('room','101')])).toThrow('verified');expect(c.values.get(legacy)).toBe(raw);c.values.delete(legacy);c.values.set(c.key,raw);expect(()=>c.commands.queue()).toThrow('verified');expect(c.values.get(c.key)).toBe(raw);
});
it('closed setup detects another writer changing its durable queue and preserves the newer stored bytes',async()=>{
 const c=setupBrowser(),native=setupNative(c);vi.stubGlobal('fetch',native.request);c.commands.enqueue([setupRoom('room','101')]);const changed=JSON.parse(c.values.get(c.key)!);changed.jobs[0].label='Another writer changed this queue';const bytes=JSON.stringify(changed);c.values.set(c.key,bytes);await expect(c.commands.run()).rejects.toThrow('verified');expect(c.values.get(c.key)).toBe(bytes);expect(native.posts).toEqual([]);
});
it('closed setup demands an exact property-create hash receipt and follows only that actual new hotel',async()=>{
 const initial=setupData();initial.hotel=null;initial.propertyId=null;initial.properties=[];const c=setupBrowser(initial),native=setupNative(c);vi.stubGlobal('fetch',native.request);c.commands.enqueue([{rowKey:'hotel',label:'Hotel',plan:{kind:'pms',operation:'property.create',input:{name:'Actual new hotel',branchId:'setup-branch',businessDate:'2026-10-18'}}}]);const key=c.commands.queue()!.jobs[0].key,result=await c.commands.run(),id='property-'+setupHash('sha256').update(key).digest('hex').slice(0,32);expect(result.view.propertyId).toBe(id);expect(c.commands.queue()!.jobs[0].receipt).toEqual({id,propertyId:id,version:1});result.acknowledge();c.bind({...c.source,data:result.view});expect(c.commands.pending()).toBe(false);expect((await c.commands.refresh()).view.propertyId).toBe(id);
});
it('closed setup does not accept a plausible wrong PMS receipt or an empty queue/read as acknowledgement',async()=>{
 const c=setupBrowser(),native=setupNative(c);vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);if(init?.method!=='POST')return response;const decoded=await response.json();return Response.json({data:{...decoded.data,propertyId:'foreign'}});});c.commands.enqueue([setupRoom('room','101')]);await expect(c.commands.run()).rejects.toThrow('receipt');expect(c.commands.queue()!.jobs[0].state).toBe('uncertain');expect(c.events).toEqual([]);vi.stubGlobal('fetch',native.request);const result=await c.commands.run();result.acknowledge();await expect(c.commands.run()).rejects.toThrow('No unfinished');expect(native.effects()).toBe(1);
});
it('closed setup acknowledges a native archived product receipt honestly without inventing an active choice or rewriting its original price',async()=>{
 const c=setupBrowser(),native=setupNative(c);let lose=true;vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);if(init?.method==='POST'&&lose){lose=false;throw Error('Original product committed; reply lost');}return response;});c.commands.enqueue([setupMenu[0]]);await expect(c.commands.run()).rejects.toThrow('lost');native.replace({...native.view(),products:[]});const result=await c.commands.run();expect(result.view.products).toEqual([]);expect(c.commands.queue()!.jobs[0].confirmation).toBe('native-receipt-history');expect(c.commands.queue()!.jobs[0].receipt?.id).toBe('products-'+c.commands.queue()!.jobs[0].key);result.acknowledge();expect(native.effects()).toBe(1);expect(c.commands.pending()).toBe(false);
});
it('closed setup keeps one-shot completion separate from newer form fields without publishing a saved event',async()=>{
 const c=setupBrowser(),native=setupNative(c),NativeFormData=FormData,draft={reason:'Original setup input'},form={isConnected:true,getAttribute:()=> 'reviewed-edit'} as unknown as HTMLFormElement;class Fields extends NativeFormData{constructor(node?:HTMLFormElement){super();if(node===form)this.append('reason',draft.reason);}}vi.stubGlobal('FormData',Fields);vi.stubGlobal('fetch',native.request);c.commands.enqueue([setupRoom('room','101')],{form,formEditVersion:'reviewed-edit'});const result=await c.commands.run();draft.reason='Newer unsubmitted setup fields';expect(result.acknowledge()).toEqual({newer:true});expect(c.events).toEqual([]);expect(()=>result.acknowledge()).toThrow('already');expect(draft.reason).toBe('Newer unsubmitted setup fields');
});

type SetupHookCell={value?:unknown;current?:unknown;deps?:readonly unknown[];cleanup?:()=>void};
function setupHooks(){const cells:SetupHookCell[]=[],effects:(()=>void)[]=[],layouts:(()=>void)[]=[];let cursor=0;const state=(initial:unknown)=>{const index=cursor++;if(!cells[index])cells[index]={value:typeof initial==='function'?initial():initial};return[cells[index].value,(next:unknown)=>{cells[index].value=typeof next==='function'?next(cells[index].value):next;}];},effect=(queue:(()=>void)[],run:()=>void|(()=>void),deps?:readonly unknown[])=>{const index=cursor++,old=cells[index];if(!old||!deps||!old.deps||deps.length!==old.deps.length||deps.some((value,i)=>!Object.is(value,old.deps?.[i])))queue.push(()=>{old?.cleanup?.();const cleanup=run();cells[index]={deps,cleanup:typeof cleanup==='function'?cleanup:undefined};});};return{react:{...React,useState:state,useRef:(initial:unknown)=>{const index=cursor++;return cells[index]??(cells[index]={current:initial});},useEffect:(run:()=>void|(()=>void),deps?:readonly unknown[])=>effect(effects,run,deps),useLayoutEffect:(run:()=>void|(()=>void),deps?:readonly unknown[])=>effect(layouts,run,deps)},render<T>(run:()=>T){cursor=0;const result=run();while(layouts.length)layouts.shift()!();while(effects.length)effects.shift()!();return result;},close(){for(const cell of cells)cell.cleanup?.();}};}
function setupNodes(tree:React.ReactNode,result:React.ReactElement<Record<string,unknown>>[]=[]){React.Children.forEach(tree,node=>{if(!React.isValidElement<Record<string,unknown>>(node))return;result.push(node);setupNodes(node.props.children as React.ReactNode,result);});return result;}
async function mountSetup(c:ReturnType<typeof setupBrowser>,network:(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>){const require=setupRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),root=process.env.ERPX_HOSPITALITY_SETUP_SOURCE_ROOT??'.',source=setupRead(setupResolve(root,'packages/feature-shared/src/components/hospitality/standard-setup-guide.tsx'),'utf8'),compiled=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText,loaded={exports:{} as Record<string,unknown>},hooks=setupHooks(),router={refresh:vi.fn(),push:vi.fn()};vi.stubGlobal('fetch',network);const command=await import('@erpx/feature-shared/lib/hospitality-setup-command');
 const ports=(path:string):unknown=>{if(path==='react')return hooks.react;if(path==='react/jsx-runtime')return require(path);if(path.endsWith("@erpx/front/components/front-language"))return{FrontLanguageContext:setupLanguageContext,FrontLocalizedTree};if(path.endsWith("@erpx/feature-shared/lib/front-language"))return{frontText:setupFrontText};if(path.endsWith("@erpx/feature-shared/lib/destination-router"))return{useRouter:()=>router};if(path.endsWith("@erpx/feature-work_projection/lib/destination-registry"))return{canonicalUiHref:(url:string)=>url};if(path.endsWith("@erpx/workplace/components/workspace-link"))return{default:'OwnerWorkspaceLink'};if(path.endsWith("@erpx/ui/components/foundation"))return{Button:'Button',Card:'Card',Field:'Field',InlineNotice:'Notice',Select:'Select',Table:'Table',TableFrame:'TableFrame',TextInput:'TextInput'};if(path.endsWith("@erpx/ui/components/draft-form-drawer"))return{DraftFormDrawer:'RightDrawer'};if(path.endsWith("@erpx/feature-work_projection/lib/context-switch"))return{contextIsSwitching:()=>false,registerUnsavedWork:setupRegisterUnsavedWork};if(path.endsWith("@erpx/feature-work_projection/lib/work-refresh"))return{announceWorkChange:()=>c.surface.dispatchEvent(new Event('erpx:work-changed'))};if(path.endsWith("@erpx/feature-finance/lib/finance-money"))return{decimalToMinor:setupDecimalToMinor};if(path.endsWith("@erpx/feature-shared/lib/hospitality-grid"))return{expandRoomNumbers,roomSaveKey};if(path.endsWith("@erpx/feature-shared/lib/hospitality-setup-command"))return command;if(path.endsWith("@erpx/server-pms/public/rules/pms-model"))return{records};if(path.endsWith('.module.css'))return{default:new Proxy({}, {get:(_target,key)=>String(key)})};throw Error('Unexpected actual standard setup presenter import '+path);};new Function('require','module','exports',compiled)(ports,loaded,loaded.exports);let props={initial:c.source.data,base:c.source.base,owner:c.source.owner,canBootstrapPos:c.source.canBootstrapPos,canReadPos:c.source.canReadPos};const Component=loaded.exports.StandardHospitalitySetupGuide as (p:typeof props)=>React.ReactElement<typeof props>;
 return{router,render(){const element=Component(props),Owner=element.type;if(typeof Owner!=='function')throw Error('Actual setup owner is missing.');return hooks.render(()=>Reflect.apply(Owner,undefined,[element.props]) as React.ReactNode);},update(initial:HospitalitySetup){props={...props,initial};},close(){hooks.close();}};
}
it('actual standard setup preserves native prerequisite and rehearsal links and opens the whole room grid in one standard right drawer without a mount HTTP read',async()=>{
 const c=setupBrowser(),network=vi.fn(),mounted=await mountSetup(c,network);try{let tree=setupNodes(mounted.render());expect(tree.filter(node=>node.type==='OwnerWorkspaceLink').map(node=>node.props.href)).toEqual(expect.arrayContaining(['/holdings/setup/finance/setup','/holdings/setup/commerce','/holdings/setup/pos?tab=settings','/holdings/setup/pms/night-audit?property=setup-hotel']));Reflect.apply(tree.find(node=>node.type==='Button'&&node.props.children==='Add rooms')!.props.onClick as ()=>void,undefined,[]);tree=setupNodes(mounted.render());expect(tree.find(node=>node.type==='RightDrawer')?.props).toMatchObject({appearance:'standard',title:'Room entry grid',open:true});expect(tree.filter(node=>node.type==='form')).toHaveLength(1);expect(tree.filter(node=>typeof node.props.name==='string').map(node=>node.props.name)).toEqual(expect.arrayContaining(['room-first-numbers','room-first-type','room-first-floor']));expect(tree.some(node=>node.type==='option'&&node.props.value==='setup-type')).toBe(true);expect(network).not.toHaveBeenCalled();}finally{mounted.close();}
});
it('actual standard setup retains a closed original room-type form through unknown and authority loss, retries its original key/body on return, and preserves newer fields',async()=>{
 const c=setupBrowser(),native=setupNative(c),NativeFormData=FormData,draft={name:'Reviewed room type',capacity:'2'},edits={version:'reviewed-edit'},form={isConnected:true,getAttribute:()=>edits.version,setAttribute:(_name:string,text:string)=>{edits.version=text;},reset:vi.fn()} as unknown as HTMLFormElement;class Fields extends NativeFormData{constructor(node?:HTMLFormElement){super();if(node===form)for(const [key,value]of Object.entries(draft))this.append(key,value);}}vi.stubGlobal('FormData',Fields);let lose=true;const mounted=await mountSetup(c,async(url,init)=>{const response=await native.request(url,init);if(init?.method==='POST'&&lose){lose=false;throw Error('Native room type committed; response lost');}return response;}),nodes=()=>setupNodes(mounted.render()),button=(label:string)=>nodes().find(node=>node.type==='Button'&&node.props.children===label)!;
 try{Reflect.apply(button('Add room type').props.onClick as ()=>void,undefined,[]);let editor=nodes().find(node=>node.type==='form')!;Reflect.apply(editor.props.onInputCapture as (e:unknown)=>void,undefined,[{currentTarget:form}]);mounted.render();Reflect.apply(nodes().find(node=>node.type==='form')!.props.onSubmit as (e:unknown)=>void,undefined,[{currentTarget:form,preventDefault:vi.fn()}]);await vi.waitFor(()=>expect(button('Retry original unfinished rows').props.disabled).toBe(false));const original=JSON.parse(c.values.get(c.key)!).jobs[0];draft.name='Newer unsent room-type name';editor=nodes().find(node=>node.type==='form')!;Reflect.apply(editor.props.onInputCapture as (e:unknown)=>void,undefined,[{currentTarget:form}]);Reflect.apply(button('Close and retain draft').props.onClick as ()=>void,undefined,[]);mounted.update({...c.source.data,permissions:{...c.source.data.permissions,hotel:false}});let drawer=nodes().find(node=>node.type==='RightDrawer'&&node.props.title==='Room type')!;expect(drawer.props.open).toBe(false);expect(drawer.props.retainFormContents).toBe(true);expect(button('Retry original unfinished rows')).toBeDefined();mounted.update(c.source.data);mounted.render();await vi.waitFor(()=>expect(button('Retry original unfinished rows').props.disabled).toBe(false));Reflect.apply(button('Retry original unfinished rows').props.onClick as ()=>void,undefined,[]);await vi.waitFor(()=>{expect(nodes().some(node=>node.type==='Notice'&&String(node.props.children).includes('newer or restored draft'))).toBe(true);expect(mounted.router.refresh).toHaveBeenCalledOnce();});drawer=nodes().find(node=>node.type==='RightDrawer'&&node.props.title==='Room type')!;expect(drawer.props.open).toBe(false);expect(drawer.props.retainFormContents).toBe(true);expect(draft.name).toBe('Newer unsent room-type name');expect(form.reset).not.toHaveBeenCalled();expect(native.posts[1].init.body).toBe(original.sent.body);expect(new Headers(native.posts[1].init.headers).get('Idempotency-Key')).toBe(original.key);expect(native.effects()).toBe(1);expect(c.events.filter(event=>event.type==='erpx:record-command-saved')).toEqual([]);
 }finally{mounted.close();}
});
it.each(['hospitality/setup','finance/hospitality-settings'] as const)('actual %s Server caller preserves its reader/authority and binds only native public owner identity to the standard setup',async path=>{
 const require=setupRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),root=process.env.ERPX_HOSPITALITY_SETUP_SOURCE_ROOT??'.',source=setupRead(setupResolve(root,`packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/${path}/${path==='hospitality/setup'?'standard-screen':'screen'}.tsx`),'utf8'),compiled=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText,loaded={exports:{} as Record<string,unknown>},data=setupData(),actor={userId:'setup-user',companyId:'setup-company',branchId:null,assignmentId:'original-assignment',privateAuthority:'not-client'},read=vi.fn(async()=>data),permission=vi.fn((value:unknown)=>value),has=vi.fn((_actor:unknown,key:string)=>['pos.configure','pos.accounting.prepare','finance.write'].includes(key));
 const ports=(name:string):unknown=>{if(name==='react/jsx-runtime')return require(name);if(name.endsWith('/permissions'))return{hasPermission:has};if(name.endsWith('/authorization'))return{requireWorkContext:async()=>actor,publicScope:()=>({kind:'company'})};if(name.endsWith('/iam-permissions'))return{requirePermission:permission};if(name.endsWith('/workspace'))return{workspaceBase:()=>'/holdings/setup'};if(name.endsWith("@erpx/feature-work_projection/lib/destination-registry"))return{canonicalUiHref:(value:string)=>value};if(name.endsWith("@erpx/server-pms/public/hospitality-setup"))return{hospitalitySetup:read};if(name.endsWith("@erpx/feature-shared/components/hospitality/standard-setup-guide"))return{StandardHospitalitySetupGuide:'StandardSetupOwner'};if(name.endsWith('/finance-workspace'))return{FinanceWorkspace:'OriginalFinanceOwner'};if(name.endsWith('/page-error'))return{pageError:(cause:unknown)=>{throw cause;}};throw Error('Unexpected actual setup Server caller import '+name);};new Function('require','module','exports',compiled)(ports,loaded,loaded.exports);const Page=loaded.exports.default as (props:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch:string;property:string}>})=>Promise<React.ReactNode>,tree=await Page({params:Promise.resolve({holdingSlug:'setup'}),searchParams:Promise.resolve({branch:'setup-branch',property:'setup-hotel'})}),owner=setupNodes(tree).find(node=>node.type==='StandardSetupOwner')!;expect(read).toHaveBeenCalledExactlyOnceWith(actor,'setup-branch','setup-hotel');expect(owner.props.owner).toEqual({userId:actor.userId,companyId:actor.companyId,branchId:null});expect(owner.props.canBootstrapPos).toBe(true);expect(JSON.stringify(owner.props.owner)).not.toContain('not-client');expect(Object.keys(owner.props.owner as object).sort()).toEqual(['branchId','companyId','userId']);if(path.startsWith('finance'))expect(permission).toHaveBeenCalledExactlyOnceWith(actor,'finance.read');else expect(permission).not.toHaveBeenCalled();
});

it.each(['bootstrap','register','table'] as const)('closed setup validates the actual %s native receipt and same-owner created row without inventing a version',async kind=>{
 const c=setupBrowser(),native=setupNative(c);vi.stubGlobal('fetch',native.request);const draft:HospitalitySetupDraft=kind==='bootstrap'?{rowKey:kind,label:kind,plan:{kind,input:{mode:'restaurant',registerName:'Actual register',warehouseId:'setup-warehouse'}}}:kind==='register'?{rowKey:kind,label:kind,plan:{kind,input:{mode:'restaurant',name:'Actual register',warehouseId:'setup-warehouse'}}}:{rowKey:kind,label:kind,plan:{kind,input:{registerId:'setup-register',name:'Actual table',seats:4}}};c.commands.enqueue([draft]);const result=await c.commands.run();expect(result.jobs).toHaveLength(1);expect(result.jobs[0].receipt).not.toHaveProperty('version');expect(result.jobs[0].confirmation).toBe('native-current-record');result.acknowledge();expect(c.commands.pending()).toBe(false);expect(native.effects()).toBe(1);
});
it('closed setup refuses a plausible foreign random register receipt and keeps the actual original key pending',async()=>{
 const c=setupBrowser(),native=setupNative(c);vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{if(String(url).endsWith('/pos?tab=settings'))return Response.json({data:{companyId:'setup-company',branchId:null,registers:native.view().registers.map(row=>({...row,company_id:'setup-company',branch_id:'setup-branch',version:1}))}});const response=await native.request(url,init);return init?.method==='POST'?Response.json({data:{id:'foreign-random-register'}}):response;});c.commands.enqueue([{rowKey:'register',label:'Register',plan:{kind:'register',input:{mode:'restaurant',name:'Actual register',warehouseId:'setup-warehouse'}}}]);const key=c.commands.queue()!.jobs[0].key;await expect(c.commands.run()).rejects.toThrow('actual same-Company');expect(c.commands.queue()!.jobs[0]).toMatchObject({key,state:'uncertain'});expect(native.effects()).toBe(1);expect(c.events).toEqual([]);
});
it.each([false,true] as const)('closed setup holds an inactive random-ID register unless existing current pos.read confirms its real row (read=%s)',async canRead=>{
 const c=setupBrowser();c.source.canReadPos=canRead;const native=setupNative(c);let lose=true,settingsReads=0,createdId='';vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{if(String(url).endsWith('/pos?tab=settings')){++settingsReads;return Response.json({data:{companyId:'setup-company',branchId:null,registers:[{id:createdId,company_id:'setup-company',branch_id:'setup-branch',warehouse_id:'setup-warehouse',name:'Later lawful register name',mode:'restaurant',active:false,version:2}]}});}const response=await native.request(url,init);if(init?.method==='POST'&&lose){lose=false;createdId=String((await response.json()).data.id);native.replace({...native.view(),registers:[]});throw Error('Register committed; original reply lost before later normal inactivation');}return response;});c.commands.enqueue([{rowKey:'register',label:'Register',plan:{kind:'register',input:{mode:'restaurant',name:'Original register',warehouseId:'setup-warehouse'}}}]);await expect(c.commands.run()).rejects.toThrow('lost');const original=c.commands.queue()!.jobs[0];if(canRead){const result=await c.commands.run();expect(result.jobs[0].confirmation).toBe('native-receipt-history');expect(result.view.registers).toEqual([]);result.acknowledge();expect(c.commands.pending()).toBe(false);expect(settingsReads).toBe(1);}else{await expect(c.commands.run()).rejects.toThrow('requires current pos.read');expect(c.commands.queue()!.jobs[0].state).toBe('uncertain');expect(settingsReads).toBe(0);}expect(native.posts[1].init.body).toBe(original.sent!.body);expect(new Headers(native.posts[1].init.headers).get('Idempotency-Key')).toBe(original.key);expect(native.effects()).toBe(1);
});
it('closed setup serializes in-flight queue recovery and refuses concurrent newer work while ORIGINAL JSON remains pending',async()=>{
 const c=setupBrowser(),native=setupNative(c),started=setupDeferred(),release=setupDeferred();class OriginalJSON extends Response{override async json(){started.resolve();await release.promise;return super.json();}}vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);return init?.method==='POST'?new OriginalJSON(await response.text()):response;});c.commands.enqueue([setupRoom('room','101')]);const running=c.commands.run();try{await started.promise;const original=c.commands.queue()!.jobs[0];await expect(c.commands.run()).rejects.toThrow('Wait');expect(()=>c.commands.enqueue([setupRoom('newer','102')])).toThrow('Wait');expect(native.posts).toHaveLength(1);release.resolve();const result=await running;result.acknowledge();expect(c.commands.queue()!.jobs[0].key).toBe(original.key);expect(native.posts).toHaveLength(1);expect(native.effects()).toBe(1);}finally{release.resolve();}
});
it('closed setup captures original form completion before asynchronous row planning so newer edits are never reported saved',async()=>{
 const c=setupBrowser(),native=setupNative(c),NativeFormData=FormData,draft={name:'Original reviewed fields'},edit={version:'first'},form={isConnected:true,getAttribute:()=>edit.version} as unknown as HTMLFormElement;class Fields extends NativeFormData{constructor(node?:HTMLFormElement){super();if(node===form)this.append('name',draft.name);}}vi.stubGlobal('FormData',Fields);vi.stubGlobal('fetch',native.request);const completion=captureHospitalitySetupCompletion(form);draft.name='Newer fields while room range hashing was pending';edit.version='newer';c.commands.enqueue([setupRoom('room','101')],completion,c.source.data);const result=await c.commands.run();expect(result.acknowledge()).toEqual({newer:true});expect(c.events).toEqual([]);expect(draft.name).toBe('Newer fields while room range hashing was pending');
});
it('closed setup retains the original native nightly-rate minor amount and property version through exact replay',async()=>{
 const c=setupBrowser(),native=setupNative(c);let lose=true;vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{const response=await native.request(url,init);if(init?.method==='POST'&&lose){lose=false;throw Error('Native nightly rate committed; reply lost');}return response;});c.commands.enqueue([{rowKey:'nightly-rate',label:'Standard nightly',plan:{kind:'pms',operation:'rate.save',version:2,input:{typeId:'setup-type',name:'Standard nightly',from:'2026-10-18',to:'2026-11-01',price:150000}}}]);await expect(c.commands.run()).rejects.toThrow('lost');const original=c.commands.queue()!.jobs[0],result=await c.commands.run();expect(records(result.view.hotel!,'rate')[0].data).toMatchObject({typeId:'setup-type',name:'Standard nightly',from:'2026-10-18',to:'2026-11-01',price:150000});expect(result.jobs[0].receipt).toMatchObject({propertyId:'setup-hotel',version:3});expect(native.posts[1].init.body).toBe(original.sent!.body);expect(new Headers(native.posts[1].init.headers).get('Idempotency-Key')).toBe(original.key);expect(native.effects()).toBe(1);result.acknowledge();expect(c.commands.pending()).toBe(false);
});

// Standard setup continuation: actual original locale provider, retained editor,
// and partial native batches. These are controlled owner-handler tests, not a
// browser/DB acceptance claim.
import {FrontLanguageContext as setupLanguageContext} from '@erpx/front/components/front-language';
import {frontText as setupFrontText} from '@erpx/feature-shared/lib/front-language';
const setupButton=(nodes:ReturnType<typeof setupNodes>,text:string)=>{const node=nodes.find(node=>node.type==='Button'&&node.props.children===text);if(!node)throw Error('Actual setup button missing: '+text);return node;};
function setupFormFields(initial:Record<string,string>){const draft={...initial},edit={value:'reviewed-edit'},form={isConnected:true,getAttribute:()=>edit.value,setAttribute:(_name:string,next:string)=>{edit.value=next;},reset:vi.fn(()=>{for(const name of Object.keys(draft))delete draft[name];})} as unknown as HTMLFormElement,Native=FormData;class Fields extends Native{constructor(node?:HTMLFormElement){super();if(node===form)for(const [key,value]of Object.entries(draft))this.append(key,value);}}vi.stubGlobal('FormData',Fields);return{draft,edit,form,bind(nodes:ReturnType<typeof setupNodes>){const node=nodes.find(node=>node.type==='form');if(!node)throw Error('Actual setup form missing');const ref=node.props.ref;if(ref&&typeof ref==='object'&&'current'in ref)ref.current=form;return node;},mark(node:ReturnType<typeof setupNodes>[number]){Reflect.apply(node.props.onInputCapture as (event:unknown)=>void,undefined,[{currentTarget:form}]);},submit(node:ReturnType<typeof setupNodes>[number]){Reflect.apply(node.props.onSubmit as (event:unknown)=>void,undefined,[{currentTarget:form,preventDefault:vi.fn()}]);}};}
it('actual setup keeps the original English/Thai switch and translates owner guidance, empty state, right drawers and retention controls without an HTTP read',async()=>{
 const c=setupBrowser(),network=vi.fn(),mounted=await mountSetup(c,network),fields=setupFormFields({name:'ชื่อห้องที่ยังไม่ส่ง',capacity:'2'});const nodes=()=>setupNodes(mounted.render());
 try{
  expect(renderToStaticMarkup(mounted.render())).toContain('Hotel and restaurant setup');Reflect.apply(setupButton(nodes(),'Add room type').props.onClick as ()=>void,undefined,[]);const form=fields.bind(nodes());fields.mark(form);mounted.render();Reflect.apply(setupButton(nodes(),'ไทย').props.onClick as ()=>void,undefined,[]);
  let html=renderToStaticMarkup(mounted.render());for(const text of ['ตั้งค่าโรงแรมและร้านอาหาร','เตรียมบัญชีและสต็อก','ตั้งค่าบัญชีการค้า','ประเภทห้อง','ปิดและเก็บร่างไว้','แก้ไขต่อ'])expect(html).toContain(text);expect(nodes().find(node=>node.type==='RightDrawer'&&node.props.open)?.props.title).toBe('ประเภทห้อง');expect(fields.draft.name).toBe('ชื่อห้องที่ยังไม่ส่ง');expect(form.key).toBe(nodes().find(node=>node.type==='form')?.key);expect(fields.form.reset).not.toHaveBeenCalled();
  Reflect.apply(setupButton(nodes(),'Close and retain draft').props.onClick as ()=>void,undefined,[]);html=renderToStaticMarkup(mounted.render());expect(html).toContain('เปิดร่างตั้งค่าที่เก็บไว้ต่อ');Reflect.apply(setupButton(nodes(),'English').props.onClick as ()=>void,undefined,[]);expect(renderToStaticMarkup(mounted.render())).toContain('Resume retained setup draft');expect(network).not.toHaveBeenCalled();expect(c.values.size).toBe(0);
 }finally{mounted.close();}
});
it('actual setup language changes preserve an uncertain original row and display its native recovery in Thai while retaining the same request key/body',async()=>{
 const c=setupBrowser(),native=setupNative(c),fields=setupFormFields({name:'Reviewed room type',capacity:'2'});let lose=true;const mounted=await mountSetup(c,async(url,init)=>{const response=await native.request(url,init);if(init?.method==='POST'&&lose){lose=false;throw Error('Native setup reply lost');}return response;}),nodes=()=>setupNodes(mounted.render());
 try{
  Reflect.apply(setupButton(nodes(),'Add room type').props.onClick as ()=>void,undefined,[]);let form=fields.bind(nodes());fields.mark(form);form=fields.bind(nodes());fields.submit(form);await vi.waitFor(()=>expect(setupButton(nodes(),'Retry original unfinished rows').props.disabled).toBe(false));const original=JSON.parse(c.values.get(c.key)!).jobs[0];Reflect.apply(setupButton(nodes(),'ไทย').props.onClick as ()=>void,undefined,[]);expect(renderToStaticMarkup(mounted.render())).toContain('ลองยืนยันรายการเดิมที่ยังค้าง');expect(renderToStaticMarkup(mounted.render())).toContain('ยังไม่แน่ชัด');Reflect.apply(setupButton(nodes(),'Review original row inputs').props.onClick as ()=>void,undefined,[]);expect(nodes().find(node=>node.type==='RightDrawer'&&node.props.open)?.props.title).toBe('รายการตั้งค่าเดิม');
  Reflect.apply(setupButton(nodes(),'Retry original unfinished rows').props.onClick as ()=>void,undefined,[]);await vi.waitFor(()=>expect(native.posts).toHaveLength(2));await vi.waitFor(()=>expect(mounted.router.refresh).toHaveBeenCalledOnce());expect(native.posts[1].init.body).toBe(original.sent.body);expect(new Headers(native.posts[1].init.headers).get('Idempotency-Key')).toBe(original.key);expect(native.effects()).toBe(1);expect(renderToStaticMarkup(mounted.render())).toContain('ยืนยันทุกรายการที่ส่ง');
 }finally{mounted.close();}
});
it('actual setup close/resume preserves the original reviewed source and dirty uncontrolled form; a different-editor discard actually resets it',async()=>{
 const c=setupBrowser(),network=vi.fn(),mounted=await mountSetup(c,network),fields=setupFormFields({name:'Original retained type',capacity:'2'}),nodes=()=>setupNodes(mounted.render());
 try{
  Reflect.apply(setupButton(nodes(),'Add room type').props.onClick as ()=>void,undefined,[]);const first=fields.bind(nodes());fields.mark(first);mounted.render();Reflect.apply(setupButton(nodes(),'Close and retain draft').props.onClick as ()=>void,undefined,[]);expect(nodes().find(node=>node.type==='RightDrawer')?.props).toMatchObject({open:false,dirty:true,retainFormContents:true});
  Reflect.apply(setupButton(nodes(),'Resume retained setup draft').props.onClick as ()=>void,undefined,[]);expect(nodes().find(node=>node.type==='RightDrawer')?.props).toMatchObject({open:true,dirty:true});expect(nodes().find(node=>node.type==='form')?.key).toBe(first.key);expect(fields.draft.name).toBe('Original retained type');expect(fields.form.reset).not.toHaveBeenCalled();expect(c.surface.confirm).not.toHaveBeenCalled();
  Reflect.apply(setupButton(nodes(),'Close and retain draft').props.onClick as ()=>void,undefined,[]);Reflect.apply(setupButton(nodes(),'Add room type').props.onClick as ()=>void,undefined,[]);expect(nodes().find(node=>node.type==='RightDrawer')?.props).toMatchObject({open:true,dirty:true});expect(fields.form.reset).not.toHaveBeenCalled();expect(c.surface.confirm).not.toHaveBeenCalled();
  c.surface.confirm.mockReturnValueOnce(false);Reflect.apply(setupButton(nodes(),'Add rooms').props.onClick as ()=>void,undefined,[]);expect(nodes().find(node=>node.type==='form')?.key).toBe(first.key);expect(fields.draft.name).toBe('Original retained type');expect(fields.form.reset).not.toHaveBeenCalled();
  c.surface.confirm.mockReturnValueOnce(true);Reflect.apply(setupButton(nodes(),'Add rooms').props.onClick as ()=>void,undefined,[]);expect(fields.form.reset).toHaveBeenCalledOnce();expect(fields.draft).toEqual({});expect(nodes().find(node=>node.type==='RightDrawer')?.props).toMatchObject({title:'Room entry grid',dirty:false});expect(nodes().find(node=>node.type==='form')?.key).not.toBe(first.key);expect(network).not.toHaveBeenCalled();
 }finally{mounted.close();}
});
it('actual table partial batch refuses a changed saved row before any new send, retains all fields, then accepts only unchanged saved row plus corrected rejected row',async()=>{
 const c=setupBrowser(),native=setupNative(c),fields=setupFormFields({register:'setup-register'}),calls:{url:string;body:string;key:string}[]=[];let reject=true;const mounted=await mountSetup(c,async(url,init)=>{if(init?.method==='POST'){const body=String(init.body),input=JSON.parse(body);calls.push({url:String(url),body,key:new Headers(init.headers).get('Idempotency-Key')!});if(input.name==='Rejected table'&&reject){reject=false;return Response.json({error:{code:'POS_TABLE_NAME_EXISTS',message:'Native duplicate table'}},{status:422});}}return native.request(url,init);}),nodes=()=>setupNodes(mounted.render());
 try{
  Reflect.apply(setupButton(nodes(),'Prepare tables').props.onClick as ()=>void,undefined,[]);Reflect.apply(setupButton(nodes(),'Add table row').props.onClick as ()=>void,undefined,[]);const names=nodes().filter(node=>node.type==='TextInput'&&String(node.props.name).endsWith('-name')).map(node=>String(node.props.name));expect(names).toHaveLength(2);Object.assign(fields.draft,{[names[0]]:'Saved table',[names[0].replace('-name','-seats')]:'4',[names[1]]:'Rejected table',[names[1].replace('-name','-seats')]:'4'});let form=fields.bind(nodes());fields.mark(form);form=fields.bind(nodes());fields.submit(form);await vi.waitFor(()=>{expect(calls).toHaveLength(2);expect(nodes().find(node=>node.type==='RightDrawer'&&node.props.title==='Restaurant tables')?.props.dismissDisabled).toBe(false);});const before=JSON.parse(c.values.get(c.key)!);expect(before.jobs.map((job:{state:string})=>job.state)).toEqual(['saved','rejected']);expect(native.effects()).toBe(1);
  fields.draft[names[0]]='Newer unsent saved table';fields.draft[names[1]]='Corrected table';form=fields.bind(nodes());fields.mark(form);form=fields.bind(nodes());fields.submit(form);await vi.waitFor(()=>expect(nodes().some(node=>node.type==='Notice'&&String(node.props.children).includes('previously saved setup row'))).toBe(true));expect(calls).toHaveLength(2);expect(native.effects()).toBe(1);expect(fields.draft[names[0]]).toBe('Newer unsent saved table');expect(nodes().find(node=>node.type==='RightDrawer'&&node.props.title==='Restaurant tables')?.props).toMatchObject({open:true,dirty:true});expect(mounted.router.refresh).not.toHaveBeenCalled();expect(fields.form.reset).not.toHaveBeenCalled();expect(c.events.filter(event=>event.type==='erpx:record-command-saved')).toEqual([]);expect(JSON.parse(c.values.get(c.key)!).jobs[0]).toEqual(before.jobs[0]);
  fields.draft[names[0]]='Saved table';form=fields.bind(nodes());fields.mark(form);form=fields.bind(nodes());fields.submit(form);await vi.waitFor(()=>expect(mounted.router.refresh).toHaveBeenCalledOnce());expect(calls).toHaveLength(3);expect(native.effects()).toBe(2);expect(native.view().tables.map(table=>table.name)).toEqual(['Saved table','Corrected table']);const after=JSON.parse(c.values.get(c.key)!);expect(after.jobs[0]).toEqual(before.jobs[0]);expect(calls.filter(call=>JSON.parse(call.body).name==='Saved table')).toHaveLength(1);expect(calls[2].key).not.toBe(calls[1].key);expect(c.events.filter(event=>event.type==='erpx:record-command-saved')).toHaveLength(1);expect(nodes().some(node=>node.type==='form')).toBe(false);
 }finally{mounted.close();}
});
it('closed setup preserves a saved product when its catalogue is rejected, refuses changed product corrections atomically, and permits only the unchanged dependency with corrected catalogue',async()=>{
 const c=setupBrowser(),native=setupNative(c);let reject=true;vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{if(String(url).endsWith('/pos/catalog/save')&&reject){reject=false;return Response.json({error:{code:'POS_INGREDIENT_INVALID',message:'Native ingredient rejection before effects'}},{status:422});}return native.request(url,init);});c.commands.enqueue(setupMenu);await expect(c.commands.run()).rejects.toThrow('ingredient rejection');const before=c.commands.queue()!;expect(before.jobs.map(job=>job.state)).toEqual(['saved','rejected']);const product=setupMenu[0];if(product.plan.kind!=='product')throw Error('Actual product plan missing');const productPlan=product.plan;const catalogue=setupMenu[1];if(catalogue.plan.kind!=='catalog')throw Error('Actual catalogue plan missing');const corrected={...catalogue,plan:{...catalogue.plan,input:{...catalogue.plan.input,station:'Corrected station'}}};expect(()=>c.commands.enqueue([{...product,plan:{...productPlan,input:{...productPlan.input,name:'Newer unsent coffee'}}},corrected])).toThrow('previously saved setup row');expect(c.commands.queue()).toEqual(before);expect(native.effects()).toBe(1);expect(native.posts).toHaveLength(1);c.commands.enqueue([product,corrected]);const result=await c.commands.run();result.acknowledge();expect(native.effects()).toBe(2);expect(native.posts.filter(post=>post.url.endsWith('/commerce/products'))).toHaveLength(1);expect(c.commands.queue()!.jobs[0]).toEqual(before.jobs[0]);expect(native.view().products[0]).toMatchObject({name:'Coffee',station:'Corrected station'});
});
it('actual room partial batch refuses a newer field on its saved room before the presenter can skip that row or clear the whole grid',async()=>{
 const c=setupBrowser(),native=setupNative(c),fields=setupFormFields({}),calls:string[]=[];let reject=true;const mounted=await mountSetup(c,async(url,init)=>{if(init?.method==='POST'){calls.push(String(init.body));const body=JSON.parse(String(init.body));if(body.operation==='room.save'&&body.input.number==='102'&&reject){reject=false;return Response.json({error:{code:'VALIDATION_FAILED',message:'Native room row rejected before effects'}},{status:422});}}return native.request(url,init);}),nodes=()=>setupNodes(mounted.render());
 try{
  Reflect.apply(setupButton(nodes(),'Add rooms').props.onClick as ()=>void,undefined,[]);Reflect.apply(setupButton(nodes(),'Add room row').props.onClick as ()=>void,undefined,[]);const ranges=nodes().filter(node=>node.type==='TextInput'&&String(node.props.name).endsWith('-numbers')).map(node=>String(node.props.name));expect(ranges).toHaveLength(2);for(const [index,name]of ranges.entries())Object.assign(fields.draft,{[name]:String(101+index),[name.replace('-numbers','-type')]:'setup-type',[name.replace('-numbers','-floor')]:'1'});let form=fields.bind(nodes());fields.mark(form);form=fields.bind(nodes());fields.submit(form);await vi.waitFor(()=>{expect(calls).toHaveLength(2);expect(nodes().find(node=>node.type==='RightDrawer'&&node.props.title==='Room entry grid')?.props.dismissDisabled).toBe(false);});const before=JSON.parse(c.values.get(c.key)!);expect(before.jobs.map((job:{state:string})=>job.state)).toEqual(['saved','rejected']);fields.draft[ranges[0].replace('-numbers','-floor')]='Newer unsent floor';fields.draft[ranges[1]]='103';form=fields.bind(nodes());fields.mark(form);form=fields.bind(nodes());fields.submit(form);await vi.waitFor(()=>expect(nodes().some(node=>node.type==='Notice'&&String(node.props.children).includes('previously saved setup row'))).toBe(true));expect(calls).toHaveLength(2);expect(native.effects()).toBe(1);expect(fields.draft[ranges[0].replace('-numbers','-floor')]).toBe('Newer unsent floor');expect(nodes().find(node=>node.type==='RightDrawer'&&node.props.title==='Room entry grid')?.props).toMatchObject({open:true,dirty:true});expect(fields.form.reset).not.toHaveBeenCalled();expect(mounted.router.refresh).not.toHaveBeenCalled();expect(JSON.parse(c.values.get(c.key)!)).toEqual(before);expect(c.events.filter(event=>event.type==='erpx:record-command-saved')).toEqual([]);
 }finally{mounted.close();}
});
it('actual room partial correction keeps saved A unchanged and sends only corrected B at A native receipt version without replacing its original review',async()=>{
 const c=setupBrowser(),native=setupNative(c),fields=setupFormFields({}),calls:{body:string;key:string}[]=[];let reject=true;const mounted=await mountSetup(c,async(url,init)=>{if(init?.method==='POST'){const body=String(init.body);calls.push({body,key:new Headers(init.headers).get('Idempotency-Key')!});const command=JSON.parse(body);if(command.operation==='room.save'&&command.input.number==='102'&&reject){reject=false;return Response.json({error:{code:'VALIDATION_FAILED',message:'Native room B rejected before effects'}},{status:422});}}return native.request(url,init);}),nodes=()=>setupNodes(mounted.render());
 try{
  Reflect.apply(setupButton(nodes(),'Add rooms').props.onClick as ()=>void,undefined,[]);Reflect.apply(setupButton(nodes(),'Add room row').props.onClick as ()=>void,undefined,[]);const ranges=nodes().filter(node=>node.type==='TextInput'&&String(node.props.name).endsWith('-numbers')).map(node=>String(node.props.name));expect(ranges).toHaveLength(2);for(const [index,name]of ranges.entries())Object.assign(fields.draft,{[name]:String(101+index),[name.replace('-numbers','-type')]:'setup-type',[name.replace('-numbers','-floor')]:'1'});let form=fields.bind(nodes());fields.mark(form);form=fields.bind(nodes());const originalFormKey=form.key;fields.submit(form);await vi.waitFor(()=>{expect(calls).toHaveLength(2);expect(nodes().find(node=>node.type==='RightDrawer'&&node.props.title==='Room entry grid')?.props.dismissDisabled).toBe(false);});const before=JSON.parse(c.values.get(c.key)!);expect(before.jobs.map((job:{state:string})=>job.state)).toEqual(['saved','rejected']);expect(before.jobs[0].receipt).toMatchObject({propertyId:'setup-hotel',version:3});expect(c.source.data.hotel!.property.version).toBe(2);expect(native.view().hotel!.property.version).toBe(3);expect(native.effects()).toBe(1);
  fields.draft[ranges[1].replace('-numbers','-floor')]='2';form=fields.bind(nodes());expect(form.key).toBe(originalFormKey);fields.mark(form);form=fields.bind(nodes());fields.submit(form);await vi.waitFor(()=>expect(mounted.router.refresh).toHaveBeenCalledOnce());expect(calls).toHaveLength(3);expect(JSON.parse(calls[2].body)).toMatchObject({operation:'room.save',propertyId:'setup-hotel',expectedVersion:3,input:{number:'102',typeId:'setup-type',floor:'2',active:true}});expect(calls[2].key).not.toBe(calls[1].key);expect(calls.filter(call=>JSON.parse(call.body).input.number==='101')).toHaveLength(1);expect(native.effects()).toBe(2);expect(native.view().hotel!.property.version).toBe(4);expect(records(native.view().hotel!,'room').map(row=>row.data)).toEqual(expect.arrayContaining([expect.objectContaining({number:'101',floor:'1'}),expect.objectContaining({number:'102',floor:'2'})]));const after=JSON.parse(c.values.get(c.key)!);expect(after.jobs[0]).toEqual(before.jobs[0]);expect(after.jobs[1]).toEqual(before.jobs[1]);expect(after.jobs[2].receipt).toMatchObject({propertyId:'setup-hotel',version:4});expect(c.source.data.hotel!.property.version).toBe(2);expect(c.events.filter(event=>event.type==='erpx:record-command-saved')).toHaveLength(1);expect(fields.form.reset).not.toHaveBeenCalled();expect(nodes().some(node=>node.type==='form')).toBe(false);
 }finally{mounted.close();}
});

it('the canonical PMS setup entry forwards the original params and complete query promises to the real standard server owner',async()=>{
 const require=setupRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),source=setupRead(setupResolve("packages/workplace/src/routes/holdings/[holdingSlug]/pms/setup/_entry.tsx"),'utf8'),compiled=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,loaded={exports:{} as Record<string,unknown>},owner=()=>null;
 new Function('require','module','exports',compiled)((name:string):unknown=>{if(name==='react/jsx-runtime')return require(name);if(name.endsWith("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/hospitality/setup/standard-screen"))return {default:owner};throw Error('Unexpected actual PMS setup entry import '+name);},loaded,loaded.exports);
 const params=Promise.resolve({holdingSlug:'setup'}),searchParams=Promise.resolve({branch:'original-branch',property:'original-hotel',q:['First','Second'],returnTo:'/holdings/setup/pms/settings?property=original-hotel'}),Page=loaded.exports.default as(props:{params:typeof params;searchParams:typeof searchParams})=>React.ReactElement<{params:typeof params;searchParams:typeof searchParams}>,tree=Page({params,searchParams});expect(tree.type).toBe(owner);expect(tree.props.params).toBe(params);expect(tree.props.searchParams).toBe(searchParams);expect(await tree.props.searchParams).toEqual({branch:'original-branch',property:'original-hotel',q:['First','Second'],returnTo:'/holdings/setup/pms/settings?property=original-hotel'});
});
import {recordDrawerHref as pmsAckRecordDrawerHref,withoutRecordDrawer as pmsAckWithoutRecordDrawer} from '@erpx/feature-records/lib/record-drawer';

// Actual PMS original completion crosses the real mounted RecordDrawer dirty
// controller. Only DOM/router/primitives are ports; neither ACK nor dirty logic
// is replaced. Existing owner callbacks and native command decoder remain live.
async function mountPmsEditAckDrawer(f:Awaited<ReturnType<typeof actualPmsWorkspaceSend>>){
 const require=setupRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),root=process.env.ERPX_PMS_RECORD_ACK_SOURCE_ROOT??'.',source=setupRead(setupResolve(root,'packages/feature-records/src/components/record-drawer.tsx'),'utf8'),compiled=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText,hooks=setupHooks(),loaded={exports:{} as Record<string,unknown>},router={push:vi.fn(),pushPreservingQuery:vi.fn()},attributes=new Map<string,string>();
 const href='https://pms-dev-erpx.jesadakorn.com/acme/acme-co/holding/housekeeping?property=original-hotel&floor=3&taskState=working&detailKind=pms_record&detailId=original-task';
 Object.assign(window.location,{href,origin:new URL(href).origin,pathname:new URL(href).pathname,search:new URL(href).search});
 Object.assign(window,{history:{state:{original:'native-history'},pushState:vi.fn()}});
 const controls:DrawerElement[]=[],observerCallbacks:(()=>void)[]=[];
 class DrawerElement{
  tagName:string;name='';id='';type='';value='';checked=false;disabled=false;labels:{textContent:string|null}[]=[];
  get attributes(){return Array.from(attributes,([name,value])=>({name,value}));}
  constructor(tagName='FORM'){this.tagName=tagName;}
  matches(selector:string){return this.tagName==='FORM'&&selector==='form';}
  closest(selector:string){return selector.includes('form')?this.tagName==='FORM'?this:f.form:null;}
  querySelectorAll(){return this.tagName==='FORM'?controls:[];}
  setAttribute(name:string,value:string){attributes.set(name,value);}
  getAttribute(name:string){return attributes.get(name)??null;}
 }
 Object.setPrototypeOf(f.form,DrawerElement.prototype);
 Object.assign(f.form,{tagName:'FORM',isConnected:true,querySelectorAll:()=>controls,getAttribute:(name:string)=>attributes.get(name)??null,setAttribute:(name:string,value:string)=>{attributes.set(name,value);}});
 vi.stubGlobal('Element',DrawerElement);
 vi.stubGlobal('MutationObserver',class{constructor(callback:()=>void){observerCallbacks.push(callback);}observe=vi.fn();disconnect=vi.fn();});
 vi.stubGlobal('requestAnimationFrame',(callback:FrameRequestCallback)=>{callback(0);return 0;});
 const content=Object.assign(new EventTarget(),{contains:(node:unknown)=>node===f.form,querySelector:()=>null,querySelectorAll:()=>[]});
 const ports=(name:string):unknown=>{
  if(name==='react')return hooks.react;if(name==='react/jsx-runtime')return require(name);if(name==='react-dom')return{createPortal:(node:React.ReactNode)=>node};
  if(name.endsWith("@erpx/ui/components/workspace"))return{WorkspaceDetail:'WorkspaceDetail'};if(name.endsWith("@erpx/ui/components/workspace-overlay"))return{useWorkspaceOverlay:()=>({editors:0})};if(name==="@erpx/feature-records/components/record-workspace")return{useRecordWorkspace:()=>({host:null})};
  if(name.endsWith("@erpx/feature-shared/lib/destination-search"))return{useSearchParams:()=>new URLSearchParams(window.location.search)};if(name.endsWith("@erpx/feature-shared/lib/destination-router"))return{useRouter:()=>router};
  if(name.endsWith("@erpx/ui/components/dialog"))return{Dialog:'Dialog'};if(name.endsWith("@erpx/ui/components/button"))return{Button:'Button'};if(name==="@erpx/ui/components/record-context")return{EmbeddedRecord:'EmbeddedRecord'};
  if(name==='@erpx/feature-records/lib/record-drawer')return{recordDrawerHref:pmsAckRecordDrawerHref,withoutRecordDrawer:pmsAckWithoutRecordDrawer};if(name.endsWith("@erpx/feature-work_projection/lib/context-switch"))return{registerUnsavedWork:setupRegisterUnsavedWork};if(name.endsWith('.css'))return{};
  throw Error('Unexpected actual RecordDrawer import '+name);
 };
 new Function('require','module','exports',compiled)(ports,loaded,loaded.exports);
 const Drawer=loaded.exports.RecordDrawer as (props:{title:string;fullHref:string;children:React.ReactNode})=>React.ReactNode;
 const render=()=>hooks.render(()=>setupNodes(Drawer({title:'Original housekeeping task',fullHref:href,children:null})));
 let nodes=render();const node=nodes.find(item=>item.props['data-record-drawer-content']!==undefined);if(!node||typeof node.props.ref!=='function')throw Error('Actual drawer content ref is required.');Reflect.apply(node.props.ref,undefined,[content]);nodes=render();
 const dispatchEdit=(value:string,financeBubble=false)=>{f.edit(value);const input=Object.assign(new DrawerElement('INPUT'),{name:'reason',type:'text',value,labels:[{textContent:'Reason'}]});controls.splice(0,controls.length,input);const event=new Event('input',{bubbles:true}),ownerStamp=()=>{(f.form as unknown as Element).setAttribute('data-erpx-edit-version',crypto.randomUUID());};Object.defineProperty(event,'target',{value:input});if(financeBubble)document.addEventListener('input',ownerStamp);try{content.dispatchEvent(event);if(financeBubble)document.dispatchEvent(event);window.dispatchEvent(event);}finally{if(financeBubble)document.removeEventListener('input',ownerStamp);}return render();};
 const edited=(value:string)=>dispatchEdit(value),editedThroughFinanceBubble=(value:string)=>Promise.resolve(dispatchEdit(value,true));
 return {router,render,edited,editedThroughFinanceBubble,stamp:()=>attributes.get('data-erpx-edit-version'),dirty:()=>render().find(item=>item.props['data-record-drawer-content']!==undefined)?.props['data-record-dirty'],retained:()=>render().find(item=>item.props['data-record-drawer-retained-draft']!==undefined),detachForm(){(f.form as unknown as {isConnected:boolean}).isConnected=false;observerCallbacks[0]?.();return render();},close(){const drawer=render().find(item=>item.props['data-testid']==='record-detail-drawer');if(!drawer||typeof drawer.props.onClose!=='function')throw Error('Actual drawer close handler is required.');Reflect.apply(drawer.props.onClose,undefined,[]);return render();},confirmation:()=>render().find(item=>item.type==='Dialog'&&item.props.title==='Discard unsaved changes?'),dispose(){hooks.close();}};
}

it.each((['front','backoffice'] as const).flatMap(owner=>([false,true] as const).map(newer=>[owner,newer] as const)))('actual %s PMS original ACK after optional read failure keeps RecordDrawer newer=%s notes dirty through close',async(owner,newer)=>{
 const f=await actualPmsWorkspaceSend(owner,{failRefresh:true}),mounted=await mountPmsEditAckDrawer(f);
 expect(mounted.stamp()).toBeUndefined();mounted.edited('Submitted housekeeping note');const submittedStamp=mounted.stamp();expect(submittedStamp).toMatch(/^[a-f0-9-]{36}$/);expect(mounted.dirty()).toBe(true);
 const running=f.submit();
 try{
  await f.started;if(newer){mounted.edited('Newer unsent housekeeping note');const newerStamp=mounted.stamp();expect(newerStamp).not.toBe(submittedStamp);/* The drawer must compare against its retained edit version even if an owner-side update restores the DOM attribute to the submitted version before the old ACK arrives. */(f.form as unknown as Element).setAttribute('data-erpx-edit-version',submittedStamp!);expect(mounted.stamp()).toBe(submittedStamp);}f.release();await running;
  expect(f.setData).not.toHaveBeenCalled();expect(f.completions).toEqual([{form:f.form,formEditVersion:submittedStamp}]);expect(f.attempts.current.size).toBe(0);expect(f.reset).toHaveBeenCalledTimes(newer?0:1);expect(f.draft()).toBe(newer?'Newer unsent housekeeping note':'');expect(mounted.dirty()).toBe(newer);
  const unload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unload);expect(unload.defaultPrevented).toBe(newer);
  mounted.close();expect(mounted.confirmation()?.props.open).toBe(newer);
  if(newer){expect(mounted.router.push).not.toHaveBeenCalled();expect(mounted.router.pushPreservingQuery).not.toHaveBeenCalled();const keep=setupNodes(mounted.confirmation()?.props.children as React.ReactNode).find(item=>item.type==='Button'&&item.props.children==='Keep editing');expect(keep).toBeDefined();Reflect.apply(keep!.props.onClick as ()=>void,undefined,[]);expect(mounted.confirmation()?.props.open).toBe(false);expect(mounted.dirty()).toBe(true);expect(f.draft()).toBe('Newer unsent housekeeping note');}
  else expect(mounted.router.pushPreservingQuery).toHaveBeenCalledExactlyOnceWith('/acme/acme-co/holding/housekeeping?property=original-hotel&floor=3&taskState=working',{scroll:false});
  expect(f.fetcher.mock.calls.filter(([,init])=>init?.method==='POST')).toHaveLength(1);
 }finally{f.release();mounted.dispose();f.reads.cancel();}
});

it('actual PMS RecordDrawer retains a newer field when the acknowledged action unmounts and still guards discard',async()=>{
 const f=await actualPmsWorkspaceSend('front'),mounted=await mountPmsEditAckDrawer(f);let pending:Promise<void>|undefined;
 try{
  mounted.edited('Submitted correction reason');
  const submittedStamp=mounted.stamp();pending=f.submit();
  await f.started;
  mounted.edited('Newer correction reason entered while request was pending');
  const newerStamp=mounted.stamp();
  expect(newerStamp).not.toBe(submittedStamp);
  f.release();await pending;
  expect(f.completions).toEqual([{form:f.form,formEditVersion:submittedStamp}]);
  expect(f.reset).not.toHaveBeenCalled();
  expect(f.draft()).toBe('Newer correction reason entered while request was pending');
  mounted.detachForm();
  expect(mounted.dirty()).toBe(true);
  expect(mounted.retained()).toBeDefined();
  expect(setupNodes(mounted.retained()!.props.children as React.ReactNode).some(node=>node.type==='output'&&node.props.children==='Newer correction reason entered while request was pending')).toBe(true);
  const unload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unload);expect(unload.defaultPrevented).toBe(true);
  mounted.close();expect(mounted.confirmation()?.props.open).toBe(true);expect(mounted.router.push).not.toHaveBeenCalled();expect(mounted.router.pushPreservingQuery).not.toHaveBeenCalled();
  const keep=setupNodes(mounted.confirmation()?.props.children as React.ReactNode).find(node=>node.type==='Button'&&node.props.children==='Keep editing');
  expect(keep).toBeDefined();Reflect.apply(keep!.props.onClick as ()=>void,undefined,[]);
  expect(mounted.confirmation()?.props.open).toBe(false);expect(mounted.dirty()).toBe(true);expect(mounted.retained()).toBeDefined();
  mounted.close();const discard=setupNodes(mounted.confirmation()?.props.children as React.ReactNode).find(node=>node.type==='Button'&&node.props.children==='Discard changes');
  expect(discard).toBeDefined();Reflect.apply(discard!.props.onClick as ()=>void,undefined,[]);
  expect(mounted.dirty()).toBe(false);expect(mounted.retained()).toBeUndefined();expect(mounted.router.pushPreservingQuery).toHaveBeenCalledOnce();
 }finally{f.release();mounted.dispose();f.reads.cancel();}
});

it('actual PMS RecordDrawer keeps a reopened record draft when its pre-close command ACK arrives late',async()=>{
 const f=await actualPmsWorkspaceSend('front',{failRefresh:true}),mounted=await mountPmsEditAckDrawer(f),originalHref=window.location.href;
 const setLocation=(href:string)=>{const url=new URL(href);Object.assign(window.location,{href:url.href,origin:url.origin,pathname:url.pathname,search:url.search});},drawer=()=>setupNodes(mounted.render()).find(item=>item.props['data-testid']==='record-detail-drawer');
 mounted.edited('Submitted before closing');const submittedStamp=mounted.stamp()!,running=f.submit();
 try{
  await f.started;mounted.close();expect(mounted.confirmation()?.props.open).toBe(true);
  const discard=setupNodes(mounted.confirmation()?.props.children as React.ReactNode).find(item=>item.type==='Button'&&item.props.children==='Discard changes');expect(discard).toBeDefined();Reflect.apply(discard!.props.onClick as ()=>void,undefined,[]);
  expect(mounted.router.pushPreservingQuery).toHaveBeenCalledOnce();const closedHref=new URL(String(mounted.router.pushPreservingQuery.mock.calls[0]?.[0]),originalHref).href;setLocation(closedHref);expect(drawer()?.props.open).toBe(false);expect(mounted.dirty()).toBe(false);
  setLocation(originalHref);expect(drawer()?.props.open).toBe(true);mounted.edited('Newer draft after reopening');const newerStamp=mounted.stamp()!;expect(newerStamp).not.toBe(submittedStamp);
  // Model an owner render restoring an old submitted stamp while the drawer retains the newer edit revision.
  (f.form as unknown as Element).setAttribute('data-erpx-edit-version',submittedStamp!);f.release();await running;
  expect(f.completions).toEqual([{form:f.form,formEditVersion:submittedStamp}]);expect(f.attempts.current.size).toBe(0);expect(f.reset).not.toHaveBeenCalled();expect(f.draft()).toBe('Newer draft after reopening');expect(mounted.dirty()).toBe(true);
  const unload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unload);expect(unload.defaultPrevented).toBe(true);mounted.close();expect(mounted.confirmation()?.props.open).toBe(true);expect(mounted.router.pushPreservingQuery).toHaveBeenCalledOnce();
 }finally{f.release();mounted.dispose();f.reads.cancel();}
});

it('actual PMS RecordDrawer distinguishes a missing or stale command stamp from an explicit scoped discard',async()=>{
 const f=await actualPmsWorkspaceSend('front'),mounted=await mountPmsEditAckDrawer(f);
 try{
  mounted.edited('New unsent housekeeping note');const current=mounted.stamp()!;expect(mounted.dirty()).toBe(true);
  const staleFormDetails=[{form:f.form,formEditVersion:null},{form:f.form},{form:f.form,formEditVersion:'stale-original'}];
  for(const detail of [...staleFormDetails,{root:{contains:(node:unknown)=>node===f.form}}]){window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail}));expect(mounted.dirty()).toBe(true);expect(mounted.stamp()).toBe(current);}
  for(const detail of staleFormDetails){window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail}));expect(mounted.dirty()).toBe(true);expect(mounted.stamp()).toBe(current);}
  window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{form:f.form,formEditVersion:'stale-original',root:{contains:(node:unknown)=>node===f.form}}}));expect(mounted.dirty()).toBe(true);expect(mounted.stamp()).toBe(current);
  window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{root:{contains:()=>false}}}));expect(mounted.dirty()).toBe(true);
  window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{form:f.form,formEditVersion:current}}));expect(mounted.dirty()).toBe(false);
  mounted.edited('A newer unsent housekeeping note');const newer=mounted.stamp();expect(mounted.dirty()).toBe(true);expect(newer).not.toBe(current);
  (f.form as unknown as Element).setAttribute('data-erpx-edit-version',current);
  for(const type of ['erpx:record-command-saved','erpx:record-saved']){window.dispatchEvent(new CustomEvent(type,{detail:{form:f.form,formEditVersion:current}}));expect(mounted.dirty()).toBe(true);}
  window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form:f.form,formEditVersion:newer}}));expect(mounted.dirty()).toBe(false);
  mounted.edited('Latest unsent housekeeping note');expect(mounted.dirty()).toBe(true);expect(mounted.stamp()).not.toBe(newer);
  window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{root:{contains:(node:unknown)=>node===f.form}}}));expect(mounted.dirty()).toBe(false);mounted.close();expect(mounted.router.pushPreservingQuery).toHaveBeenCalledOnce();expect(f.fetcher).not.toHaveBeenCalled();expect(f.draft()).toBe('Latest unsent housekeeping note');
 }finally{f.release();mounted.dispose();f.reads.cancel();}
});

it('actual PMS RecordDrawer reconciles the final Finance bubble stamp and preserves newer edits against a stale ACK',async()=>{
 const f=await actualPmsWorkspaceSend('front'),mounted=await mountPmsEditAckDrawer(f);
 try{
  await mounted.editedThroughFinanceBubble('Submitted housekeeping note');const submittedStamp=mounted.stamp()!;expect(submittedStamp).toMatch(/^[a-f0-9-]{36}$/);expect(mounted.dirty()).toBe(true);
  const submitted={form:f.form,formEditVersion:submittedStamp};window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:submitted}));expect(mounted.dirty()).toBe(false);
  await mounted.editedThroughFinanceBubble('Newer unsent housekeeping note');const newerStamp=mounted.stamp()!;expect(newerStamp).not.toBe(submittedStamp);expect(mounted.dirty()).toBe(true);
  window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:submitted}));expect(mounted.dirty()).toBe(true);
  window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:submitted}));expect(mounted.dirty()).toBe(true);
  window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form:f.form,formEditVersion:newerStamp}}));expect(mounted.dirty()).toBe(false);
 }finally{mounted.dispose();f.reads.cancel();}
});
