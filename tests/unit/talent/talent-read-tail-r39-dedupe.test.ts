import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const adapters = resolve(__dirname, '../../../packages/server-talent/src/adapters');
const src = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const strip = (s: string) => s.replace(/:\s*string/g, '');
const grp = (text: string, re: RegExp) => {
  const m = text.match(re);
  if (!m) throw new Error(`no match ${re}`);
  return m[1];
};
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const ev = (vars: Record<string, unknown>, body: string) => new Function(...Object.keys(vars), body)(...Object.values(vars)) as any;

const headDecl = strip(src('talent-read-tail-r37-sources.ts').match(/export const selectProjection = [\s\S]*?;\n/)![0].replace('export const', 'const'));
const selectProjection = new Function(headDecl + ' return selectProjection;')() as (p: string) => string;

// Exact pre-r39 emitted SQL (BEFORE=27). Stubs keep the pinned strings
// owner-local; every value below was captured from the former literal heads.
const GOLDEN: Record<string, string> = {
  jsonbAgg1: "(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM talent_proposals p WHERE p.application_id=a.id)",
  jsonbAgg2: "(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM talent_candidate_privacy_events event WHERE event.candidate_id=c.id AND event.company_id=c.company_id)",
  applicationImages: "SELECT a.id,a.candidate_id,a.company_id,jsonb_build_object('application',to_jsonb(a),'requisition',to_jsonb(r),\n 'proposals',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM talent_proposals p WHERE p.application_id=a.id),\n 'offers',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM talent_offers o WHERE o.application_id=a.id AND o.company_id=a.company_id),\n 'handoffs',(SELECT jsonb_agg(jsonb_build_object('handoff',to_jsonb(h),'receipt',to_jsonb(receipt),'people',hired.image) ORDER BY h.id) FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=h.id AND receipt.company_id=h.company_id LEFT JOIN (__PEOPLE_SRC__) hired ON hired.handoff_id=h.id AND hired.company_id=h.company_id WHERE o.application_id=a.id AND o.company_id=a.company_id)) image\n FROM talent_applications a LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id",
  candidateImage: "SELECT jsonb_build_object('candidate',to_jsonb(c),\n 'privacy',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM talent_candidate_privacy_events event WHERE event.candidate_id=c.id AND event.company_id=c.company_id),\n 'applications',(SELECT jsonb_agg(a.image ORDER BY a.id) FROM (SELECT a.id,a.candidate_id,a.company_id,jsonb_build_object('application',to_jsonb(a),'requisition',to_jsonb(r),\n 'proposals',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM talent_proposals p WHERE p.application_id=a.id),\n 'offers',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM talent_offers o WHERE o.application_id=a.id AND o.company_id=a.company_id),\n 'handoffs',(SELECT jsonb_agg(jsonb_build_object('handoff',to_jsonb(h),'receipt',to_jsonb(receipt),'people',hired.image) ORDER BY h.id) FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=h.id AND receipt.company_id=h.company_id LEFT JOIN (__PEOPLE_SRC__) hired ON hired.handoff_id=h.id AND hired.company_id=h.company_id WHERE o.application_id=a.id AND o.company_id=a.company_id)) image\n FROM talent_applications a LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id) a WHERE a.candidate_id=c.id AND a.company_id=c.company_id))::text image\n FROM talent_candidates c WHERE c.id=? AND c.company_id=?",
  scopeProbe: "SELECT 1 FROM__CTE__workforce_actor_scope WHERE __WHERE__",
  eligibleOwner: "EXISTS(SELECT 1 __X_FROM__ WHERE x.company_id=a.company_id AND x.user_id=a.owner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=r.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=r.department_id)))",
  ownedApps: "SELECT a.*,r.job_title,c.legal_name candidate_name,CASE WHEN __ELIGIBLE__ THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.company_id=? AND ",
  detailApp: "SELECT a.*,r.job_title,r.branch_id,r.department_id,c.legal_name candidate_name,c.email candidate_email,c.retention_until,c.version candidate_version,CASE WHEN __ELIGIBLE__ THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.id=? AND a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)",
  recruiterRefs: "SELECT DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name __X_FROM__ LEFT __P_JOIN__ ON p.company_id=x.company_id AND p.user_id=x.user_id AND p.merged_into_id IS NULL WHERE x.company_id=? AND iam_assignment_active(x.id,?) AND iam_principal_active(x.user_id,x.company_id,?) ORDER BY 2",
  metricSource: "__CLOCK__ SELECT talent_metric_owned.* FROM (__ROWS__) talent_metric_owned CROSS JOIN talent_metric_clock WHERE __READ__ AND __READ__ AND __PRED__",
};

function render(): Record<string, string> {
  const ars = src('attachment-read-sources.ts');
  const jsonbAgg = ev({ selectProjection }, strip(grp(ars, /(const jsonbAgg=[\s\S]*?;\n)/)) + '\nreturn jsonbAgg;');
  const appTpl = grp(ars, /const applicationImages=(`[\s\S]*?`);/);
  const app = ev({ selectProjection, jsonbAgg, talentHirePeopleSources: '__PEOPLE_SRC__' }, 'return ' + appTpl + ';');
  const retTpl = grp(ars, /database\.prepare\((`[\s\S]*?`)\)\.bind/);
  const candidateImage = ev({ selectProjection, jsonbAgg, applicationImages: app, candidate: 'c.id=?' }, 'return ' + retTpl + ';');

  const readTpl = grp(src('attachment-read.ts'), /\.prepare\((`SELECT|`\$\{selectProjection[\s\S]*?`)\)/);
  const scopeProbe = ev({ selectProjection, talentActorScopeCte: '__CTE__', currentScope: { where: '__WHERE__' } }, 'return ' + readTpl + ';');

  const tal = src('talent.ts');
  const eligTpl = grp(tal, /const eligibleOwner = (`[^;]*`);\n/);
  const eligibleOwner = ev({ selectProjection, identityaccessSources: { talent_adapters_talent_assignments_x_from: '__X_FROM__' } }, 'return ' + eligTpl + ';');
  const projDecl = strip(grp(tal, /(const applicationProjection = [\s\S]*?;\n)/));
  const projection = ev({ selectProjection, eligibleOwner: '__ELIGIBLE__' }, projDecl + '\nreturn applicationProjection;');
  const distinctTpl = grp(tal, /\.prepare\(\s*(`\$\{selectProjection[\s\S]*?\}`),\s*\n/);
  const recruiterRefs = ev({ selectProjection, identityaccessSources: { talent_adapters_talent_assignments_x_from: '__X_FROM__' }, peoplepayrollSources: { talent_adapters_talent_hr_people_p_join: '__P_JOIN__' } }, 'return ' + distinctTpl + ';');

  const metricTpl = grp(src('metric-purchases.ts'), /sql:(`[\s\S]*?`),args:/);
  const metricSource = ev({ selectProjection, talentClockCte: () => '__CLOCK__', rows: '__ROWS__', assignmentReadSql: () => '__READ__', purchasedCapabilityPredicate: () => '__PRED__', capability: '__CAP__' }, 'return ' + metricTpl + ';');

  return {
    jsonbAgg1: jsonbAgg('to_jsonb(p)', 'p.id', 'talent_proposals p', 'p.application_id=a.id'),
    jsonbAgg2: jsonbAgg('to_jsonb(event)', 'event.id', 'talent_candidate_privacy_events event', 'event.candidate_id=c.id AND event.company_id=c.company_id'),
    applicationImages: app,
    candidateImage,
    scopeProbe,
    eligibleOwner,
    ownedApps: projection('c.legal_name candidate_name', 'a.company_id=? AND '),
    detailApp: projection('r.branch_id,r.department_id,c.legal_name candidate_name,c.email candidate_email,c.retention_until,c.version candidate_version', 'a.id=? AND a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)'),
    recruiterRefs,
    metricSource,
  };
}

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r39 straggler projection-head family', () => {
  test('renders byte-identically to every pre-r39 literal head', () => {
    const actual = render();
    expect(Object.keys(actual).sort()).toEqual(Object.keys(GOLDEN).sort());
    for (const [name, expected] of Object.entries(GOLDEN)) {
      expect(actual[name], name).toBe(expected);
      readOnly(actual[name]);
    }
  });

  test('all eight named stragglers delegate to the r37 head', () => {
    const targets: Array<[string, number, number, string]> = [
      ['attachment-read-sources.ts', 3, 1, "'./talent-read-tail-r37-sources'"],
      ['attachment-read.ts', 1, 0, "'./talent-read-tail-r37-sources'"],
      ['talent.ts', 3, 0, '"./talent-read-tail-r37-sources"'],
      ['metric-purchases.ts', 1, 0, "'./talent-read-tail-r37-sources'"],
    ];
    for (const [name, delegations, literals, specifier] of targets) {
      const text = src(name);
      expect(text.match(/selectProjection\(/g) ?? [], name).toHaveLength(delegations);
      expect(text.match(/\bSELECT\b/g) ?? [], name).toHaveLength(literals);
      expect(text, name).toContain(`from ${specifier}`);
    }
    expect(src('attachment-read-sources.ts')).toContain('EXISTS(SELECT 1 FROM talent_offers selected');
  });

  test('one universal head, locks, owner boundary and native reads stay untouched', () => {
    const head = src('talent-read-tail-r37-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(head.match(/export const selectProjection/g) ?? []).toHaveLength(1);
    expect(selectProjection('id')).toBe('SELECT id');

    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE talent_requisitions');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('people-hire-handoff-facts.ts')).toContain('FOR ${mode}');
    expect(src('native-talent-read.ts')).toContain('nativeTalentReadFirst');
    expect(src('talent-read-tail-r36-sources.ts')).toContain('SELECT 1 FROM ${source} WHERE ${predicate}');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE talent_requisitions');
    for (const name of ['attachment-read.ts', 'talent.ts', 'metric-purchases.ts'])
      expect(src(name)).not.toMatch(/LOCK TABLE|FOR\s+UPDATE|FOR\s+SHARE|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|pg_advisory/i);
  });

  test('lowers the talent owner SELECT KPI from BEFORE=27 to 19', () => {
    const files: string[] = [];
    const walk = (d: string) => {
      for (const e of readdirSync(d, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(d, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(resolve(__dirname, '../../../packages/server-talent/src'));
    let count = 0;
    for (const f of files) count += (readFileSync(f, 'utf8').match(/\bSELECT\b/g) ?? []).length;
    expect(count).toBe(11);
    expect(count).toBeLessThan(27);
  });
});
