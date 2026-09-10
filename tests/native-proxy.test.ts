import { afterEach,it,expect,vi } from 'vitest';
import { NextRequest } from 'next/server';
import { proxy } from '../src/proxy';
afterEach(()=>vi.unstubAllEnvs());
it('keeps branch rewrites on the loopback origin and replaces caller scope headers',()=>{
 vi.stubEnv('__NEXT_NO_MIDDLEWARE_URL_NORMALIZE','true');
 const request=new NextRequest('http://127.0.0.1:3101/api/v1/holdings/acme/branches/bangkok/people/people/person?x=1',
  {headers:{'x-erpx-workspace-kind':'holding','x-erpx-department-slug':'forged'}});
 const response=proxy(request);
 expect(response.headers.get('x-middleware-rewrite')).toBe('http://127.0.0.1:3101/api/v1/holdings/acme/people/people/person?x=1');
 expect(response.headers.get('x-middleware-request-x-erpx-workspace-kind')).toBe('branch');
 expect(response.headers.get('x-middleware-request-x-erpx-branch-slug')).toBe('bangkok');
 expect(response.headers.get('x-middleware-request-x-erpx-department-slug')).toBeNull();
});
it('rejects foreign hosts, redirects HTTP and sets secure workspace cookies in production',()=>{
 vi.stubEnv('NODE_ENV','production');vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://erpx.jesadakorn.com');
 expect(proxy(new NextRequest('http://127.0.0.1:3101/login',{headers:{host:'evil.example'}})).status).toBe(421);
 const response=proxy(new NextRequest('http://127.0.0.1:3101/login',{headers:{host:'erpx.jesadakorn.com','x-forwarded-proto':'http'}}));
 expect(response.status).toBe(308);expect(response.headers.get('location')).toBe('https://erpx.jesadakorn.com/login');
 const workspace=proxy(new NextRequest('http://127.0.0.1:3101/holdings/acme/home',{headers:{host:'erpx.jesadakorn.com','x-forwarded-proto':'https'}}));
 expect(workspace.cookies.get('erpx_last_workspace')).toMatchObject({secure:true,httpOnly:true});
});
