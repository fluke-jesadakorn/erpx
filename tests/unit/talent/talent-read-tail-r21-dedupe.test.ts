import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/attachment-read-sources.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r20 baseline). The people-payroll source SQL
// and the offer-target candidate predicate are stubbed to keep the pinned
// strings owner-local and stable.
const EXPECTED_APPLICATIONS = "SELECT a.id,a.candidate_id,a.company_id,jsonb_build_object('application',to_jsonb(a),'requisition',to_jsonb(r),\n 'proposals',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM talent_proposals p WHERE p.application_id=a.id),\n 'offers',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM talent_offers o WHERE o.application_id=a.id AND o.company_id=a.company_id),\n 'handoffs',(SELECT jsonb_agg(jsonb_build_object('handoff',to_jsonb(h),'receipt',to_jsonb(receipt),'people',hired.image) ORDER BY h.id) FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=h.id AND receipt.company_id=h.company_id LEFT JOIN (__PEOPLE_SRC__) hired ON hired.handoff_id=h.id AND hired.company_id=h.company_id WHERE o.application_id=a.id AND o.company_id=a.company_id)) image\n FROM talent_applications a LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id";
const EXPECTED_RETENTION = "SELECT jsonb_build_object('candidate',to_jsonb(c),\n 'privacy',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM talent_candidate_privacy_events event WHERE event.candidate_id=c.id AND event.company_id=c.company_id),\n 'applications',(SELECT jsonb_agg(a.image ORDER BY a.id) FROM (SELECT a.id,a.candidate_id,a.company_id,jsonb_build_object('application',to_jsonb(a),'requisition',to_jsonb(r),\n 'proposals',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM talent_proposals p WHERE p.application_id=a.id),\n 'offers',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM talent_offers o WHERE o.application_id=a.id AND o.company_id=a.company_id),\n 'handoffs',(SELECT jsonb_agg(jsonb_build_object('handoff',to_jsonb(h),'receipt',to_jsonb(receipt),'people',hired.image) ORDER BY h.id) FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=h.id AND receipt.company_id=h.company_id LEFT JOIN (__PEOPLE_SRC__) hired ON hired.handoff_id=h.id AND hired.company_id=h.company_id WHERE o.application_id=a.id AND o.company_id=a.company_id)) image\n FROM talent_applications a LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id) a WHERE a.candidate_id=c.id AND a.company_id=c.company_id))::text image\n FROM talent_candidates c WHERE c.id=? AND c.company_id=?";

async function tsFiles(directory) {
  const found = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text) {
  const helper = text.match(/const jsonbAgg=[\s\S]*?;\n/)[0].replace(/:\s*string/g, '');
  const jsonbAgg = new Function(helper + ' return jsonbAgg;')();
  const appTpl = text.match(/const applicationImages=(`[\s\S]*?`);/)[1];
  const retTpl = text.match(/database\.prepare\((`[\s\S]*?`)\)\.bind/)[1];
  const app = new Function('jsonbAgg', 'talentHirePeopleSources', 'return ' + appTpl + ';')(jsonbAgg, '__PEOPLE_SRC__');
  const ret = new Function('jsonbAgg', 'applicationImages', 'candidate', 'return ' + retTpl + ';')(jsonbAgg, app, 'c.id=?');
  return { app, ret };
}

describe('talent read-tail r21 jsonb sibling family dedupe', () => {
  it('declares the shared family once and routes all five siblings through it', async () => {
    const text = await readFile(file, 'utf8');
    expect(text.split('const jsonbAgg=').length - 1).toBe(1);
    expect(text.split('${jsonbAgg(').length - 1).toBe(5);
    // the only literal jsonb_agg( is the shared helper body
    expect(text.split('jsonb_agg(').length - 1).toBe(1);
  });

  it('emits byte-identical SQL for application and retention source reads', async () => {
    const { app, ret } = evaluate(await readFile(file, 'utf8'));
    expect(app).toBe(EXPECTED_APPLICATIONS);
    expect(ret).toBe(EXPECTED_RETENTION);
  });

  it('keeps source locks and consumers unchanged', async () => {
    const text = await readFile(file, 'utf8');
    expect(text).toContain("LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts,talent_candidate_privacy_events IN SHARE ROW EXCLUSIVE MODE");
    expect(text).toContain('talentHirePeopleSourceReadLocks(database)');
    expect(text).toContain("import {talentHirePeopleSources,talentHirePeopleSourceReadLocks}");
    expect(text).toContain('export function talentAttachmentReadSourceImage');
    expect(text).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below the r20 after=69', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(65);
    expect(count).toBeLessThan(69);
  });
});
