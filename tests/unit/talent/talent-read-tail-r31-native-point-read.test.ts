import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeTalentReadFirst } from '../../../packages/server-talent/src/adapters/native-talent-read';

const kind = 'talent_rd_workspace_record_source';
const sql =
  "SELECT capability,company_id FROM (SELECT r.id,r.company_id,r.branch_id,r.department_id,'requisition' kind,'recruit.positions' capability FROM talent_requisitions r\n " +
  "UNION ALL SELECT c.id,c.company_id,NULL,NULL,'candidate','recruit.candidates' FROM talent_candidates c\n " +
  "UNION ALL SELECT a.id,a.company_id,r.branch_id,r.department_id,'application','recruit.applications' FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id\n " +
  "UNION ALL SELECT p.id,a.company_id,r.branch_id,r.department_id,'proposal','recruit.applications' FROM talent_proposals p JOIN talent_applications a ON a.id=p.application_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id\n " +
  "UNION ALL SELECT o.id,o.company_id,r.branch_id,r.department_id,'offer','recruit.offers' FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id) actual_talent_source WHERE id=? LIMIT 1";

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44993',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const ok = (value: unknown) => new Response(JSON.stringify(value), { status: 200, headers: { 'content-type': 'application/json' } });

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('talent read-tail r31 owner-local point read native port', () => {
  it('retired the TS point read and routes the caller through the native kind', async () => {
    const text = await readFile(new URL('../../../packages/server-talent/src/adapters/record-purchases.ts', import.meta.url), 'utf8');
    expect(text).not.toContain('SELECT capability,company_id FROM (${sources})');
    expect(text).toContain("nativeTalentReadFirst<{capability:string;company_id:string}>('talent_rd_workspace_record_source',[id],database)");
    // The r22 sources family, locks and cross-owner predicates stay untouched.
    expect(text).toContain('import {talentSourceRows}');
    expect(text).toContain('${talentSourceRows(');
    expect(text).toContain('const sources=');
    expect(text).toContain("talentExistsSelect('workspace_records', `module_id='talent'");
    expect(text).toContain('assignmentReadSql("talent_viewer","records.read")');
  });

  it('re-pins the talent owner SELECT KPI strictly below BEFORE=48 to 47', async () => {
    let count = 0;
    const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(12);
    expect(count).toBeLessThan(40);
  });

  it('sends only the kind and positional values to the mounted command-plan route', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config;
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      expect(JSON.parse(String(init.body))).toEqual({ kind, values: ['source-1'] });
      return ok({ data: { statements: [{ sql, values: ['source-1'] }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const seen: string[] = [];
    const db = {
      prepare(statement: string) {
        seen.push(statement);
        return { bind(...values: unknown[]) {
          expect(values).toEqual(['source-1']);
          return { first: async () => ({ capability: 'recruit.offers', company_id: 'company' }) };
        } };
      },
    } as never;
    try {
      await expect(nativeTalentReadFirst(kind, ['source-1'], db)).resolves.toEqual({ capability: 'recruit.offers', company_id: 'company' });
      expect(seen).toEqual([sql]);
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
      expect(fetchMock).toHaveBeenCalledTimes(1);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });
});
