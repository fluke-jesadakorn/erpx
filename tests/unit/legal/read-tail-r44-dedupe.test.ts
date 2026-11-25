import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalExternalUploadShareJoinHead,
  legalFilePreservationHoldTargetHead,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r44-sources';
import contract from '../../../packages/server-legal/src/adapters/legal-read-tail-r44-sources.json' with {
  type: 'json',
};

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

const LEGAL_HOLD_EXISTS = "EXISTS(SELECT 1 FROM legal_holds h WHERE h.item_id=i.id AND h.state<>'released') legal_hold";

/** Each member's expected SQL is the exact literal that was inline before the
 * head was extracted. Rendering must be byte-identical, including binds, the
 * state inequality token, ORDER BY and LIMIT. */
const extMembers = [
  {
    file: 'legal/documents.ts', projection: 'u.*,s.item_id', u: 'u', s: 's', tail: 'WHERE u.id=?',
    expected: 'SELECT u.*,s.item_id FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id WHERE u.id=?',
  },
  {
    file: 'legal/service.ts', projection: 'u.id,u.name,u.checksum,u.created_by,u.created_at', u: 'u', s: 's',
    tail: 'WHERE s.item_id=? ORDER BY u.created_at DESC LIMIT 100',
    expected: 'SELECT u.id,u.name,u.checksum,u.created_by,u.created_at FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id WHERE s.item_id=? ORDER BY u.created_at DESC LIMIT 100',
  },
  {
    file: 'legal/destruction-sources.ts', projection: 'upload.id,upload.object_key,upload.checksum,upload.attachment_source_id,upload.attachment_link_id', u: 'upload', s: 'share', tail: 'WHERE share.item_id=?',
    expected: 'SELECT upload.id,upload.object_key,upload.checksum,upload.attachment_source_id,upload.attachment_link_id FROM legal_external_uploads upload JOIN legal_shares share ON share.id=upload.share_id WHERE share.item_id=?',
  },
  {
    file: 'legal/destruction-sources.ts', projection: '1', u: 'upload', s: 'share',
    tail: 'WHERE upload.id=? AND share.item_id=? AND upload.checksum=? AND upload.object_key=?',
    expected: 'SELECT 1 FROM legal_external_uploads upload JOIN legal_shares share ON share.id=upload.share_id WHERE upload.id=? AND share.item_id=? AND upload.checksum=? AND upload.object_key=?',
  },
  {
    file: 'canonical-attachment-read.ts', projection: 'jsonb_agg(to_jsonb(u) ORDER BY u.id)', u: 'u', s: 'share', tail: 'WHERE share.item_id=i.id',
    expected: 'SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id) FROM legal_external_uploads u JOIN legal_shares share ON share.id=u.share_id WHERE share.item_id=i.id',
  },
  {
    file: 'attachment-migration-facts.ts',
    projection: `u.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,u.id component_ref,
    u.name original_name,u.media_type,u.bytes,u.checksum,u.object_key,u.created_by,u.created_at,
    ${LEGAL_HOLD_EXISTS}
   `,
    u: 'u', s: 's', tail: 'JOIN legal_items i ON i.id=s.item_id\n    WHERE u.attachment_source_id IS NULL ORDER BY u.created_at,u.id',
    expected: `SELECT u.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,u.id component_ref,
    u.name original_name,u.media_type,u.bytes,u.checksum,u.object_key,u.created_by,u.created_at,
    ${LEGAL_HOLD_EXISTS}
    FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id JOIN legal_items i ON i.id=s.item_id
    WHERE u.attachment_source_id IS NULL ORDER BY u.created_at,u.id`,
  },
];

const presMembers = [
  {
    file: 'runtime-minio-mutation-authority.ts', projection: '1', target: 't', hold: 'h', inequality: '!=',
    expected: "SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets t ON t.item_id=p.item_id JOIN legal_holds h ON h.id=t.hold_id WHERE h.state!='released'",
  },
  {
    file: 'runtime-mutation-authority.ts', projection: '1', target: 'target', hold: 'hold', inequality: '<>',
    expected: "SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets target ON target.item_id=p.item_id JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state<>'released'",
  },
];

const extCallers: Record<string, number> = {
  'legal/documents.ts': 1,
  'legal/service.ts': 1,
  'legal/destruction-sources.ts': 2,
  'canonical-attachment-read.ts': 1,
  'attachment-migration-facts.ts': 1,
};
const presCallers: Record<string, number> = {
  'runtime-minio-mutation-authority.ts': 1,
  'runtime-mutation-authority.ts': 1,
};

describe('legal read-tail r44 parameterized external-upload and preservation head family', () => {
  it('renders byte-identical SQL for every external upload/share member', () => {
    for (const m of extMembers) {
      expect(legalExternalUploadShareJoinHead(m.projection, m.u, m.s) + m.tail).toBe(m.expected);
    }
    expect(legalExternalUploadShareJoinHead('u.$&,s.item_id', 'u', 's')).toBe(
      'SELECT u.$&,s.item_id FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id ',
    );
  });

  it('renders byte-identical preservation/hold-target SQL and keeps both inequality tokens', () => {
    for (const m of presMembers) {
      expect(legalFilePreservationHoldTargetHead(m.projection, m.target, m.hold, m.inequality)).toBe(m.expected);
    }
    const rendered = presMembers.map(m => legalFilePreservationHoldTargetHead(m.projection, m.target, m.hold, m.inequality));
    expect(rendered[0]).toContain("state!='released'");
    expect(rendered[1]).toContain("state<>'released'");
  });

  it('keeps both heads read-only (no writes or row locks)', () => {
    const all = [
      ...extMembers.map(m => legalExternalUploadShareJoinHead(m.projection, m.u, m.s)),
      ...presMembers.map(m => legalFilePreservationHoldTargetHead(m.projection, m.target, m.hold, m.inequality)),
    ].join(' ');
    expect(all).toMatch(/SELECT \S+ FROM (legal_external_uploads|legal_file_preservations)/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition of each head and removes every inline head from the callers', async () => {
    const shared = await src('legal-read-tail-r44-sources.ts');
    expect(shared.match(/export const legalExternalUploadShareJoinHead/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const legalFilePreservationHoldTargetHead/g) ?? []).toHaveLength(1);
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(shared).toContain('legal-read-tail-r44-sources.json');
    expect(shared).toContain('LEGAL_READ_TAIL_R44_CONTRACT_INVALID');
    expect(contract).toEqual({
      externalUploadShare:
        'SELECT {projection} FROM legal_external_uploads {uploadAlias} JOIN legal_shares {shareAlias} ON {shareAlias}.id={uploadAlias}.share_id ',
      filePreservationHoldTarget:
        "SELECT {projection} FROM legal_file_preservations p JOIN legal_hold_targets {targetAlias} ON {targetAlias}.item_id=p.item_id JOIN legal_holds {holdAlias} ON {holdAlias}.id={targetAlias}.hold_id WHERE {holdAlias}.state{inequality}'released'",
    });
    const rust = await src('legal_read_tail_r44.rs');
    expect(rust).toContain('include_str!("legal-read-tail-r44-sources.json")');
    expect(rust).toContain('render_external_upload_share');
    expect(rust).toContain('render_file_preservation_hold_target');
    expect(rust).toContain('755bd9304bff30bffd2aade73bbb5ce8abc0e393b21b2af100adf2be357da75b');
    const extHead = 'FROM legal_external_uploads ';
    const presHead = 'FROM legal_file_preservations p JOIN legal_hold_targets ';
    for (const [file, count] of Object.entries(extCallers)) {
      const text = await src(file);
      expect(text.match(/legalExternalUploadShareJoinHead\(/g) ?? []).toHaveLength(count);
      expect(text).not.toContain(extHead);
    }
    for (const [file, count] of Object.entries(presCallers)) {
      const text = await src(file);
      expect(text.match(/legalFilePreservationHoldTargetHead\(/g) ?? []).toHaveLength(count);
      expect(text).not.toContain(presHead);
    }
  });

  it('leaves corpus/citation/advisory/FOR UPDATE reads intact', async () => {
    const [corpus, citations, r9, r24, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r24-sources.ts'),
      src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9-r43 helpers untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts', 'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts', 'legal-read-tail-r34-sources.ts', 'legal-read-tail-r35-sources.ts',
      'legal-read-tail-r36-sources.ts', 'legal-read-tail-r38-sources.ts', 'legal-read-tail-r39-sources.ts',
      'legal-read-tail-r40-sources.ts', 'legal-read-tail-r41-sources.ts', 'legal-read-tail-r42-sources.ts',
      'legal-read-tail-r43-sources.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalFilesAdvisoryLockSql', 'legalCompanyAdvisoryLockSql',
      'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalActorScopeProjectionSql', 'legalEventsByItemSql',
      'legalRateCountPrefix', 'legalDisposedDocumentPrefix', 'legalPurchaseClockCteSql',
      'legalApprovedCycleSql', 'legalHoldActiveSql', 'legalHoldTargetGuardPrefix',
      'legalMemberEditExistsSql', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalPurchaseClockExistsSql',
      'legalOrderedHistorySql', 'legalRecentItemListSql', 'legalPointByIdSql',
      'legalStatePointSql', 'legalPointByIdAndKeySql', 'legalItemOrderedSql', 'legalPointByKeySql',
      'legalReleasedItemGuardSql', 'legalMemberEditPointSql', 'legalFactDataSql', 'legalMetricCountPrefix',
      'legalDisposedGuardSql', 'legalHoldOwnTargetSql', 'legalExistsPointGuardHeadSql',
      'legalShareDocumentReadHead', 'legalFeatureSearchSql', 'legalEnvelopeSignerReadHead',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=212', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(212);
  });

  it('retirement:ts-statements is BEFORE=109 -> AFTER=107 for the r44 native contract', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(109);
    expect(total).toBe(101);
  });
});
