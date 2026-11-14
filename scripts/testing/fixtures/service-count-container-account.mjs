// Synthetic HTTP-only normal Account login and SSO. Cookies stay in a 0600 case file.
import fs from 'node:fs';
import http from 'node:http';
const persona=process.argv[2]??'maker';if(!['maker','finance'].includes(persona))throw Error('PERSONA_INVALID');
const dir='/case/',credentials=JSON.parse(fs.readFileSync(dir+'credentials.json','utf8'));
const jars=new Map(),account='account-dev-erpx.jesadakorn.com',holding='acme.jesadakorn.com';
function request(host,path,{method='GET',body,origin}={}) {return new Promise((resolve,reject)=>{
 const cookies=jars.get(host)??new Map(),headers={host,accept:'application/json',...(cookies.size?{cookie:[...cookies].map(([k,v])=>`${k}=${v}`).join('; ')}:{}),...(origin?{origin}:{}),...(body?{'content-type':'application/json'}:{})};
 const req=http.request({hostname:'127.0.0.1',port:44817,path,method,headers},res=>{const parts=[];res.on('data',b=>parts.push(b));res.on('end',()=>{for(const line of res.headers['set-cookie']??[]){const pair=line.split(';')[0],i=pair.indexOf('=');if(i>0){const name=pair.slice(0,i),value=pair.slice(i+1);if(value)cookies.set(name,value);else cookies.delete(name);}}jars.set(host,cookies);resolve({status:res.statusCode,location:res.headers.location,body:Buffer.concat(parts)});});});req.on('error',reject);req.end(body?JSON.stringify(body):undefined);
});}
const health=await request('127.0.0.1:44817','/api/health'),ready=await request('127.0.0.1:44817','/api/ready');if(health.status!==200||ready.status!==503)throw Error(`FOUNDATION_LIVENESS_${health.status}_READINESS_${ready.status}`);
const login=await request(account,'/api/auth/sign-in/email',{method:'POST',origin:`https://${account}`,body:{email:credentials[persona].email,password:credentials[persona].password}});if(login.status!==200)throw Error(`ACCOUNT_SIGNIN_${login.status}`);
const start=await request(holding,'/sso/start?next=%2Fservice');if(start.status!==303||!start.location)throw Error(`SSO_START_${start.status}`);
const authUrl=new URL(start.location);if(authUrl.hostname!==account||authUrl.pathname!=='/sso/authorize')throw Error('SSO_ACCOUNT_TARGET_INVALID');
const authorize=await request(account,authUrl.pathname+authUrl.search);if(authorize.status!==303||!authorize.location)throw Error(`SSO_AUTHORIZE_${authorize.status}`);
const callbackUrl=new URL(authorize.location);if(callbackUrl.hostname!==holding||callbackUrl.pathname!=='/sso/callback')throw Error('SSO_HOLDING_TARGET_INVALID');
const callback=await request(holding,callbackUrl.pathname+callbackUrl.search);if(callback.status!==303||!jars.get(holding)?.has('__Host-erpx-product'))throw Error(`SSO_CALLBACK_${callback.status}`);
const value={cookie:[...jars.get(holding)].map(([k,v])=>`${k}=${v}`).join('; '),assignmentId:credentials[persona].assignmentId,assignmentVersion:credentials[persona].assignmentVersion,companyId:credentials.companyId};
fs.writeFileSync(dir+'account-proof-'+persona+'.json',JSON.stringify(value),{mode:0o600,flag:'wx'});console.log('normal Account sign-in and Holding SSO passed: '+persona);
