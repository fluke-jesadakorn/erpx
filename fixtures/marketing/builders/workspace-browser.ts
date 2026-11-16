import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import type {FileStore} from '@erpx/server-runtime/public/runtime/types';
import {seedExpenseCompany} from '../../expenses/company';
import {createMarketingAsset,changeMarketing} from '@erpx/server-marketing/public/marketing';

/** One independently reviewed consent form; no creative/provider prerequisites. */
export async function seedWorkspaceForms(input:{run:string;directory:string;db:PostgresDatabase;files:FileStore}){
 const url=new URL(String(input.db.pool.options.connectionString));
 if(!/^[a-f0-9]{12}$/.test(input.run)||!input.directory.endsWith(`/.runtime/browser/${input.run}`)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${input.run}_h001_db`)throw Error('EXACT_FORMS_WORKSPACE_FIXTURE_REQUIRED');
 const f=await seedExpenseCompany(input.db,input.files),author=await f.member('workspace-form-author','holding_owner'),reviewer=await f.member('workspace-form-reviewer','holding_owner');
 for(const actor of [author,reviewer]){actor.modules=['marketing'];await input.db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(actor.modules),actor.assignmentId).run();}
 const form=await createMarketingAsset(author,{kind:'form',name:'Fictional workspace inquiry',configuration:{consentRequired:true},preview:{headline:'A declared fictional workspace inquiry',summary:'Development smoke only; no outreach or provider.'}},crypto.randomUUID());
 await changeMarketing(reviewer,'asset',String(form.id),{action:'activate',expectedVersion:1,reason:'Independent review of a fictional consent form for the development workspace.'},crypto.randomUUID());
 await writeFile(join(input.directory,'workspace-form.json'),JSON.stringify({run:input.run,id:String(form.id),version:2,companyId:f.company})+'\n',{mode:0o600});
}
