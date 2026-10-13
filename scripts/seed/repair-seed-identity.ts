import { getLocalProxy } from "../local/fixtures";
import { seedWorker } from "./seed-roster";

// Local-only reconciliation of known generated fixtures. Snapshots retain the old rows.
const platform = await getLocalProxy({configPath:"wrangler.jsonc",persist:true,remoteBindings:false});
try {
  if(platform.env.ERPX_ENVIRONMENT !== "local") throw new Error("Identity fixture repair is restricted to local development.");
  for(const [holding,db,files,companies] of [["h001",platform.env.H001_DB,platform.env.H001_FILES,["acme-co","beacon-co"]],["h002",platform.env.H002_DB,platform.env.H002_FILES,["northstar-co"]]] as const) {
    if(!await db.prepare("SELECT 1 FROM seed_repairs WHERE repair_id='employee-links-v2'").first()) {
      const snapshot=await db.prepare("SELECT * FROM employees WHERE id LIKE 'seed-emp-%'").all();
      await db.batch([db.prepare("INSERT INTO seed_repairs VALUES('employee-links-v2',?,?)").bind(JSON.stringify(snapshot.results),Date.now()),db.prepare("UPDATE employees SET user_id=NULL WHERE id LIKE 'seed-emp-%'")]);
    }
    const legacy=await db.prepare("SELECT * FROM hr_people WHERE id='person-emp_mali' AND user_id='usr_mali' AND created_by='system'").first();
    if(legacy&&holding==="h001") await db.batch([
      db.prepare("INSERT INTO seed_repairs VALUES(?,?,?) ON CONFLICT DO NOTHING").bind("identity-v2-legacy-mali",JSON.stringify(legacy),Date.now()),
      db.prepare("UPDATE hr_people SET user_id=NULL,status='archived',merged_into_id='hr-acme-co-001' WHERE id='person-emp_mali' AND created_by='system'"),
      db.prepare("UPDATE hr_engagements SET status='ended' WHERE person_id='person-emp_mali' AND created_by='system'"),
    ]);
    if(holding==="h001"){
      const legacyIds=["asg_nisa","asg_nisa_owner","asg_nisa_employee","asg_nisa_finance","asg_nisa_department_manager","asg_mali","asg_mali_finance","asg_mali_department_manager","asg_mali_people_manager"];
      const snapshot=await db.prepare(`SELECT * FROM assignments WHERE id IN (${legacyIds.map(()=>"?").join(",")})`).bind(...legacyIds).all();
      await db.batch([db.prepare("INSERT INTO seed_repairs VALUES('legacy-role-cleanup-v1',?,?) ON CONFLICT DO NOTHING").bind(JSON.stringify(snapshot.results),Date.now()),db.prepare(`UPDATE assignments SET revoked_at=?,version=version+1 WHERE id IN (${legacyIds.map(()=>"?").join(",")}) AND revoked_at IS NULL`).bind(Date.now(),...legacyIds)]);
    }
    let ordinal=0;
    for(const company of companies) for(let index=0;index<20;index++) {
      ordinal++; const worker=seedWorker(company,index); const personId=`hr-${company}-${String(index+1).padStart(3,"0")}`;
      const engagementId=`eng-${company}-${String(index+1).padStart(3,"0")}`;
      const employeeId=company==="acme-co"&&index===0?"emp_mali":`seed-emp-${String(ordinal).padStart(3,"0")}`;
      const repair=`identity-v2-${personId}`;
      if(await db.prepare("SELECT 1 FROM seed_repairs WHERE repair_id=?").bind(repair).first()){await db.prepare("UPDATE employees SET user_id=? WHERE id=? AND person_id=?").bind(worker.userId,employeeId,personId).run();continue;}
      const person=await db.prepare("SELECT * FROM hr_people WHERE id=? AND version=1").bind(personId).first();
      const engagement=await db.prepare("SELECT * FROM hr_engagements WHERE id=? AND version=1").bind(engagementId).first<Record<string,string|number|null>>();
      const employee=await db.prepare("SELECT * FROM employees WHERE id=?").bind(employeeId).first();
      if(!person||!engagement) { console.log(`Preserved edited or missing fixture ${personId}`);continue; }
      const country=engagement.branch_id==="bangkok"?"TH":engagement.branch_id==="singapore"?"SG":engagement.branch_id==="london"?"GB":engagement.branch_id==="rotterdam"?"NL":engagement.branch_id==="toronto"?"CA":"US";
      await db.batch([
        db.prepare("INSERT INTO seed_repairs VALUES(?,?,?)").bind(repair,JSON.stringify({person,engagement,employee}),Date.now()),
        db.prepare("UPDATE hr_people SET user_id=?,legal_name=?,preferred_name=NULL,email=?,phone=NULL,country_code=? WHERE id=? AND version=1").bind(worker.userId,worker.name,worker.email,country,personId),
        db.prepare("UPDATE hr_engagements SET job_title=?,worker_type=?,employment_type=? WHERE id=? AND version=1").bind(worker.jobTitle,worker.userId?"employee":index%5===0?"contractor":"employee",worker.userId?"salaried":index%5===0?"contractor":"salaried",engagementId),
        db.prepare("UPDATE employees SET person_id=?,user_id=?,display_name=?,company_id=?,branch_id=?,department_id=? WHERE id=?").bind(personId,worker.userId,worker.name,company,engagement.branch_id,engagement.department_id,employeeId),
        db.prepare("UPDATE leave_requests SET company_id=?,branch_id=?,department_id=? WHERE employee_id=? AND id LIKE 'seed-%'").bind(company,engagement.branch_id,engagement.department_id,employeeId),
        db.prepare("UPDATE hr_payment_batch_items SET payee_name=? WHERE person_id=? AND id LIKE 'payment-item-%'").bind(worker.name,personId),
      ]);
    }
    await db.prepare("UPDATE hr_country_packs SET status='testing',approval_evidence='Development examples; statutory validation pending',validated_by=NULL WHERE id LIKE 'pack-%' AND rules_json LIKE '%synthetic demo rules%'").run();
    for(const company of companies){
      const originals=await db.prepare("SELECT * FROM finance_parties WHERE company_id=? AND name IN ('Demo Customer','Demo Supplier')").bind(company).all();
      if(originals.results.length)await db.batch([db.prepare("INSERT INTO seed_repairs VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(`partner-names-${company}`,JSON.stringify(originals.results),Date.now()),db.prepare("UPDATE finance_parties SET name=CASE WHEN name='Demo Customer' THEN 'Aster Workplace Services' ELSE 'Meridian Contract Supply' END WHERE company_id=? AND name IN ('Demo Customer','Demo Supplier')").bind(company)]);
    }
    const invalidMatches=await db.prepare("SELECT * FROM finance_bank_transactions WHERE id LIKE 'banktx-%' AND matched_type='finance_payment'").all();
    if(invalidMatches.results.length)await db.batch([
      db.prepare("INSERT INTO seed_repairs VALUES('unsupported-bank-fixture-matches-v1',?,?) ON CONFLICT DO NOTHING").bind(JSON.stringify(invalidMatches.results),Date.now()),
      db.prepare("UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL WHERE id LIKE 'banktx-%' AND matched_type='finance_payment'"),
      db.prepare("UPDATE finance_bank_statements SET status='matching' WHERE id LIKE 'statement-%' AND EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.statement_id=finance_bank_statements.id AND t.match_status!='matched')"),
    ]);
    // Preserve the original generated export before reconciling its fixture payee names.
    const batches=await db.prepare("SELECT id,file_key,file_checksum FROM hr_payment_batches WHERE id LIKE 'payment-batch-%' AND file_key IS NOT NULL").all<{id:string;file_key:string;file_checksum:string}>();
    for(const batch of batches.results){
      const original=await files.get(batch.file_key);if(!original)continue;
      const previousBody=await original.text(),archive=`${holding}/seed-archive/${batch.id}-${batch.file_checksum}.csv`;
      const items=await db.prepare("SELECT reference,payee_name,amount_minor,currency,status FROM hr_payment_batch_items WHERE batch_id=? ORDER BY reference").bind(batch.id).all<{reference:string;payee_name:string;amount_minor:number;currency:string;status:string}>();
      const csv=["reference,payee,amount,currency,status",...items.results.map(i=>[i.reference,i.payee_name,(i.amount_minor/100).toFixed(2),i.currency,i.status].join(","))].join("\n");
      if(csv===previousBody)continue;
      if(!await files.head(archive))await files.put(archive,previousBody,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:batch.file_checksum}});
      const checksum=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(csv))),b=>b.toString(16).padStart(2,"0")).join("");
      await files.put(batch.file_key,csv,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:checksum,format:"generic_csv"}});
      await db.prepare("UPDATE hr_payment_batches SET file_checksum=? WHERE id=?").bind(checksum,batch.id).run();
    }
    const duplicates=await db.prepare("SELECT legal_name,COUNT(*) count FROM hr_people WHERE status!='archived' AND merged_into_id IS NULL GROUP BY legal_name HAVING COUNT(*)>1").all();
    console.log(holding,{duplicates:duplicates.results,linked:(await db.prepare("SELECT COUNT(*) count FROM hr_people WHERE user_id IS NOT NULL").first())});
  }
  await platform.env.CONTROL_DB.prepare("UPDATE platform_assignments SET revoked_at=?,version=version+1 WHERE (id LIKE 'platform_usr_%' OR id='asg_nisa_platform') AND user_id NOT IN ('usr_mali','usr_jesadakorn') AND revoked_at IS NULL").bind(Date.now()).run();
  console.log("Known local fixture identities reconciled; old rows retained in seed_repairs.");
} finally {await platform.dispose();}
