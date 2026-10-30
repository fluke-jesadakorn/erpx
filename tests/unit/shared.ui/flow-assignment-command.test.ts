import {afterEach,expect,it,vi} from 'vitest';
import {createFlowAssignmentCommands} from '../../../packages/workplace/src/lib/flow-assignment-command';

afterEach(()=>vi.unstubAllGlobals());
const flow={kind:'stock_count',recordId:'count-a',recordVersion:4,version:3,steps:[],history:[],canManage:true,canAct:false};
const identity={user:'u',company:'c',assignment:'a',revision:'7',assignmentVersion:'2'};
const response=()=>Response.json({data:flow});
function browser(){const dispatch=vi.fn();vi.stubGlobal('window',{dispatchEvent:dispatch});vi.stubGlobal('document',{querySelector:()=>null});return dispatch;}
it('retains the exact original request after loss and retries it in the original context',async()=>{
 browser();let context={...identity},first=true;const calls:{endpoint:string;body:unknown;headers:Headers}[]=[];
 const send=vi.fn(async(endpoint:RequestInfo|URL,init?:RequestInit)=>{calls.push({endpoint:String(endpoint),body:init?.body,headers:new Headers(init?.headers)});if(first){first=false;throw Error('lost');}return response();});
 const api=createFlowAssignmentCommands(send,()=>context,()=> 'original-key'),input={stepKey:'approve',userId:'person',expectedVersion:2,recordVersion:4,reason:'Original',saveDefault:false};
 await expect(api.submit('/api/flow/a','stock_count','count-a',2,input)).rejects.toThrow('lost');
 expect(api.pending('/api/flow/a')).not.toBeNull();
 context={...identity,company:'other'};await expect(api.retry('/api/flow/a')).rejects.toThrow('original work context');expect(calls).toHaveLength(1);
 context={...identity};const remount=createFlowAssignmentCommands(send,()=>context);expect(remount.pending('/api/flow/a')).not.toBeNull();expect(await remount.retry('/api/flow/a')).toEqual({unchanged:true});
 expect(calls[1].endpoint).toBe(calls[0].endpoint);expect(calls[1].body).toBe(calls[0].body);expect([...calls[1].headers]).toEqual([...calls[0].headers]);expect(calls[1].headers.get('idempotency-key')).toBe('original-key');expect(calls[1].headers.get('x-erpx-context-company')).toBe('c');
});
it('keeps a newer draft and never acknowledges a stale response',async()=>{
 const dispatch=browser();let edit='first',resolve!:(value:Response)=>void;const form={isConnected:true,getAttribute:()=>edit} as unknown as HTMLFormElement;
 const send=vi.fn(()=>new Promise<Response>(done=>{resolve=done;}));const api=createFlowAssignmentCommands(send,()=>({...identity}),()=> 'key-newer');
 const request=api.submit('/api/flow/newer','stock_count','count-a',2,{stepKey:'approve',recordVersion:4},{form,formEditVersion:'first'});edit='newer';resolve(response());
 expect(await request).toEqual({unchanged:false});expect(dispatch.mock.calls.some(call=>['erpx:record-command-saved','erpx:work-changed'].includes(call[0].type))).toBe(false);
});
it('releases a first definitive rejection but retains a malformed success for exact retry',async()=>{
 browser();let mode='reject';const send=vi.fn(async()=>mode==='reject'?Response.json({error:{message:'invalid'}},{status:422}):Response.json({data:{id:'incomplete'}}));const api=createFlowAssignmentCommands(send,()=>({...identity}),()=> 'key-reject');
 await expect(api.submit('/api/flow/reject','stock_count','count-a',2,{recordVersion:4})).rejects.toThrow('invalid');expect(api.pending('/api/flow/reject')).toBeNull();
 mode='malformed';await expect(api.submit('/api/flow/reject','stock_count','count-a',2,{recordVersion:4})).rejects.toThrow('not acknowledged');expect(api.pending('/api/flow/reject')).not.toBeNull();
});
it('retains an uncertain attempt when a later retry is rejected or identity changes during the reply',async()=>{
 browser();let context={...identity},mode='lost';const send=vi.fn(async()=>{if(mode==='lost')throw Error('lost');if(mode==='reject')return Response.json({error:{message:'revoked'}},{status:403});context={...identity,assignment:'different'};return response();});const api=createFlowAssignmentCommands(send,()=>context,()=> 'retry-key'),input={recordVersion:4};
 await expect(api.submit('/api/flow/stale','stock_count','count-a',2,input)).rejects.toThrow('lost');mode='reject';await expect(api.retry('/api/flow/stale')).rejects.toThrow('revoked');expect(api.pending('/api/flow/stale')).not.toBeNull();
 mode='changed';await expect(api.retry('/api/flow/stale')).rejects.toThrow('original work context');context={...identity};expect(api.pending('/api/flow/stale')).not.toBeNull();
});

it('does not signal save or shared refresh when the original form disconnects',async()=>{
 const dispatch=browser();let resolve!:(value:Response)=>void;const form={isConnected:true,getAttribute:()=>"original"} as unknown as HTMLFormElement;
 const api=createFlowAssignmentCommands(vi.fn(()=>new Promise<Response>(done=>{resolve=done;})),()=>({...identity}),()=> 'key-detached');
 const request=api.submit('/api/flow/detached','stock_count','count-a',2,{recordVersion:4},{form,formEditVersion:'original'});Object.assign(form,{isConnected:false});resolve(response());
 expect(await request).toEqual({unchanged:false});expect(dispatch.mock.calls.some(call=>['erpx:record-command-saved','erpx:work-changed'].includes(call[0].type))).toBe(false);
});
