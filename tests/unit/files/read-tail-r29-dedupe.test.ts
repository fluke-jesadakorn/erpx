import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  PUBLICATION_ID_PREFIX_SQL,
  STORAGE_LEDGER_PRESENCE_SQL,
  LINK_SET_JOIN_SQL,
  PUBLICATION_EXISTS_SQL,
  packageFileLinkJoinSql,
} from '../../../packages/server-files/src/adapters/read-tail-r29-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
type Pair = readonly [string, string];

/** Expand only the named r29 interpolations; every other byte stays as source,
 * so each rendered statement can be compared with its HEAD literal exactly. */
const expand = (text: string, pairs: readonly Pair[]) =>
  pairs.reduce((out, [needle, value]) => out.split(needle).join(value), text);
const rendered = (file: string, pairs: readonly Pair[]) => expand(src(file), pairs);

const P: Pair = ['${PUBLICATION_ID_PREFIX_SQL}', PUBLICATION_ID_PREFIX_SQL];
const L: Pair = ['${STORAGE_LEDGER_PRESENCE_SQL}', STORAGE_LEDGER_PRESENCE_SQL];
const J: Pair = ['${LINK_SET_JOIN_SQL}', LINK_SET_JOIN_SQL];
const E: Pair = ['${PUBLICATION_EXISTS_SQL}', PUBLICATION_EXISTS_SQL];
const PE: Pair = ["${packageFileLinkJoinSql('publication')}", packageFileLinkJoinSql('publication')];
const PC: Pair = ["${packageFileLinkJoinSql('public-creative')}", packageFileLinkJoinSql('public-creative')];

/** Verbatim HEAD literals for the 13 call sites replaced by the five families. */
const HEAD = {
  publicationShare: 'SELECT id FROM attachment_publications WHERE id=? AND company_id=? FOR SHARE',
  publicationPms: "SELECT id FROM attachment_publications WHERE id=? AND company_id=? AND target_type=? AND target_id=? AND state='approved'",
  ledgerReconcile: "SELECT 1 FROM attachment_storage_ledger WHERE company_id=? AND attachment_id=? AND variant=? AND object_key=? AND bytes=? AND state='stored'",
  ledgerCleanup: 'EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE company_id=? AND object_key=?)',
  linkCurrent: "SELECT 1 FROM attachment_links link JOIN attachment_sets owner ON owner.id=link.set_id WHERE link.id=? AND link.attachment_id=? AND owner.company_id=? AND owner.target_type='legal_item' AND owner.target_id=?",
  linkShared: "EXISTS(SELECT 1 FROM attachment_links link JOIN attachment_sets owner ON owner.id=link.set_id WHERE link.attachment_id=? AND link.state IN ('draft','bound') AND (owner.target_type<>'legal_item' OR owner.target_id<>? OR owner.company_id<>?))",
  publicationLink: 'EXISTS(SELECT 1 FROM attachment_links link JOIN attachment_sets set_row ON set_row.id=link.set_id WHERE link.id=publication.source_link_id AND set_row.company_id=publication.company_id)',
  creativePredicate: 'EXISTS(SELECT 1 FROM attachment_publications p',
  legalPublication: "EXISTS(SELECT 1 FROM attachment_publications publication JOIN attachment_links link ON link.id=publication.source_link_id WHERE link.attachment_id=? AND publication.state='approved')",
  rollbackPublication: 'EXISTS(SELECT 1 FROM attachment_publications WHERE derivative_object_key=?)',
  packageJoin: "\n FROM sets s JOIN attachment_links l ON l.set_id=s.id LEFT JOIN attachments a ON a.id=l.attachment_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id\n UNION ALL SELECT '",
} as const;

describe('Files read-tail r29 byte-identical fragment families', () => {
  test('helper values are the exact shared read-only literals', () => {
    expect(PUBLICATION_ID_PREFIX_SQL).toBe('SELECT id FROM attachment_publications');
    expect(STORAGE_LEDGER_PRESENCE_SQL).toBe('SELECT 1 FROM attachment_storage_ledger WHERE company_id=?');
    expect(LINK_SET_JOIN_SQL).toBe('SELECT 1 FROM attachment_links link JOIN attachment_sets');
    expect(PUBLICATION_EXISTS_SQL).toBe('EXISTS(SELECT 1 FROM attachment_publications');
    expect(packageFileLinkJoinSql('publication')).toBe(HEAD.packageJoin + "publication',");
    expect(packageFileLinkJoinSql('public-creative')).toBe(HEAD.packageJoin + "public-creative',");
    for (const sql of [
      PUBLICATION_ID_PREFIX_SQL, STORAGE_LEDGER_PRESENCE_SQL, LINK_SET_JOIN_SQL,
      PUBLICATION_EXISTS_SQL, packageFileLinkJoinSql('publication'),
    ]) expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('all 13 call sites render byte-identically to their HEAD literals', () => {
    const cases: readonly { file: string; pairs: readonly Pair[]; original: string }[] = [
      { file: 'marketing-creative.ts', pairs: [P], original: HEAD.publicationShare },
      { file: 'pms-command-facts.ts', pairs: [P], original: HEAD.publicationPms },
      { file: 'attachments/migration.ts', pairs: [L], original: HEAD.ledgerReconcile },
      { file: 'attachments/service.ts', pairs: [L], original: HEAD.ledgerCleanup },
      { file: 'legal-destruction-source-facts.ts', pairs: [J], original: HEAD.linkCurrent },
      { file: 'legal-destruction-source-facts.ts', pairs: [J], original: HEAD.linkShared },
      { file: 'attachments/service-repository.ts', pairs: [J], original: HEAD.publicationLink },
      { file: 'marketing-creative.ts', pairs: [E], original: HEAD.creativePredicate },
      { file: 'legal-destruction-source-facts.ts', pairs: [E], original: HEAD.legalPublication },
      { file: 'messaging-upload-rollback.ts', pairs: [E], original: HEAD.rollbackPublication },
      { file: 'ecommerce-package-obligation-sources.ts', pairs: [PE], original: HEAD.packageJoin + "publication'," },
      { file: 'marketing-package-obligation-sources.ts', pairs: [PC], original: HEAD.packageJoin + "public-creative'," },
      { file: 'performance-package-obligation-sources.ts', pairs: [PE], original: HEAD.packageJoin + "publication'," },
    ];
    for (const { file, pairs, original } of cases) {
      expect(rendered(file, pairs)).toContain(original);
      expect(src(file)).not.toContain(original);
    }
  });

  test('one text-only definition per helper and every call site composes it', () => {
    const helper = src('read-tail-r29-source-sql.ts');
    for (const name of [
      'PUBLICATION_ID_PREFIX_SQL', 'STORAGE_LEDGER_PRESENCE_SQL', 'LINK_SET_JOIN_SQL',
      'PUBLICATION_EXISTS_SQL', 'packageFileLinkJoinSql',
    ]) expect(helper.split(`export const ${name} =`).length - 1).toBe(1);
    expect(src('marketing-creative.ts').match(/\$\{PUBLICATION_ID_PREFIX_SQL\}/g) ?? []).toHaveLength(1);
    expect(src('pms-command-facts.ts').match(/\$\{PUBLICATION_ID_PREFIX_SQL\}/g) ?? []).toHaveLength(1);
    expect(src('attachments/migration.ts').match(/\$\{STORAGE_LEDGER_PRESENCE_SQL\}/g) ?? []).toHaveLength(1);
    expect(src('attachments/service.ts').match(/\$\{STORAGE_LEDGER_PRESENCE_SQL\}/g) ?? []).toHaveLength(1);
    expect(src('legal-destruction-source-facts.ts').match(/\$\{LINK_SET_JOIN_SQL\}/g) ?? []).toHaveLength(2);
    expect(src('attachments/service-repository.ts').match(/\$\{LINK_SET_JOIN_SQL\}/g) ?? []).toHaveLength(1);
    expect(src('marketing-creative.ts').match(/\$\{PUBLICATION_EXISTS_SQL\}/g) ?? []).toHaveLength(1);
    expect(src('legal-destruction-source-facts.ts').match(/\$\{PUBLICATION_EXISTS_SQL\}/g) ?? []).toHaveLength(1);
    expect(src('messaging-upload-rollback.ts').match(/\$\{PUBLICATION_EXISTS_SQL\}/g) ?? []).toHaveLength(1);
    for (const [file, kind] of [
      ['ecommerce-package-obligation-sources.ts', 'publication'],
      ['marketing-package-obligation-sources.ts', 'public-creative'],
      ['performance-package-obligation-sources.ts', 'publication'],
    ] as const) expect(src(file).split(`packageFileLinkJoinSql('${kind}')`).length - 1).toBe(1);
  });

  test('locks, advisory statements, native writes and r10-r28 helpers stay untouched', () => {
    const mkt = src('marketing-creative.ts');
    expect(expand(mkt, [P])).toContain('WHERE id=? AND company_id=? FOR SHARE');
    expect(mkt).toContain('LOCK TABLE attachment_publications,attachment_links,attachment_sets,attachments IN SHARE MODE');
    const legal = src('legal-destruction-source-facts.ts');
    expect(legal).toContain("AND owner_id=? FOR UPDATE");
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    expect(sr).toContain('SELECT COALESCE(max(revision),0)+1 AS next_revision');
    const msg = src('messaging-upload-rollback.ts');
    expect(msg).toContain("SELECT pg_advisory_xact_lock(hashtextextended(?,0))");
    expect(msg).toContain('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_scan_attempts,attachment_storage_ledger,attachment_publications,attachment_orphan_preservations IN SHARE ROW EXCLUSIVE MODE');
    const mig = src('attachments/migration.ts');
    for (const kind of ['MIG_001', 'MIG_012', 'MIG_023']) expect(mig).toContain(`nativeWrite('${kind}'`);
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    expect(src('commerce-delivery-dedupe-sql.ts')).toContain('export const COMMERCE_DELIVERY_GUARD_SQL =');
    expect(src('social-upload-cleanup-rows-sql.ts')).toContain('export const jsonbRowsSql =');
    expect(src('social-draft-image-guard-sql.ts')).toContain('export const socialDraftImageExistsSql =');
    const r26 = src('read-tail-r26-source-sql.ts');
    for (const name of ['preservedObjectExistsSql', 'imageSnapshotSql', 'orphanReconciliationSql', 'attachmentsByIdCompanySql'])
      expect(r26).toContain(`export const ${name} =`);
    expect(src('read-tail-r27-source-sql.ts')).toContain('export const SELECT_ATTACHMENT_ID_SQL =');
    expect(src('read-tail-r28-source-sql.ts')).toContain('export const PRIVATE_IMAGE_COLUMNS_SQL =');
  });

  test('retirement:ts-statements is strictly below the r29 BEFORE=277 baseline', () => {
    let literal = 0;
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          literal += literalSelectCount(text);
          total += selectCount(text);
        }
      }
    };
    walk(srcRoot);
    expect(literal).toBeLessThan(277);
    expect(total).toBeLessThan(278);
  });
});
