import {describe,expect,it,vi} from 'vitest';

vi.mock('server-only',()=>({}));

import {
  createNativeWorkSnapshotReader,
  nativeWorkSnapshotConfigured,
  readNativeWorkSnapshot,
  type NativeWorkSnapshotTransport,
} from '../../../packages/server-work_projection/src/adapters/native-work-snapshot';
import {ApiError} from '../../../packages/server-runtime/src/adapters/http';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44992',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  origin:'https://acme.example.com',
  ...overrides,
});
const actor={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',
  scope:{kind:'holding' as const},
  authority:{revision:7,assignmentVersion:3},
};
const context={
  userId:'user',holdingId:'h001',companyId:'company',
  assignmentId:'assignment',revision:7,
};
const items=[{
  id:'maintenance:work',type:'maintenance_work',queue:'todo',canAct:true,
  actionKind:null,relationships:['assigned'],updatedAt:1,sourceId:'work',
  href:'/maintenance/work',
}];
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const read=(transport:NativeWorkSnapshotTransport,raw=config())=>
  createNativeWorkSnapshotReader(raw,transport)(incoming(),actor);

describe('native Work snapshot transport',()=>{
  it('forwards the exact loopback target, captured cookie and selected context',async()=>{
    const transport=vi.fn().mockResolvedValue(ok({data:{items,unavailableSources:['supportWork']},context}));
    const snapshot=await read(transport);
    expect(snapshot.items).toEqual(items);
    expect(snapshot.unavailableSources).toEqual(['supportWork']);
    const [url,headers]=transport.mock.calls[0]!;
    expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/work/native-snapshot');
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('origin')).toBe('https://acme.example.com');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  });

  it('rejects a changed context instead of returning foreign facts',async()=>{
    const transport=vi.fn().mockResolvedValue(ok({
      data:{items,unavailableSources:[]},
      context:{...context,revision:6},
    }));
    const error=await read(transport).then(()=>null,reason=>reason);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).code).toBe('WORK_CONTEXT_CHANGED');
  });

  it('rejects presentation fields that the native availability contract does not carry',async()=>{
    const transport=vi.fn().mockResolvedValue(ok({
      data:{items:[{...items[0],title:'Synthetic'}],unavailableSources:[]},
      context,
    }));
    const error=await read(transport).then(()=>null,reason=>reason);
    expect((error as ApiError).code).toBe('NATIVE_WORK_SNAPSHOT_RESPONSE_INVALID');
  });

  it('rejects an unknown Holding target and an invalid configuration',async()=>{
    const transport=vi.fn();
    const unknown=await createNativeWorkSnapshotReader(config(),transport)(
      incoming(),
      {...actor,holdingSlug:'other'},
    ).then(()=>null,reason=>reason);
    expect((unknown as ApiError).code).toBe('NATIVE_WORK_SNAPSHOT_TARGET_UNAVAILABLE');
    let invalid: unknown;
    try {
      createNativeWorkSnapshotReader('{}',transport);
    } catch (error) {
      invalid = error;
    }
    expect((invalid as ApiError).code).toBe('NATIVE_WORK_SNAPSHOT_CONFIGURATION_INVALID');
    expect(transport).not.toHaveBeenCalled();
  });

  it('surfaces configured route failures with their named error',async()=>{
    const transport=vi.fn().mockResolvedValue(new Response(
      JSON.stringify({error:{code:'WORK_AUTHORITY_EXPIRED',message:'Review your current access.'}}),
      {status:409,headers:{'content-type':'application/json'}},
    ));
    const error=await read(transport).then(()=>null,reason=>reason);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).status).toBe(409);
    expect((error as ApiError).code).toBe('WORK_AUTHORITY_EXPIRED');
  });

  it('never silently falls back when the consumer is configured or unset',async()=>{
    const transport=vi.fn().mockRejectedValue(new Error('offline'));
    const error=await read(transport).then(()=>null,reason=>reason);
    expect((error as ApiError).code).toBe('NATIVE_WORK_SNAPSHOT_UNAVAILABLE');
    vi.stubEnv('ERPX_NATIVE_WORK_PROJECTION_CONFIG','');
    try{
      expect(nativeWorkSnapshotConfigured()).toBe(true);
      const unset=await readNativeWorkSnapshot(incoming(),actor).then(()=>null,reason=>reason);
      expect((unset as ApiError).code).toBe('NATIVE_WORK_SNAPSHOT_CONFIGURATION_INVALID');
    }finally{
      vi.unstubAllEnvs();
    }
    const missing=await readNativeWorkSnapshot(incoming(),actor).then(()=>null,reason=>reason);
    expect((missing as ApiError).code).toBe('NATIVE_WORK_SNAPSHOT_CONFIGURATION_REQUIRED');
  });
});
