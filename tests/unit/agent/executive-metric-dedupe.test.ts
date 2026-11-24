import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  readFile(new URL(`../../../packages/server-agent/src/${name}`, import.meta.url), 'utf8');

/** Verbatim pre-dedupe statements (r13 -> r14 BEFORE: 63 SELECTs). */
const approvedVersionsSql =
  "SELECT 'Approved versions' label,count(*)::text value FROM agent_versions v JOIN agent_catalog c ON c.id=v.agent_id WHERE c.company_id=? AND c.status='approved' AND v.status='approved'";
const invocationOutcomesSql =
  "SELECT e.outcome label,count(*)::text value FROM ai_invocation_evidence e WHERE e.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1";

describe('Agent read-tail r14 executive metric count family dedupe', () => {
  it('authors the parameterized count projection once', async () => {
    const source = await src('adapters/executive-metric-projections.ts');
    const def = source.match(/const agentMetricCountSql = \(label: string\) =>\s*`([^`]*)`;/)?.[1];
    expect(def).toBe('SELECT ${label} label,count(*)::text value FROM ');
    expect(source.match(/SELECT /g)).toHaveLength(1);
  });

  it('routes both metric reads through the shared projection with their own source and binds', async () => {
    const source = await src('adapters/executive-metric-projections.ts');
    expect(source).toContain('agentMetricCountSql("\'Approved versions\'") +');
    expect(source).toContain('agentMetricCountSql(label) +');
    expect(source.match(/agentMetricCountSql\(/g)).toHaveLength(2);
    expect(source).toContain('values = [a.companyId];');
    expect(source).toContain('values = [a.companyId, Date.parse(f.from), Date.parse(f.to) + 86400000];');
  });

  it('renders the exact pre-dedupe statement text for both callers', () => {
    const metricCount = (label: string) => `SELECT ${label} label,count(*)::text value FROM `;
    expect(
      metricCount("'Approved versions'") +
        'agent_versions v JOIN agent_catalog c ON c.id=v.agent_id WHERE c.company_id=? AND c.status=\'approved\' AND v.status=\'approved\'',
    ).toBe(approvedVersionsSql);
    expect(
      metricCount('e.outcome') +
        'ai_invocation_evidence e WHERE e.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1',
    ).toBe(invocationOutcomesSql);
  });

  it('leaves the translation guard and the lock statements untouched', async () => {
    const assistant = await src('adapters/workspace-assistant.ts');
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string)');
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
    const governance = await src('adapters/agent-governance.ts');
    expect(governance).toContain('"SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE"');
    expect(governance).toContain('"SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c"');
    expect(governance).toContain('"SELECT id FROM agent_pools WHERE id=? FOR SHARE"');
  });
});
