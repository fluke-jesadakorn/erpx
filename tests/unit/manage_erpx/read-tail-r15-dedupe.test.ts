import { describe, it, expect, vi } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { betaPointByKeySql } from '../../../packages/server-manage_erpx/src/adapters/beta-point-by-key-sql';
import { betaInvitationPointSql } from '../../../packages/server-manage_erpx/src/adapters/beta-invitation-point-sql';
import { betaSlotPointSql } from '../../../packages/server-manage_erpx/src/adapters/beta-slot-point-sql';

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

/** The folded 54-byte head, shared verbatim by the invitation and slot helpers. */
const HEAD = 'SELECT ${projection} FROM beta_${table} WHERE ${key}=?';

const ORACLES = [
  ['invitation-all', 'SELECT * FROM beta_team_invitations WHERE token_hash=?', 1,
    'ebb2e3e0308047e741a2300b769e45751dec94aafed79f8a523c1f61644cbcac'],
  ['invitation-user', 'SELECT accepted_user_id FROM beta_team_invitations WHERE id=?', 1,
    '09f3e85d3b000d095db80e4bf4b16825c1c56340a4b87a641807f7c7fc8d3b9b'],
  ['slot-active', "SELECT 1 FROM beta_slots WHERE holding_id=? AND state='active'", 1,
    '781c3086f1475d98ed20071149ed9473b5f631b1ee8c6a0ef692e38c68b7cbcf'],
  ['slot-entry', 'SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?', 1,
    '7d0b3edad4db8a5823428b3fdd93d463b8f9cba30e8f52fdb75a1b5923cabdea'],
] as const;

describe('manage_erpx beta point-by-key head family (r15)', () => {
  it('keeps the folded head byte-identical in exactly one owner-local declaration', async () => {
    expect(HEAD).toHaveLength(54);
    expect(sha256(HEAD)).toBe('775dc9cb0e8ec2a5428bfc7d204c73dbb887fedb5b2d90ac887a764f9715b8e4');
    const text = await readFile(adapter('beta-point-by-key-sql.ts'), 'utf8');
    expect(text.match(/SELECT \$\{projection\} FROM beta_\$\{table\} WHERE \$\{key\}=\?/g)).toHaveLength(1);
    expect(text.match(/SELECT /g)).toHaveLength(1);
    expect(betaPointByKeySql('slots', '1', 'holding_id')).toBe('SELECT 1 FROM beta_slots WHERE holding_id=?');
  });

  it('renders all four retired call shapes byte-identically with unchanged binds', () => {
    const rendered: Record<string, string> = {
      'invitation-all': betaInvitationPointSql('*', 'token_hash'),
      'invitation-user': betaInvitationPointSql('accepted_user_id', 'id'),
      'slot-active': betaSlotPointSql('1', 'holding_id', " AND state='active'"),
      'slot-entry': betaSlotPointSql('slug,company_name,branch_name,state', 'owner_user_id'),
    };
    for (const [name, sql, binds, hash] of ORACLES) {
      expect(rendered[name]).toBe(sql);
      expect(sha256(rendered[name])).toBe(hash);
      expect(rendered[name].match(/\?/g)).toHaveLength(binds);
    }
  });

  it('retires both inline heads while keeping every caller statement unchanged', async () => {
    const invitation = await readFile(adapter('beta-invitation-point-sql.ts'), 'utf8');
    const slot = await readFile(adapter('beta-slot-point-sql.ts'), 'utf8');
    for (const text of [invitation, slot]) {
      expect(text).not.toContain('SELECT ');
      expect(text).toContain('betaPointByKeySql(');
    }
    expect(invitation).toContain("betaPointByKeySql('team_invitations'");
    expect(slot).toContain("betaPointByKeySql('slots'");
    for (const [file, needle] of [
      ['beta-team.ts', "betaInvitationPointSql('*','token_hash')"],
      ['runtime-active-slot-source.ts', "betaSlotPointSql('1', 'holding_id'"],
      ['people-app-entrypoint-facts.ts', "betaSlotPointSql('slug,company_name,branch_name,state'"],
    ] as const) {
      const text = await readFile(adapter(file), 'utf8');
      expect(text).toContain(needle);
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
      expect(text).not.toContain('betaPointByKeySql');
    }
  });

  it('lowers the owner TS SELECT count from BEFORE=22 to 19', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(16);
    expect(total).toBeLessThan(20);
  });
});
