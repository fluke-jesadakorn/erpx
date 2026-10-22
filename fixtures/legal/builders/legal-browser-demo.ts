/** Synthetic Legal personas and evidence for one exact disposable browser run. */
import { randomBytes } from 'node:crypto';
import { mkdir, writeFile, chmod } from 'node:fs/promises';
import { join } from 'node:path';
import { hashPassword } from 'better-auth/crypto';
import { PDFDocument, StandardFonts } from 'pdf-lib';
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import type { FileStore } from "@erpx/server-runtime/adapters/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { seedExpenseCompany } from "../../expenses/company";
import { businessRoles } from "@erpx/feature-identity_access/lib/business-roles";
import { legalCommand, detail } from "@erpx/server-legal/public/legal/service";
import { lawCorpusCommand } from "@erpx/server-legal/public/legal/law-corpus";
import { uploadDocument } from "@erpx/server-legal/public/legal/documents";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";

type Input = { run:string; directory:string; control:PostgresDatabase; h001:PostgresDatabase; h002:PostgresDatabase; h001Files:FileStore; h002Files:FileStore };
export type LegalUatUser = { key:string; name:string; email:string; userId:string; role:string; companyId:string; holdingSlug:string; assignmentId:string; workspaceBase:string; scope:string; scopeNodeId:string|null };
const commandKey = () => crypto.randomUUID();
async function privateJson(path:string,value:unknown) {
  await writeFile(path,JSON.stringify(value,null,2)+'\n',{mode:0o600}); await chmod(path,0o600);
}
function exactDatabase(db:PostgresDatabase,run:string,binding:string) {
  const connection = new URL(String(db.pool.options.connectionString));
  if (connection.hostname!=='127.0.0.1' || connection.port!=='5432' || connection.username!=='erpx_test_admin' ||
      connection.pathname!==`/erpx_browser_${run}_${binding.toLowerCase()}`) throw new Error('EXACT_LEGAL_UAT_DATABASE_REQUIRED');
}
export async function seedLegalBrowserDemo(input:Input) {
  if (!/^[a-f0-9]{12}$/.test(input.run)) throw new Error('EXACT_LEGAL_UAT_RUN_REQUIRED');
  exactDatabase(input.control,input.run,'CONTROL_DB'); exactDatabase(input.h001,input.run,'H001_DB'); exactDatabase(input.h002,input.run,'H002_DB');
  const password = `Legal-UAT-${randomBytes(18).toString('hex')}`, passwordHash = await hashPassword(password), now=Date.now();
  const users:LegalUatUser[]=[];
  const companies:Record<string,{id:string;holdingSlug:string;branchIds:string[];departmentIds:string[]}>={};
  const actors:Record<string,WorkContext>={};
  const seeded:Record<string,Record<string,string>>={};
  const alternateAssignments:Record<string,{id:string;companyId:string;scopeKind:string;scopeNodeId:string}>={};
  for (const [letter,db,files,holdingId,holdingSlug] of [
    ['A',input.h001,input.h001Files,'h001','simulation'],
    ['B',input.h001,input.h001Files,'h001','simulation'],
    ['C',input.h002,input.h002Files,'h002','riverside-lantern'],
  ] as const) {
    const f=await seedExpenseCompany(db,files);
    await db.prepare('UPDATE organization_nodes SET display_name=? WHERE id=?').bind(`Legal UAT Company ${letter} · synthetic only`,f.company).run();
    await db.prepare('UPDATE organization_nodes SET display_name=? WHERE id=?').bind(`Legal ${letter} branch 1`,f.branch).run();
    await db.prepare('UPDATE organization_nodes SET display_name=? WHERE id=?').bind(`Legal ${letter} operations`,f.department).run();
    await db.prepare('UPDATE organization_nodes SET display_name=? WHERE id=?').bind(`Legal ${letter} privacy`,f.accountsDepartment).run();
    const secondBranch=`${f.company}-legal-branch-2`,secondDepartment=`${f.company}-legal-team-2`;
    await db.batch([
      db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Legal second branch',?,?,?)").bind(secondBranch,secondBranch,f.company,f.company,secondBranch),
      db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Legal second team',?,?,?)").bind(secondDepartment,secondDepartment,secondBranch,f.company,secondBranch),
    ]);
    companies[letter]={id:f.company,holdingSlug,branchIds:[f.branch,secondBranch],departmentIds:[f.department,f.accountsDepartment,secondDepartment]};
    const register=async(key:string,role:keyof typeof businessRoles|'holding_owner',scope='holding',node:string|null=null)=>{
      const actor=await f.member(`legal-${letter}-${key}`,role);
      actor.assignmentId=await f.grant(actor,role,scope,node); actor.role=role;
      actor.holdingId=holdingId; actor.holdingSlug=holdingSlug; actor.holdingName=`Legal Holding ${holdingId==='h001'?'A':'B'}`;
      const scopedBranch=scope==='department'?(node===secondDepartment?secondBranch:f.branch):scope==='branch'?node:null;
      actor.branchId=scopedBranch;actor.departmentId=scope==='department'?node:null;
      actor.scope=scope==='department'?{kind:'department',holdingId,holdingSlug,holdingName:actor.holdingName,companyId:f.company,branchId:scopedBranch!,branchSlug:scopedBranch!,branchName:`Legal ${letter} branch`,departmentId:node!,departmentSlug:node!,departmentName:`Legal ${letter} department`}:scope==='branch'?{kind:'branch',holdingId,holdingSlug,holdingName:actor.holdingName,companyId:f.company,branchId:node!,branchSlug:node!,branchName:`Legal ${letter} branch`}:{kind:'holding',holdingId,holdingSlug,holdingName:actor.holdingName,companyId:f.company};
      actor.name=`${letter} ${key.replaceAll('-',' ')}`; actor.email=`legal-${letter.toLowerCase()}-${key}@simulation.test`; actor.modules=role==='holding_owner'?['legal','organization','identity_access','collaboration']:[...businessRoles[role]];
      await db.batch([
        db.prepare('UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=? AND id<>?').bind(now,f.company,actor.userId,actor.assignmentId),
        db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(actor.modules),actor.assignmentId),
        db.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(actor.name,actor.name,actor.email,f.company,actor.userId),
        db.prepare('UPDATE hr_engagements SET branch_id=?,department_id=? WHERE company_id=? AND person_id=?').bind(scopedBranch??f.branch,scope==='department'?node:scope==='branch'&&node===secondBranch?secondDepartment:f.department,f.company,`person-${actor.userId}`),
      ]);
      await input.control.batch([
        input.control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,actor.name,actor.email,now,now),
        input.control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,passwordHash,now,now),
        input.control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,actor.name,now),
      ]);
      actors[`${letter}:${key}`]=actor;
      users.push({key:`${letter}:${key}`,name:actor.name,email:actor.email,userId:actor.userId,role,companyId:f.company,holdingSlug,assignmentId:actor.assignmentId,workspaceBase:workspaceBase(actor.scope),scope,scopeNodeId:node});
      return actor;
    };
    const manager=await register('manager','legal_manager');
    if(letter==='A'){
      const alternate=await f.grant(manager,'legal_manager','branch',f.branch);
      await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(businessRoles.legal_manager),alternate).run();
      alternateAssignments.A={id:alternate,companyId:f.company,scopeKind:'branch',scopeNodeId:f.branch};
    }
    await register('policy-reviewer','legal_manager');
    await register('finance','finance_approver');
    const reviewer1=await register('reviewer-1','legal_counsel'),reviewer2=await register('reviewer-2','legal_counsel');
    const requester=await register('requester','legal_requester','branch',f.branch);
    await register('other-branch','legal_counsel','branch',secondBranch);
    await register('department-requester','legal_requester','department',f.department);
    await register('other-department','legal_counsel','department',f.accountsDepartment);
    const revoked=await register('revoked-counsel','legal_counsel');
    await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(now,revoked.assignmentId).run();
    await register('outsider','employee');
    await register('owner','holding_owner');
    await legalCommand(db,manager,{action:'settings',enabled:true,aiEnabled:false},commandKey());
    const create=async(kind:string,title:string,data:Record<string,unknown>,who=manager)=>String((await legalCommand(db,who,{action:'create',item:{kind,title,description:'Synthetic rehearsal only. No real legal conclusion or certification.',classification:'confidential',ownerId:who.userId,data}},commandKey())).id);
    const act=async(id:string,action:string,extra:Record<string,unknown>={})=>legalCommand(db,manager,{action,id,expectedVersion:(await detail(db,manager,id)).item.version,...extra},commandKey());
    const contract=await create('contract',`${letter} UAT supply agreement`,{contractType:'Synthetic supply',parties:`Company ${letter} and Synthetic Supplier`});
    await act(contract,'document',{name:'Approval draft',body:'Synthetic agreement: deliver ten training items within thirty days. No real transaction or legal advice.'});
    const approvedDocumentId=(await detail(db,manager,contract)).documents[0].id;
    await act(contract,'document',{name:'Signatory authority',body:'Synthetic authority evidence: A training signer may sign this fictional agreement only.'});
    const authorityDocumentId=(await detail(db,manager,contract)).documents.find(document=>document.name==='Signatory authority')!.id;
    for(const reviewer of [reviewer1,reviewer2])await act(contract,'member',{userId:reviewer.userId,access:'read'});
    const request=await create('request',`${letter} private requester question`,{requestType:'Synthetic advice',question:'กรุณาตรวจข้อกำหนดในสัญญาสำหรับการซ้อมเท่านั้น'},requester);
    const obligation=await create('obligation',`${letter} UAT due obligation`,{obligationType:'Synthetic delivery',requirement:'Training completion evidence',escalationOwner:reviewer1.userId});
    await act(obligation,'member',{userId:reviewer1.userId,access:'read'});
    const rule=await create('rule',`${letter} unverified Thai law fixture`,{source:'https://ratchakitcha.soc.go.th/legal-uat-fixture.pdf',applicability:'Synthetic applicability awaiting an independent expert.',procedure:'No real deadline computation.'});
    await act(rule,'document',{name:'Synthetic law original (unverified)',body:'ข้อความสมมติสำหรับการทดสอบเท่านั้น ห้ามใช้เป็นกฎหมายหรือคำแนะนำทางกฎหมาย'});
    const sourceDocumentId=(await detail(db,manager,rule)).documents[0].id;
    await act(rule,'member',{userId:reviewer1.userId,access:'read'});
    const source=await lawCorpusCommand(db,manager,{action:'law.source.register',id:rule,expectedVersion:(await detail(db,manager,rule)).item.version,reason:'Register a synthetic official-source outage rehearsal',title:'Synthetic fixture URL; no real law evidence',sourceUrl:'https://ratchakitcha.soc.go.th/legal-uat-fixture.pdf'},commandKey());
    const sourceId=String(source.evidenceId);
    const revision=await lawCorpusCommand(db,manager,{action:'law.revision.import',id:rule,expectedVersion:(await detail(db,manager,rule)).item.version,reason:'Synthetic draft remains unapproved',sourceId,documentId:sourceDocumentId,extractionMethod:'manual',revisionLabel:'SYNTHETIC-PENDING',publishedOn:'2026-09-01',effectiveFrom:'2026-09-01',chunks:[{page:1,section:'Synthetic section',text:'ข้อความสมมติสำหรับการทดสอบเท่านั้น ห้ามใช้เป็นกฎหมายหรือคำแนะนำทางกฎหมาย'}]},commandKey());
    seeded[letter]={contract,approvedDocumentId,authorityDocumentId,request,obligation,rule,sourceId,revisionId:String(revision.evidenceId),sourceDocumentId};
  }
  // One central specialist can switch explicitly between Company A and B.
  const central=actors['A:manager'],companyB=companies.B;
  const grant=commandKey();
  await input.h001.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'legal_manager',?, ?,0,9999999999999,'holding')")
    .bind(grant,central.userId,JSON.stringify(businessRoles.legal_manager),companyB.id).run();
  // Company B records remain confidential: central assignment does not grant membership.
  await mkdir(join(input.directory,'legal-files'),{recursive:true,mode:0o700});
  const pdf=await PDFDocument.create(),font=await pdf.embedFont(StandardFonts.Helvetica),page=pdf.addPage([595,842]);
  page.drawText('SYNTHETIC SIGNED ORIGINAL - UAT ONLY',{x:40,y:790,size:16,font});
  page.drawText('Deliver ten training items within thirty days.',{x:40,y:740,size:12,font});
  page.drawText('Company A / Synthetic Supplier / Training signer',{x:40,y:690,size:12,font});
  page.drawText('Signed 2026-09-30. No real agreement or legal certification.',{x:40,y:640,size:10,font});
  const signedPdfPath=join(input.directory,'legal-files','signed-original.pdf');
  await writeFile(signedPdfPath,await pdf.save(),{mode:0o600});
  // Historical synthetic records exercise elapsed retention without reducing
  // an operational policy or guessing a statutory retention period.
  for(const [letter,db,files] of [['A',input.h001,input.h001Files],['B',input.h001,input.h001Files],['C',input.h002,input.h002Files]] as const){
    const manager=actors[`${letter}:manager`],reviewer=actors[`${letter}:policy-reviewer`],realNow=Date.now;
    try{
      Date.now=()=>now-3*86_400_000;
      const created=await legalCommand(db,manager,{action:'create',item:{kind:'request',title:`${letter} historical disposable training record`,classification:'confidential',ownerId:manager.userId,data:{requestType:'Synthetic historical fixture',question:'Disposable training evidence only; no statutory retention rule'}}},commandKey());
      const act=async(who:WorkContext,action:string,extra:Record<string,unknown>={})=>legalCommand(db,who,{action,id:created.id,expectedVersion:(await detail(db,who,created.id)).item.version,...extra},commandKey());
      await act(manager,'member',{userId:reviewer.userId,access:'edit'});
      await uploadDocument(db,files,manager,created.id,(await detail(db,manager,created.id)).item.version,'Historical disposable original.pdf','application/pdf',await pdf.save(),commandKey());
      for(const state of ['review','approval'])await act(manager,'transition',{state,reason:'Historical synthetic fixture review'});
      await act(reviewer,'transition',{state:'approved',reason:'Historical independent fixture approval'});
      for(const state of ['active','completed'])await act(manager,'transition',{state,reason:'Historical synthetic completion evidence'});
      seeded[letter].retention=created.id;seeded[letter].retentionDocumentId=(await detail(db,manager,created.id)).documents[0].id;
    }finally{Date.now=realNow;}
  }
  const state={companies,records:seeded,alternateAssignments,signedPdfPath,notice:'Synthetic fixtures only. All law content, applicability and production release acceptance remain pending. No legal expert has certified this run.'};
  await privateJson(join(input.directory,'legal-credentials.json'),{password,users,run:input.run});
  await privateJson(join(input.directory,'legal-guide.json'),state);
  return state;
}
