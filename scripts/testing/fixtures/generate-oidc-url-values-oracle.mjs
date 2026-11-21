import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
if(process.version!=='v26.11.1')throw Error('OIDC URL oracle requires Node 26.11.1');
const root=process.cwd();
const load=p=>import(pathToFileURL(resolve(root,p)).href);
const {createAuthorizationURL}=await load('node_modules/@better-auth/core/dist/oauth2/create-authorization-url.mjs');
const {genericOAuth}=await load('node_modules/better-auth/dist/plugins/generic-oauth/index.mjs');
const client='synthetic client+ไทย', state='state +ไทย', nonce='nonce 😀', verifier='v'.repeat(64), redirect='https://account.example.test/callback?x=1&y=ไทย';
const reserved=['response_type','client_id','state','scope','redirect_uri','nonce','code_challenge_method','code_challenge'];
const cases=[['empty',''],['duplicates','?z=1&z=&=blank&x=2'],['reserved-separated','?'+reserved.map(x=>`${x}=old&other=${x}&${x}=`).join('&')],['reverse','?'+reserved.toReversed().map(x=>`${x}=old`).join('&')],['encoded-name','?%73tate=one&State=upper&state=two&%73tate=three'],['forms','?x=a+b&x=a%20b&x=%2B&tilde=%7E&broken=%ZZ&bad=%E0%A4%A'],['unicode','?ไทย=😀&e=e%CC%81&n=%C3%A9'],['empty-reserved','?state=&client_id=&scope=&nonce='],['long','?'+Array.from({length:80},(_,i)=>`x${i}=a%20b%2B%7E`).join('&')]];
const urls=[];
for(const [id,query] of cases){const endpoint='https://issuer.example.test/authorize'+query;const url=await createAuthorizationURL({id:'directory-fixture',options:{clientId:client},authorizationEndpoint:endpoint,state,codeVerifier:verifier,scopes:['openid','profile','email'],redirectURI:redirect,nonce});urls.push({id,endpoint,url:String(url),tuples:[...url.searchParams]});}
const oldFetch=globalThis.fetch;const calls=[];
globalThis.fetch=async input=>{calls.push(String(input instanceof Request?input.url:input));return Response.json({issuer:'https://issuer.example.test',authorization_endpoint:urls[4].endpoint,token_endpoint:'https://issuer.example.test/token',jwks_uri:'https://issuer.example.test/keys',id_token_signing_alg_values_supported:['RS256']});};
let generic;
try{const plugin=genericOAuth({config:[{providerId:'directory-fixture',clientId:client,clientSecret:'synthetic-only',discoveryUrl:'https://issuer.example.test/.well-known/openid-configuration',requireIdTokenVerification:true,pkce:true,scopes:['openid','profile','email'],disableSignUp:true,disableImplicitSignUp:true}]});const initialized=await plugin.init({socialProviders:[],logger:{error(){},warn(){}}});const provider=initialized.context.socialProviders[0];const result=await provider.createAuthorizationURL({state,codeVerifier:verifier,idTokenNonce:nonce,redirectURI:redirect});generic={url:String(result),tuples:[...result.searchParams],calls};if(generic.url!==urls[4].url)throw Error('generic/core URL mismatch');}finally{globalThis.fetch=oldFetch;}
const graph=[];
function record(id,input){const str=String(input);let output=null,error=null;try{output=encodeURIComponent(str);}catch(e){error=e.name;}graph.push({id,units:Array.from({length:str.length},(_,i)=>str.charCodeAt(i)),string:JSON.stringify(str),output,error});}
for(let i=0;i<128;i++)record(`ascii-${i}`,String.fromCharCode(i));
for(const [id,v] of [['literal',"!'()*-._~"],['delimiters',' +/%?#&='],['combining','e\u0301'],['nfc','é'],['thai','ภาษาไทย'],['astral','😀'],['high','\uD800'],['low','\uDC00'],['reversed','\uDC00\uD800'],['missing',undefined],['null',null],['bool',true],['number',17],['array',[1,null,'ไทย']],['object',{}]])record(id,v);
const sources=['packages/server-identity_access/src/adapters/iam-auth.ts','packages/server-identity_access/src/adapters/iam-providers.ts','node_modules/@better-auth/core/dist/oauth2/create-authorization-url.mjs','node_modules/@better-auth/core/dist/oauth2/utils.mjs','node_modules/better-auth/dist/plugins/generic-oauth/index.mjs','node_modules/@better-auth/core/package.json','node_modules/better-auth/package.json'];
const hashes={};for(const p of sources)hashes[p]=createHash('sha256').update(await readFile(resolve(root,p))).digest('hex');
const usvUrl=await createAuthorizationURL({id:"directory-fixture",options:{clientId:client},authorizationEndpoint:"https://issuer.example.test/authorize",state:"pre\uD800post",codeVerifier:verifier,scopes:["openid","profile","email"],redirectURI:redirect,nonce});
const usv={stateUnits:[112,114,101,55296,112,111,115,116],url:String(usvUrl)};
const fixture={usv,node:process.version,scope:'Actual pinned core and genericOAuth fake discovery; synthetic data only. Graph observations cover String conversion; native tranche implements encoding only.',hashes,client,state,nonce,verifier,redirect,challenge:new URL(urls[0].url).searchParams.get('code_challenge'),urls,generic,graph};
const target=resolve(root,'fixtures/identity_access/oidc/url-values.v1.json');
const output=JSON.stringify(fixture,null,2)+'\n';
if(process.argv.includes('--check')){
  if(await readFile(target,'utf8')!==output)throw Error('OIDC URL oracle fixture differs');
}else await writeFile(target,output);
console.log(JSON.stringify({node:process.version,urls:urls.length,graph:graph.length,genericMatches:true,fetchCalls:calls}));
