import { describe, it, expect, vi } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { betaPolicyLimitSql } from '../../../packages/server-manage_erpx/src/adapters/beta-policy-limit-sql';
import { betaPolicyStorageBytesSql } from '../../../packages/server-manage_erpx/src/adapters/attachments-service-facts';
import { recordsAiMonthlyLimitSql } from '../../../packages/server-manage_erpx/src/adapters/records-ai-limit-projection';
import { documentPageStorageLimitSql } from '../../../packages/server-manage_erpx/src/adapters/document-page-storage-limit';
import { betaPolicyPointSql } from '../../../packages/server-manage_erpx/src/adapters/beta-policy-point-sql';

vi.mock('server-only', () => ({}));

const adapter = (name: string) =>
  new URL(`../../../packages/server-manage_erpx/src/adapters/${name}`, import.meta.url);
const srcDir = new URL('../../../packages/server-manage_erpx/src/', import.meta.url);
const sha256 = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

async function tsSources(dir: URL): Promise<string[]> {
  const out: string[] = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const url = new URL(`${entry.name}${entry.isDirectory() ? '/' : ''}`, dir);
    if (entry.isDirectory()) out.push(...(await tsSources(url)));
    else if (entry.name.endsWith('.ts')) out.push(await readFile(url, 'utf8'));
  }
  return out;
}

/** The folded 56-byte head, shared verbatim by the storage-bytes and AI-limit readers. */
const HEAD = 'SELECT ${projection} FROM beta_policy WHERE company_id=?';

const ORACLES = [
  ['storage-bytes', betaPolicyStorageBytesSql,
    'SELECT storage_bytes FROM beta_policy WHERE company_id=?', 1,
    '1259639e02eef5037214427f6bf6d9a448b6711fb1daaf5d9e0fd0acb83ff4f7'],
  ['monthly-ai', recordsAiMonthlyLimitSql,
    'COALESCE((SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?),100)', 1,
    '2318a02389435d79d51563f0bfb075e13bb34406726d58a92930c4a195a19b4f'],
  ['doc-page-limit', documentPageStorageLimitSql,
    'COALESCE((SELECT storage_bytes FROM beta_policy WHERE company_id=?),9007199254740991)', 1,
    '4b35495f99c3c738026d20b340ab8b825e4e815ed328d110196dfbc3d3e8ef6d'],
  ['point-all-tab', betaPolicyPointSql('*'),
    'SELECT\t* FROM beta_policy WHERE company_id=?', 1,
    '62a3aeac19ed92d037f2fc87a7cfc1593fad8eeb61d8c326080e0a6ae8e8e1e2'],
] as const;

describe('manage_erpx beta_policy Company-limit head family (r16)', () => {
  it('keeps the folded head byte-identical in exactly one owner-local declaration', async () => {
    expect(HEAD).toHaveLength(56);
    expect(sha256(HEAD)).toBe('8211dfcde284f73a46610bc53097c16ec971aba662af747120c8d1a5bea476d2');
    const text = await readFile(adapter('beta-policy-limit-sql.ts'), 'utf8');
    expect(text.match(/SELECT \$\{projection\} FROM beta_policy WHERE company_id=\?/g)).toHaveLength(1);
    expect(text.match(/SELECT /g)).toHaveLength(1);
    expect(betaPolicyLimitSql('storage_bytes')).toBe('SELECT storage_bytes FROM beta_policy WHERE company_id=?');
    expect(betaPolicyLimitSql('ai_monthly_limit')).toBe('SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?');
  });

  it('renders all four retired call shapes byte-identically with unchanged binds', () => {
    for (const [, sql, expected, binds, hash] of ORACLES) {
      expect(sql).toBe(expected);
      expect(sha256(sql)).toBe(hash);
      expect(sql.match(/\?/g)).toHaveLength(binds);
    }
  });

  it('retires both inline heads while keeping every caller statement unchanged', async () => {
    const attachments = await readFile(adapter('attachments-service-facts.ts'), 'utf8');
    const records = await readFile(adapter('records-ai-limit-projection.ts'), 'utf8');
    for (const text of [attachments, records]) {
      expect(text).not.toContain('SELECT ');
      expect(text).toContain('betaPolicyLimitSql(');
    }
    expect(attachments).toContain("betaPolicyLimitSql('storage_bytes')");
    expect(records).toContain("betaPolicyLimitSql('ai_monthly_limit')");
    for (const [file, needle] of [
      ['legal-portal-storage-allowance.ts', 'import {betaPolicyStorageBytesSql} from "./attachments-service-facts"'],
      ['document-page-storage-limit.ts', "import {betaPolicyStorageBytesSql} from './attachments-service-facts'"],
      ['finance-ai-quota-sources.ts', "export {recordsAiMonthlyLimitSql as financeAiCompanyAllowanceSql} from './records-ai-limit-projection'"],
    ] as const) {
      expect(await readFile(adapter(file), 'utf8')).toContain(needle);
    }
  });

  it('leaves locks, native transports and cross-owner readers byte-identical', async () => {
    const protectedShas: Record<string, string> = {
      'company-provisioning-attachment-read-sources.ts':
        '4a7abe70881636a9cf40893a677c283970fdb0c0770db4b733a2d25f178801f9',
      'company-provisioning-attachment-read.ts':
        'af042402890caee72f86973e02f187bc74b8fba85170812ff9b62211ed540b59',
      'native-owner-write.ts':
        '10ce274f211859eadf3f380912324925c87b954f119d359e614b07274da52a87',
      'native-owner-read.ts':
        '3f0d19f6d97e5adda3f1c2f6048077d324d35cf3c5e44a4cdbbc9c25dfdd41e9',
      'beta-policy-point-sql.ts':
        'f843bacaef4fa8b41d7c9a3e43994c54f32117030c4eef2c88aa7b3ea23a8ce4',
      'beta-team.ts':
        'd7693cd89c304891c4137f8085516feeb12145c59a23434801bd5ea45f7efc2f',
    };
    for (const [file, hash] of Object.entries(protectedShas)) {
      const text = await readFile(adapter(file), 'utf8');
      expect(sha256(text)).toBe(hash);
      expect(text).not.toContain('betaPolicyLimitSql');
    }
  });

  it('lowers the owner TS SELECT count from BEFORE=21 to 19', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(16);
    expect(total).toBeLessThan(20);
  });
});
