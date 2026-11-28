import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalPointByIdSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r32-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');
const point = (columns: string, table: string) => `SELECT ${columns} FROM ${table} WHERE id=?`;

const members: [string, string][] = [
  ['created_by', 'legal_documents'],
  ['id,name,body,version,checksum,media_type', 'legal_documents'],
  ['*', 'legal_portal_receipts'],
  ['company_id,branch_id,department_id,version', 'legal_items'],
];

describe('legal read-tail r32 parameterized family dedupe', () => {
  it('renders byte-identical single-key point-read SQL for every member', () => {
    for (const [columns, table] of members) {
      expect(legalPointByIdSql(columns, table)).toBe(point(columns, table));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([c, t]) => legalPointByIdSql(c, t)).join(' ');
    expect(all).toMatch(/SELECT [\w,*]+ FROM legal_\w+ WHERE id=\?/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline family literals in the callers', async () => {
    const [shared, approval, portal] = await Promise.all([
      src('legal-read-tail-r32-sources.ts'),
      src('legal/approval.ts'),
      src('legal/portal.ts'),
    ]);
    expect(shared.match(/export const legalPointByIdSql/g) ?? []).toHaveLength(1);
    expect(approval).toContain('legalPointByIdSql("created_by","legal_documents")');
    expect(portal).toContain('legalPointByIdSql("id,name,body,version,checksum,media_type","legal_documents")');
    expect(portal).toContain('legalPointByIdSql("*","legal_portal_receipts")');
    expect(portal).toContain('legalPointByIdSql("company_id,branch_id,department_id,version","legal_items")');
    expect(approval).not.toContain('"SELECT created_by FROM legal_documents WHERE id=?"');
    expect(portal).not.toContain('"SELECT id,name,body,version,checksum,media_type FROM legal_documents WHERE id=?"');
    expect(portal).not.toContain('"SELECT * FROM legal_portal_receipts WHERE id=?"');
    expect(portal).not.toContain('"SELECT company_id,branch_id,department_id,version FROM legal_items WHERE id=?"');
  });

  it('leaves corpus/citation, locks and the r9/r16-r31 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts',
      'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts', 'legal/destruction-worker.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalDocumentByIdSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalPurchaseClockExistsSql', 'legalApprovedCycleSql',
      'legalHoldActiveSql', 'legalHoldTargetGuardPrefix', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalMemberEditExistsSql', 'legalEventsByItemSql',
      'legalOrderedHistorySql', 'legalRecentItemListSql',
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 16).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[16]).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(texts[17]).toContain('export function legalCitation');
    expect(texts[18]).toContain('legalCompanyAdvisoryLockSql');
    expect(texts[19]).toContain('FOR UPDATE OF x,i');
  });

  it('folds the two destruction-by-id row-lock reads into one byte-identical head', async () => {
    const worker = await src('legal/destruction-worker.ts');
    const approved =
      "SELECT x.*,i.company_id FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='approved' FOR UPDATE OF x,i";
    const executing =
      "SELECT x.*,i.company_id,i.version item_version FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='executing' FOR UPDATE OF x,i";
    const head = (projection: string, state: string) =>
      `SELECT ${projection} FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='${state}'`;
    const tail = ' FOR UPDATE OF x,i';
    expect(head('x.*,i.company_id', 'approved') + tail).toBe(approved);
    expect(head('x.*,i.company_id,i.version item_version', 'executing') + tail).toBe(executing);
    expect(worker).toContain("legalDestructionByIdLockSql('x.*,i.company_id','approved')+' FOR UPDATE OF x,i'");
    expect(worker).toContain("legalDestructionByIdLockSql('x.*,i.company_id,i.version item_version','executing')+' FOR UPDATE OF x,i'");
    expect(worker).not.toContain(approved);
    expect(worker).not.toContain(executing);
    expect((worker.match(/FOR UPDATE OF x,i/g) ?? []).length).toBe(2);
    expect((worker.match(/SELECT /g) ?? []).length).toBe(3);
    expect(head('x', 'approved')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=241', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(241);
    expect(total).toBeLessThanOrEqual(240);
  });
});
