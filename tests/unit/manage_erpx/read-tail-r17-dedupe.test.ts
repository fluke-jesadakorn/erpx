import { describe, it, expect, vi } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { usageHeadSql } from '../../../packages/server-manage_erpx/src/adapters/usage-head-sql';

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

/** The folded 27-byte head, shared verbatim by both UNION ALL branches. */
const HEAD = 'SELECT ${projection} FROM (';
const OUTER = 'count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens';
const INNER = 'input_tokens,output_tokens';

/** The exact HEAD prepare-argument text, with the two head literals still inline. */
const HEAD_STATEMENT =
  'SELECT count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens FROM (${collaboration.aiUsageSource} UNION ALL SELECT input_tokens,output_tokens FROM (${people.aiUsageSource} UNION ALL ${requests.aiUsageSource} UNION ALL ${finance.aiUsageSource}) WHERE company_id=? AND month=?)';

/** The exact retired call shape: two helper calls plus the untouched tail. */
const NEW_STATEMENT =
  "${usageHeadSql('count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens')}${collaboration.aiUsageSource} UNION ALL ${usageHeadSql('input_tokens,output_tokens')}${people.aiUsageSource} UNION ALL ${requests.aiUsageSource} UNION ALL ${finance.aiUsageSource}) WHERE company_id=? AND month=?)";

/** Expand only the helper calls; leave every other byte of the template intact. */
const expandHeads = (statement: string) =>
  statement.replace(/\$\{usageHeadSql\('([^']+)'\)\}/g, (_, projection: string) =>
    `SELECT ${projection} FROM (`,
  );

describe('manage_erpx beta AI-usage UNION head family (r17)', () => {
  it('keeps the folded head byte-identical in exactly one owner-local declaration', async () => {
    expect(HEAD).toHaveLength(27);
    expect(sha256(HEAD)).toBe('d493ab803aadc3c3a10c0e4e6d11902632d29a7d88a76893b00c1c4d3e767029');
    const text = await readFile(adapter('usage-head-sql.ts'), 'utf8');
    expect(text.match(/SELECT \$\{projection\} FROM \(/g)).toHaveLength(1);
    expect(text.match(/SELECT /g)).toHaveLength(1);
    expect(usageHeadSql(OUTER)).toBe('SELECT count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens FROM (');
    expect(usageHeadSql(INNER)).toBe('SELECT input_tokens,output_tokens FROM (');
  });

  it('renders both retired head shapes byte-identically with unchanged binds', () => {
    for (const [projection, expected, length, hash, binds] of [
      [OUTER, 'SELECT count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens FROM (',
        93, 'c92aeb786bcaaa420312642ba426b7745739837de6c5e5582f7d77f2ff794031', 0],
      [INNER, 'SELECT input_tokens,output_tokens FROM (',
        40, '5a9cb744044e3baf656e319fa8092d3653db771053c739ad7e67c9b0e4d59b0e', 0],
    ] as const) {
      const sql = usageHeadSql(projection);
      expect(sql).toBe(expected);
      expect(sql).toHaveLength(length);
      expect(sha256(sql)).toBe(hash);
      expect(sql.match(/\?/g) ?? []).toHaveLength(binds);
    }
    expect(HEAD_STATEMENT).toHaveLength(301);
    expect(sha256(HEAD_STATEMENT)).toBe('e4d8f373fd9a87670205925987902bd051836033057c1607b736462bb3844366');
    expect(HEAD_STATEMENT.match(/\?/g) ?? []).toHaveLength(2);
  });

  it('retires both inline heads while keeping the caller statement byte-identical', async () => {
    const api = await readFile(adapter('beta-api.ts'), 'utf8');
    expect(api).toContain('import { usageHeadSql } from "./usage-head-sql";');
    expect(api).toContain(NEW_STATEMENT);
    expect(api).not.toContain('SELECT count(*) requests');
    expect(api).not.toContain('SELECT input_tokens,output_tokens');
    const rendered = expandHeads(NEW_STATEMENT);
    expect(rendered).toBe(HEAD_STATEMENT);
    expect(rendered).toHaveLength(301);
    expect(sha256(rendered)).toBe('e4d8f373fd9a87670205925987902bd051836033057c1607b736462bb3844366');
    expect(rendered.match(/\?/g) ?? []).toHaveLength(2);
  });

  it('leaves locks, native transports and cross-owner readers byte-identical', async () => {
    const protectedShas: Record<string, string> = {
      'company-provisioning-attachment-read-sources.ts':
        '68683c0454f731623840ab88927b932917c41f5d7a179bcac0ae0cc80f1d7b7e',
      'company-provisioning-attachment-read.ts':
        'af042402890caee72f86973e02f187bc74b8fba85170812ff9b62211ed540b59',
      'native-owner-write.ts':
        '10ce274f211859eadf3f380912324925c87b954f119d359e614b07274da52a87',
      'native-owner-read.ts':
        '365f5569777679d4c29f15c149691d0b78ab77d19a471631f097d524b86273d0',
      'beta-policy-point-sql.ts':
        'f843bacaef4fa8b41d7c9a3e43994c54f32117030c4eef2c88aa7b3ea23a8ce4',
      'beta-team.ts':
        '3641dfa12f6f597493ce993a46f0713f331bf4cc06421271ac0d72c5bc737dcf',
    };
    for (const [file, hash] of Object.entries(protectedShas)) {
      const text = await readFile(adapter(file), 'utf8');
      expect(sha256(text)).toBe(hash);
      expect(text).not.toContain('usageHeadSql');
    }
  });

  it('lowers the owner TS SELECT count from BEFORE=20 to 13', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(13);
    expect(total).toBeLessThan(20);
  });
});
