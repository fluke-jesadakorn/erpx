import {describe,expect,it,vi} from 'vitest';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers()}));
import {
  createNativeManageErpxClient,
  nativeManageErpxConfiguration,
  nativeManageErpxFailure,
  NativeManageErpxError,
  type NativeManageErpxTransport,
} from '../../../packages/feature-platform_administration/src/adapters/server/native-manage-erpx';

const config=(overrides:Record<string,unknown>={})=>({
  privateBaseUrl:'http://127.0.0.1:3301',
  timeoutMs:1_000,
  hosts:['https://acme.example.test'],
  ...overrides,
});

const responseBody={
  data:{
    enabled:true,
    slot:{slug:'acme',company_name:'Acme Company',branch_name:'Main',state:'provisioning'},
  },
  context:{userId:'owner'},
};

function transport(value:unknown=responseBody,status=200){
  const calls:{method:string;url:string;host:string|null;cookie:string|null;body:unknown}[]=[];
  const fn=vi.fn(async (method:'GET',url:URL,headers:Headers,body:undefined,signal:AbortSignal)=>{
    expect(signal.aborted).toBe(false);
    calls.push({method,url:url.toString(),host:headers.get('host'),cookie:headers.get('cookie'),body});
    return new Response(JSON.stringify(value),{status,headers:{'content-type':'application/json; charset=utf-8'}});
  });
  return {fn:fn as unknown as NativeManageErpxTransport,calls};
}

const incoming=new Headers({host:'acme.example.test',cookie:'erpx=synthetic'});

describe('native Manage ERPX onboarding transport',()=>{
  it('uses only a loopback private base and exact registered front origins',()=>{
    expect(nativeManageErpxConfiguration(JSON.stringify(config()))).toMatchObject({
      privateBaseUrl:'http://127.0.0.1:3301',
      hosts:['https://acme.example.test'],
      timeoutMs:1_000,
    });
    const invalidConfiguration=(raw:string)=>{
      try{nativeManageErpxConfiguration(raw);}
      catch(error){expect(error).toMatchObject({code:'NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID'});return;}
      throw new Error('expected configuration rejection');
    };
    for(const invalid of [
      config({privateBaseUrl:'http://10.0.0.8:3301'}),
      config({privateBaseUrl:'http://127.0.0.1:3100'}),
      config({privateBaseUrl:'http://127.0.0.1:3301/path'}),
      config({timeoutMs:249}),
      config({hosts:[]}),
      config({hosts:['http://acme.example.test']}),
      config({hosts:['https://acme.example.test','https://acme.example.test']}),
      config({hosts:['https://acme.example.test/path']}),
    ]) invalidConfiguration(JSON.stringify(invalid));
    invalidConfiguration('{');
    invalidConfiguration(JSON.stringify({...config(),extra:true}));
  });

  it('forwards the exact Host/session and returns the bounded owner view',async()=>{
    const fake=transport(),client=createNativeManageErpxClient(config(),fake.fn);
    expect(await client.onboardingView(incoming,'owner')).toEqual(responseBody.data);
    expect(fake.calls).toEqual([{
      method:'GET',
      url:'http://127.0.0.1:3301/api/v1/onboarding',
      host:'acme.example.test',
      cookie:'erpx=synthetic',
      body:undefined,
    }]);
  });

  it('accepts an unbound owner without inventing a slot',async()=>{
    const fake=transport({data:{enabled:false,slot:null},context:{userId:'owner'}});
    const client=createNativeManageErpxClient(config(),fake.fn);
    expect(await client.onboardingView(incoming,'owner')).toEqual({enabled:false,slot:null});
  });

  it('denies an unregistered Host before any transport call',async()=>{
    const fake=transport(),client=createNativeManageErpxClient(config(),fake.fn);
    await expect(client.onboardingView(new Headers({host:'other.example.test'}),'owner'))
      .rejects.toMatchObject({code:'NATIVE_MANAGE_ERPX_AUTHORITY_INVALID'});
    expect(fake.fn).not.toHaveBeenCalled();
  });

  it('rejects a mismatched context, extra fields and malformed slot state',async()=>{
    for(const value of [
      {...responseBody,context:{userId:'other'}},
      {...responseBody,extra:true},
      {data:{enabled:true,slot:null,extra:true},context:{userId:'owner'}},
      {data:{enabled:true,slot:{...responseBody.data.slot,state:'failed'}},context:{userId:'owner'}},
      {data:{enabled:true,slot:{...responseBody.data.slot,slug:'Acme'}},context:{userId:'owner'}},
    ]){
      const fake=transport(value),client=createNativeManageErpxClient(config(),fake.fn);
      await expect(client.onboardingView(incoming,'owner'))
        .rejects.toMatchObject({code:'NATIVE_MANAGE_ERPX_RESPONSE_INVALID'});
    }
  });

  it('maps a bounded owner error and a non-JSON success without leaking a body',async()=>{
    const denied=transport({error:{code:'VERIFY_EMAIL',message:'Verify your email before joining.'}},403);
    const client=createNativeManageErpxClient(config(),denied.fn);
    await expect(client.onboardingView(incoming,'owner')).rejects.toMatchObject({
      code:'VERIFY_EMAIL',status:403,message:'Verify your email before joining.',
    });

    const malformed=vi.fn(async()=>new Response('<html>',{status:200,headers:{'content-type':'text/html'}}));
    const invalid=createNativeManageErpxClient(config(),malformed as unknown as NativeManageErpxTransport);
    await expect(invalid.onboardingView(incoming,'owner'))
      .rejects.toMatchObject({code:'NATIVE_MANAGE_ERPX_RESPONSE_INVALID'});
  });

  it('fails closed without configuration and never falls back to the legacy owner query',async()=>{
    const wrapper=await import('../../../packages/workplace/src/lib/native-manage-erpx');
    const saved=process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG;
    try{
      delete process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG;
      expect(wrapper.nativeManageErpxAvailable()).toBe(false);
      expect(await wrapper.nativeManageErpxOnboardingView(new Headers({host:'acme.example.test'}),'owner')).toBeUndefined();
      process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG='{';
      expect(wrapper.nativeManageErpxAvailable()).toBe(true);
      await expect(wrapper.nativeManageErpxOnboardingView(new Headers({host:'acme.example.test'}),'owner'))
        .rejects.toMatchObject({status:503,code:'NATIVE_MANAGE_ERPX_UNAVAILABLE'});
    }finally{
      if(saved===undefined) delete process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG;
      else process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG=saved;
    }
  });

  it('maps configured failures to a bounded unavailable response',()=>{
    expect(nativeManageErpxFailure(new NativeManageErpxError('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID')))
      .toMatchObject({status:503,code:'NATIVE_MANAGE_ERPX_UNAVAILABLE'});
    expect(nativeManageErpxFailure(new NativeManageErpxError('VERIFY_EMAIL',403,'denied')))
      .toEqual({status:403,code:'VERIFY_EMAIL',message:'denied'});
    expect(nativeManageErpxFailure(new NativeManageErpxError('NATIVE_MANAGE_ERPX_AUTHORITY_INVALID')))
      .toMatchObject({status:403,code:'FORBIDDEN'});
    expect(nativeManageErpxFailure(new NativeManageErpxError('NATIVE_MANAGE_ERPX_RESPONSE_INVALID')))
      .toMatchObject({status:503,code:'NATIVE_MANAGE_ERPX_UNAVAILABLE'});
    expect(nativeManageErpxFailure(new Error('network'))).toBeUndefined();
  });
});
