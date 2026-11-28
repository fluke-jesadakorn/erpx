import {readFileSync} from 'node:fs';
import {get} from 'node:http';

// Bun's native HTTP client is not used for gateway readiness/prewarm traffic.
if(process.versions.bun||process.version!=='v26.11.1')throw Error('PINNED_NODE_PREWARM_REQUIRED');
const input=JSON.parse(readFileSync(0,'utf8'));
const contributor=process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost';
const contributorHosts=['account.erpx.localhost:3443','acme.erpx.localhost:3443','northstar.erpx.localhost:3443','erpx-internal.erpx.localhost:3443'];
const validHost=host=>typeof host==='string'&&(contributor?contributorHosts.includes(host):/^[-a-z0-9.]+$/.test(host));
if(input.version!==1||![3100,3104].includes(input.port)||!Array.isArray(input.apps)||!input.apps.length
  ||input.apps.some(app=>!app||typeof app.id!=='string'||!/^[-a-z0-9]+$/.test(app.id)||!validHost(app.host))
  ||input.token!==null&&!/^[a-f0-9]{48}$/.test(input.token))throw Error('EXPLICIT_GATEWAY_PREWARM_REQUIRED');
for(const app of input.apps){
  await new Promise((done,reject)=>{
    if(app.path!==undefined&&app.path!==`/__erpx/${app.id}/api/health`)throw Error('INVALID_PREWARM_PATH');
    const request=get({hostname:'127.0.0.1',port:input.port,path:app.path??'/api/health',agent:false,
      headers:{host:app.host,...(input.token?{'x-erpx-workspace-preview':input.token}:{})}},response=>{
      response.once('error',fail);response.once('aborted',()=>fail(Error('APP_PREWARM_ABORTED:'+app.id)));
      response.once('end',()=>{clearTimeout(timeout);if(response.statusCode===200&&response.headers['x-erpx-app']===app.id)done();else reject(Error('APP_PREWARM_FAILED:'+app.id));});response.resume();
    });
    const timeout=setTimeout(()=>request.destroy(Error('APP_PREWARM_TIMEOUT:'+app.id)),120_000);
    function fail(error){clearTimeout(timeout);reject(error);}
    request.once('error',fail);
  });
}
console.log(JSON.stringify({prewarmed:input.apps.map(app=>app.id)}));
