import { describe, it, expect, vi } from 'vitest';
import { readFile, readdir, access } from 'node:fs/promises';
import { createHash } from 'node:crypto';

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

/** Byte-identical retired statements now owned by the Rust registry. */
const ORACLES = [
  ['manage_rd_slot_active', "SELECT 1 FROM beta_slots WHERE holding_id=? AND state='active'"],
  ['manage_rd_slot_owner', 'SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?'],
  ['manage_rd_invitation_by_token', 'SELECT * FROM beta_team_invitations WHERE token_hash=?'],
  ['manage_rd_invitation_accepted_user', 'SELECT accepted_user_id FROM beta_team_invitations WHERE id=?'],
] as const;

describe('manage_erpx beta point-by-key native cutover (r8)', () => {
  it('owns every byte-identical statement in the Rust read registry only', async () => {
    const rust = await readFile(adapter('native_owner_read_sql.rs'), 'utf8');
    for (const [kind, sql] of ORACLES) {
      expect(rust).toContain(`"${sql}"`);
      expect(rust).toContain(`pub const ${kind.replace('manage_rd_', 'ME_RD_').toUpperCase()}`);
    }
    expect(rust).toContain('READ_SLOT_ACTIVE');
    expect(rust).toContain('READ_SLOT_OWNER');
    expect(rust).toContain('READ_INVITATION_BY_TOKEN');
    expect(rust).toContain('READ_INVITATION_ACCEPTED_USER');
    expect(rust.match(/pub const ME_RD_/g)).toHaveLength(8);
  });

  it('exports closed native kind constants with no statement text in TypeScript', async () => {
    const { ME_RD_SLOT_ACTIVE, ME_RD_SLOT_OWNER, ME_RD_INVITATION_BY_TOKEN, ME_RD_INVITATION_ACCEPTED_USER } =
      await import('../../../packages/server-manage_erpx/src/adapters/native-owner-read');
    expect([ME_RD_SLOT_ACTIVE, ME_RD_SLOT_OWNER, ME_RD_INVITATION_BY_TOKEN, ME_RD_INVITATION_ACCEPTED_USER]).toEqual(
      ORACLES.map(([kind]) => kind),
    );
    const client = await readFile(adapter('native-owner-read.ts'), 'utf8');
    expect(client).not.toContain('SELECT ');
    expect(client).not.toContain('beta_slots WHERE');
    expect(client).not.toContain('beta_team_invitations WHERE');
  });

  it('deletes the replaced TypeScript SQL helpers', async () => {
    for (const file of ['beta-point-by-key-sql.ts', 'beta-slot-point-sql.ts', 'beta-invitation-point-sql.ts']) {
      await expect(access(adapter(file))).rejects.toThrow();
    }
  });

  it('cuts every point-by-key caller to the native client without emitted SQL', async () => {
    for (const [file, needle] of [
      ['runtime-active-slot-source.ts', 'nativeBetaSlotActive(db, holdingId)'],
      ['people-app-entrypoint-facts.ts', 'nativeOwnerOnboardingSlot(db,userId)'],
      ['beta-team.ts', 'nativeBetaInvitationByToken(db,value)'],
      ['beta-team.ts', 'nativeBetaInvitationAcceptedUser(db,id)'],
    ] as const) {
      const text = await readFile(adapter(file), 'utf8');
      expect(text).toContain(needle);
      expect(text).not.toContain('betaSlotPointSql');
      expect(text).not.toContain('betaInvitationPointSql');
      expect(text).not.toContain('betaPointByKeySql');
    }
  });

/** Byte-identical retired Company provisioning source image, now Rust-owned. */
const PROVISIONING_IMAGE =
  "SELECT jsonb_build_object('provisioning',to_jsonb(provisioning),'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_provisioning_events event WHERE event.company_id=provisioning.company_id AND event.holding_id=provisioning.holding_id))::text image FROM company_provisioning provisioning WHERE provisioning.company_id=? AND provisioning.company_id=?";

describe('manage_erpx Company provisioning attachment image native cutover', () => {
  it('owns the byte-identical two-bind statement in the Rust registry only', async () => {
    const rust = await readFile(adapter('native_owner_read_sql.rs'), 'utf8');
    expect(rust).toContain(`"${PROVISIONING_IMAGE}"`);
    expect(rust).toContain('ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE');
    expect(rust).toContain('READ_COMPANY_PROVISIONING_ATTACHMENT_IMAGE');
    expect(rust).toContain('bind_count: 2');
  });

  it('cuts the image reader to the native client without emitted SQL', async () => {
    const source = await readFile(adapter('company-provisioning-attachment-read-sources.ts'), 'utf8');
    expect(source).toContain('nativeCompanyProvisioningAttachmentImage(database,id,actor.companyId)');
    expect(source).not.toContain('jsonb_build_object');
    expect(source).not.toContain('company_provisioning_events event');
    expect(source).toContain('LOCK TABLE company_provisioning,company_provisioning_events IN SHARE ROW EXCLUSIVE MODE');
    const { ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE } =
      await import('../../../packages/server-manage_erpx/src/adapters/native-owner-read');
    expect(ME_RD_COMPANY_PROVISIONING_ATTACHMENT_IMAGE).toBe(
      'manage_rd_company_provisioning_attachment_image',
    );
    const client = await readFile(adapter('native-owner-read.ts'), 'utf8');
    expect(client).not.toContain('jsonb_build_object');
    expect(client).not.toContain('FROM company_provisioning');
  });
});

  it('leaves locks, authority readers and cross-owner sources byte-identical', async () => {
    const protectedShas: Record<string, string> = {
      'company-provisioning-attachment-read-sources.ts':
        '68683c0454f731623840ab88927b932917c41f5d7a179bcac0ae0cc80f1d7b7e',
      'company-provisioning-attachment-read.ts':
        'af042402890caee72f86973e02f187bc74b8fba85170812ff9b62211ed540b59',
      'native-owner-write.ts':
        '10ce274f211859eadf3f380912324925c87b954f119d359e614b07274da52a87',
      'beta-policy-point-sql.ts':
        'f843bacaef4fa8b41d7c9a3e43994c54f32117030c4eef2c88aa7b3ea23a8ce4',
    };
    for (const [file, hash] of Object.entries(protectedShas)) {
      const text = await readFile(adapter(file), 'utf8');
      expect(sha256(text)).toBe(hash);
    }
  });

  it('lowers the owner TS literal SELECT count from BEFORE=15 to 11', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(11);
    expect(total).toBeLessThan(15);
  });
});
