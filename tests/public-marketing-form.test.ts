import {beforeAll,expect,it,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {createMarketingAsset,changeMarketing} from '../src/server/marketing';
import {GET,POST} from '../src/app/api/public/marketing/[holdingSlug]/[formId]/route';

vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),holdingBindingBySlug:async()=>({id:'H001'}),runtimeEnv:async()=>env}));
vi.mock('../src/server/hospitality-rollout',()=>({assertProductionModuleEnabled:async()=>{}}));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});

it('accepts a consented public form once and binds its retry key to the form',async()=>{
 const f=await expenseFixture(),creator={...f.owner,modules:['marketing']},reviewer={...f.tie,modules:['marketing'],role:'holding_owner',assignmentId:await f.grant(f.tie,'holding_owner')};
 async function activeForm(){const form=await createMarketingAsset(creator,{kind:'form',name:'Public inquiry',configuration:{consentRequired:true},preview:{headline:'Contact our hotel'}},key());await changeMarketing(reviewer,'asset',form.id,{action:'activate',expectedVersion:1,reason:'Reviewed form consent and preview'},key());return form.id;}
 const formId=await activeForm(),otherFormId=await activeForm(),context=(id:string)=>({params:Promise.resolve({holdingSlug:'hotel',formId:id})});
 const page=await GET(new Request(`http://localhost:3100/api/public/marketing/hotel/${formId}`),context(formId));expect(page.status).toBe(200);
 const body={contactName:'Test guest',contactDetail:'Call the test number',brief:'Room inquiry',consentAccepted:true,source:'hotel site'};
 const request=(id:string,origin='http://localhost:3100')=>new Request(`http://localhost:3100/api/public/marketing/hotel/${id}`,{method:'POST',headers:{host:'localhost:3100',origin,'Idempotency-Key':'public-test-'+formId,'Content-Type':'application/json'},body:JSON.stringify(body)});
 expect((await POST(request(formId,'https://unrelated.example'),context(formId))).status).toBe(403);
 const first=await POST(request(formId),context(formId));expect(first.status).toBe(201);
 const result=await first.json();expect(result.data.status).toBe('pending');
 const retry=await POST(request(formId),context(formId));expect((await retry.json()).data.id).toBe(result.data.id);
 expect((await POST(request(otherFormId),context(otherFormId))).status).toBe(409);
 expect(await db.prepare("SELECT count(*) n FROM marketing_lead_candidates WHERE id=? AND status='pending'").bind(result.data.id).first('n')).toBe(1);
});
