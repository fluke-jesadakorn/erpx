/** Temporary TS differential; remove under retire.marketing.asset-writers. */
import {createHash} from 'node:crypto';
import {assetSchema} from '@erpx/server-marketing/domain/asset';
import {marketingPresentationReferences} from '@erpx/server-marketing/domain/presentation';
import {createCommandApplication} from '@erpx/server-workflow/application/commands';
async function main(){
const cases=[
 {kind:'content',name:' Draft ',configuration:{z:1,a:2},preview:{}},
 {kind:'form',name:'Form',configuration:{consentRequired:true,contentAssetId:'abc'},preview:{}},
 {kind:'form',name:'Form',configuration:{consentRequired:false,mediaPublicationId:'abc'},preview:{}},
 {kind:'content',name:'OK',configuration:{body:'x'.repeat(65540)},preview:{}},
 {kind:'content',name:'A\0B',configuration:{},preview:{}},
 ...[{contentAssetId:1},{contentAssetId:''},{contentVersion:0},{contentVersion:1.2},{mediaPublicationId:'abc'},{contentAssetId:'a',contentVersion:1,consentRequired:true}].map(configuration=>({kind:'form',name:'Form',configuration,preview:{}}))
];
const parsed=cases.map(raw=>{const result=assetSchema.safeParse(raw);if(!result.success)return {schema:false};try{if(result.data.kind==='form')marketingPresentationReferences(result.data.configuration);return {schema:true,presentation:true};}catch(e){return {schema:true,presentation:false,code:(e as {code?:string}).code,message:(e as Error).message};}});
const actor={userId:'user',assignmentId:'assignment',role:'staff',authority:{revision:1,assignmentVersion:2},companyId:'company',branchId:'branch',departmentId:null};
let captured:unknown;
const app=createCommandApplication({repository:{receipt:async()=>({request_hash:'none',result_json:'{}'}),clearAssertions:()=>null},digest:async(value:unknown)=>{captured=value;return createHash('sha256').update(JSON.stringify(value)).digest('hex');},clock:{now:()=>0},identifiers:{next:()=>''}} as never);
try {await app.replayCommand(actor as never,'asset-key','marketing.asset.create','marketing-content-asset-key',assetSchema.parse(cases[0]));}catch{}
process.stdout.write(JSON.stringify({cases:parsed,fingerprint:JSON.stringify(captured),hash:createHash('sha256').update(JSON.stringify(captured)).digest('hex')}));

}
main().catch(()=>{console.error('MARKETING_ASSET_ORACLE_FAILED');process.exitCode=1;});
