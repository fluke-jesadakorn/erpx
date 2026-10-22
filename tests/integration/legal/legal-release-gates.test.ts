import { beforeAll, expect, it, vi } from 'vitest';
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import { legalProductionScope, legalReleaseCriteria, productionScope, releaseCriteria } from "@erpx/catalogue/production-scope";
import { companyReleaseStatus, currentReleaseInputs, submitReleaseAcceptance, reviewReleaseAcceptance } from "@erpx/server-platform_administration/public/release-acceptance";
import { acceptedReleasePredicate } from "@erpx/server-platform_administration/public/release-predicate";
import { changeCompanyFeatureRollout } from "@erpx/server-pms/public/hospitality-rollout";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { legalCompanyCapabilityReady } from "@erpx/server-legal/public/legal/runtime-gate";
import { legalReminders, drainLegalAi } from "@erpx/server-legal/public/legal/jobs";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { mkdtemp, mkdir, readdir, symlink, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { migrateDatabase } from "@erpx/server-runtime/adapters/runtime/migrations";

vi.mock("@erpx/catalogue/release-artifact.json", () => ({ default: { softwareVersion: 'legal-release-fixture' } }));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB }), runtimeEnv: async () => env }));
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });

async function fixture() {
  const f = await expenseFixture();
  const reviewer: WorkContext = { ...f.head, role:'holding_owner', branchId:null, departmentId:null,
    assignmentId:await f.grant(f.head,'holding_owner'), scope:f.owner.scope, modules:f.owner.modules };
  await db.prepare(`INSERT INTO company_business_acceptance(company_id,status,version,requested_by,reviewed_by,requested_at,reviewed_at,reason,review_reason,updated_at)
    VALUES(?,'accepted',2,?,?,1,2,'Synthetic setup','Independent synthetic setup',2)`).bind(f.company,f.owner.userId,reviewer.userId).run();
  await db.prepare('INSERT INTO legal_settings(company_id,enabled,ai_enabled) VALUES(?,true,true)').bind(f.company).run();
  return { ...f, reviewer };
}
async function submission(f: Awaited<ReturnType<typeof fixture>>, legal = true) {
  const scope = legal ? legalProductionScope.version : productionScope.version;
  const inputs = await currentReleaseInputs(db,f.company,scope);
  return { scopeVersion:scope, softwareVersion:inputs.softwareVersion,
    expectedConfigurationRevision:inputs.configurationRevision,expectedSetupVersion:inputs.setupVersion,
    reason:'Synthetic rehearsal only; no real legal acceptance',
    evidence:(legal ? legalReleaseCriteria : releaseCriteria).map(row => ({criterionId:row.id,
      summary:'Independent simulated test evidence',files:[{reference:`fixture:${row.id}`,sha256:'a'.repeat(64)}]})) };
}
async function accept(f: Awaited<ReturnType<typeof fixture>>) {
  const release = await submitReleaseAcceptance(db,f.owner,await submission(f),key());
  let version = 1;
  for (const criterion of legalReleaseCriteria) await reviewReleaseAcceptance(db,f.reviewer,String(release.id),
    {expectedVersion:version++,criterionId:criterion.id,state:'passed',reason:'Synthetic independent review only'},key());
  return release;
}

it('accepts Legal across branches without changing or superseding the hotel cut',async () => {
  const f = await fixture();
  const hotel = await submitReleaseAcceptance(db,f.owner,await submission(f,false),key());
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'branch',?,'Second branch',?)")
    .bind(key(),`second-${key()}`,f.company).run();
  await accept(f);
  expect((await companyReleaseStatus(db,f.company,undefined,legalProductionScope.version)).state).toBe('passed');
  expect((await companyReleaseStatus(db,f.company)).release?.id).toBe(hotel.id);
  expect((await companyReleaseStatus(db,f.company)).blockers).not.toContain('A newer release superseded this acceptance.');
  expect(await db.prepare(`SELECT ${acceptedReleasePredicate('?',legalProductionScope.version)} accepted`).bind(f.company).first()).toEqual({accepted:true});
});

it('requires named independent legal review and invalidates it when rules, policy or reviewer authority change',async () => {
  const f = await fixture();
  const release = await submitReleaseAcceptance(db,f.owner,await submission(f),key());
  await expect(reviewReleaseAcceptance(db,f.owner,String(release.id),{expectedVersion:1,criterionId:'legal_content',state:'passed',reason:'Cannot certify own evidence'},key()))
    .rejects.toMatchObject({code:'INDEPENDENT_REVIEW_REQUIRED'});
  await expect(reviewReleaseAcceptance(db,f.reviewer,String(release.id),{expectedVersion:1,criterionId:'hotel',state:'passed',reason:'Wrong release scope'},key()))
    .rejects.toMatchObject({code:'RELEASE_CRITERION_SCOPE'});
  await accept(f);
  await db.prepare('UPDATE legal_settings SET reminder_days=?::jsonb WHERE company_id=?').bind('[30,7,1]',f.company).run();
  expect((await companyReleaseStatus(db,f.company,undefined,legalProductionScope.version)).blockers).toContain('Material Company configuration changed.');
  await accept(f);
  await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.reviewer.assignmentId).run();
  expect((await companyReleaseStatus(db,f.company,undefined,legalProductionScope.version)).gates.every(gate => gate.state==='pending')).toBe(true);
});

it('gates production claims per Company and keeps paused AI queued without provider requests',async () => {
  const f = await fixture(), paused = await fixture();
  await accept(f);
  vi.stubEnv('NODE_ENV','production');
  try {
    await expect(changeCompanyFeatureRollout(paused.owner,{moduleId:'legal',expectedVersion:0,state:'pilot',reason:'Unaccepted Legal'},key())).rejects.toMatchObject({code:'FEATURE_ACCEPTANCE_REQUIRED'});
    await changeCompanyFeatureRollout(f.owner,{moduleId:'legal',expectedVersion:0,state:'available',reason:'Accepted synthetic Company'},key());
    await expect(assertProductionModuleEnabled(db,f.company,'legal')).resolves.toBeUndefined();
    expect(await legalCompanyCapabilityReady(db,f.company,'ai')).toBe(true);
    expect(await legalCompanyCapabilityReady(db,paused.company,'operations')).toBe(false);
    for (const company of [f,paused]) {
      const id = `legal-${key()}`;
      await db.prepare(`INSERT INTO legal_items(id,company_id,kind,title,state,classification,owner_id,due_date,country,jurisdiction,language,currency,timezone,priority,created_by,created_at,updated_at)
        VALUES(?,?,'obligation','Synthetic due task','active','confidential',?,'2026-01-01','TH','Thailand','th','THB','Asia/Bangkok','normal',?,1,1)`).bind(id,company.company,company.owner.userId,company.owner.userId).run();
      await db.prepare("INSERT INTO legal_members(item_id,user_id,access) VALUES(?,?,'edit')").bind(id,company.owner.userId).run();
      if (company===paused) await db.prepare(`INSERT INTO legal_ai_proposals(id,item_id,source_version,source_hash,question,state,created_by,created_at,actor_json)
        VALUES(?,?,1,?,'Synthetic AI question','queued',?,1,?)`).bind(`ai-${id}`,id,'a'.repeat(64),company.owner.userId,JSON.stringify(company.owner)).run();
    }
    await legalReminders(db); await legalReminders(db);
    expect(await db.prepare("SELECT count(*) count FROM staff_notifications WHERE company_id=? AND record_type='legal'").bind(f.company).first()).toEqual({count:1});
    expect(await db.prepare("SELECT count(*) count FROM staff_notifications WHERE company_id=? AND record_type='legal'").bind(paused.company).first()).toEqual({count:0});
    const generate = vi.fn(); await drainLegalAi(db,{},generate);
    expect(generate).not.toHaveBeenCalled();
    expect(await db.prepare("SELECT p.state FROM legal_ai_proposals p JOIN legal_items i ON i.id=p.item_id WHERE i.company_id=?").bind(paused.company).first()).toEqual({state:'queued'});
  } finally { vi.unstubAllEnvs(); }
});

it.each(['assignment_version','revoked_session'] as const)('rejects queued Legal AI after %s changes before sending provider data',async change => {
  const f=await fixture(), sessionId=key(), itemId=`legal-${key()}`, jobId=key();
  const version=Number((await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.owner.assignmentId)
    .first<{version:number}>())!.version);
  const actor:WorkContext={...f.owner,authority:{sessionId,revision:1,assignmentVersion:version}};
  await db.prepare(`INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)
    VALUES(?,?,?,?,?,1,?,?)`).bind(sessionId,actor.userId,f.company,actor.assignmentId,version,Date.now()+60_000,Date.now()).run();
  await db.prepare(`INSERT INTO legal_items(id,company_id,kind,title,state,classification,owner_id,country,jurisdiction,language,currency,timezone,priority,created_by,created_at,updated_at)
    VALUES(?,?,'matter','Queued authorization rehearsal','active','confidential',?,'TH','Thailand','th','THB','Asia/Bangkok','normal',?,1,1)`)
    .bind(itemId,f.company,actor.userId,actor.userId).run();
  await db.prepare(`INSERT INTO legal_ai_proposals(id,item_id,source_version,source_hash,question,state,created_by,created_at,actor_json)
    VALUES(?,?,1,?,'Synthetic queued question','queued',?,-1,?)`).bind(jobId,itemId,'a'.repeat(64),actor.userId,JSON.stringify(actor)).run();
  if(change==='assignment_version')await db.prepare('UPDATE assignments SET version=version+1 WHERE id=?').bind(actor.assignmentId).run();
  else await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)')
    .bind(sessionId,f.company,actor.userId,Date.now()).run();
  const generate=vi.fn();
  await drainLegalAi(db,{},generate);
  expect(generate).not.toHaveBeenCalled();
  expect(await db.prepare('SELECT state,error FROM legal_ai_proposals WHERE id=?').bind(jobId).first())
    .toEqual({state:'failed',error:change==='assignment_version'?'WORK_ROLE_UNAVAILABLE':'WORK_CONTEXT_CHANGED'});
});

it('upgrades existing Legal data without enabling a release or replacing immutable originals',async () => {
  const temporary = await mkdtemp(resolve('.runtime/tests/legal-release-upgrade-'));
  const upgraded = env.H002_DB;
  try {
    await mkdir(join(temporary,'holding-write'));
    for (const file of await readdir('migrations/postgres/holding-write')) {
      if (!file.endsWith('.sql') || Number(file.split('_')[0]) >= 915) continue;
      await symlink(resolve('migrations/postgres/holding-write',file),join(temporary,'holding-write',file));
    }
    await migrateDatabase(upgraded,'holding-write',temporary);
    await upgraded.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('legal-upgrade-company','company','legal-upgrade-company','Legal upgrade','legal-upgrade-company')").run();
    await upgraded.prepare("INSERT INTO legal_settings(company_id,enabled,ai_enabled,version) VALUES('legal-upgrade-company',true,true,7)").run();
    await upgraded.prepare(`INSERT INTO legal_items(id,company_id,kind,title,state,classification,owner_id,country,jurisdiction,language,currency,timezone,priority,created_by,created_at,updated_at)
      VALUES('legal-upgrade-contract','legal-upgrade-company','contract','Historical contract','active','confidential','historical-owner','TH','Thailand','th','THB','Asia/Bangkok','normal','historical-owner',1,1)`).run();
    await upgraded.prepare("INSERT INTO legal_contracts(item_id,data) VALUES('legal-upgrade-contract','{}')").run();
    await upgraded.prepare(`INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at)
      VALUES('legal-upgrade-original','legal-upgrade-contract',1,'Original','text/plain',?,10,'Immutable original','historical-owner',1)`).bind('b'.repeat(64)).run();
    await migrateDatabase(upgraded,'holding-write');
    expect(await upgraded.prepare("SELECT enabled,ai_enabled,version FROM legal_settings WHERE company_id='legal-upgrade-company'").first()).toEqual({enabled:true,ai_enabled:true,version:7});
    expect(await upgraded.prepare("SELECT checksum,body,version FROM legal_documents WHERE id='legal-upgrade-original'").first()).toEqual({checksum:'b'.repeat(64),body:'Immutable original',version:1});
    expect((await companyReleaseStatus(upgraded,'legal-upgrade-company',undefined,legalProductionScope.version)).release).toBeNull();
    vi.stubEnv('NODE_ENV','production');
    try { expect(await legalCompanyCapabilityReady(upgraded,'legal-upgrade-company','ai')).toBe(false); }
    finally { vi.unstubAllEnvs(); }
  } finally { await rm(temporary,{recursive:true,force:true}); }
});
