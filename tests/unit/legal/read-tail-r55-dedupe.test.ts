import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { finalRootOwnerProjectionSources } from '../../../packages/server-legal/src/adapters/final-root-owner-projection-sources';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-legal/src/adapters/hospitality-owner-projection-sources';
import { collaborationNotificationSnapshotSources } from '../../../packages/server-legal/src/adapters/collaboration-notification-snapshot-sources';
import { legalMembersSnapshotSource } from '../../../packages/server-legal/src/adapters/legal-members-read-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/${name}`, import.meta.url), 'utf8');

/** Independent byte + SHA oracles for the 11 projection literals captured at
 * HEAD (BEFORE=163) before they were routed through the closed renderer. */
const ORIGINALS = [
  ['legal_events', 'SELECT "id","item_id","action","actor_id","note","version","created_at","snapshot" FROM legal_events', '94f2ac96797311c51e971312ccd48a004bf2251281ccd76a10b834af0e2f1149'],
  ['legal_items(tight)', 'SELECT "id","company_id","branch_id","department_id","kind","title","description","classification","country","jurisdiction","language","currency","timezone","priority","due_date","owner_id","created_by","state","version","created_at","updated_at","escalation_owner","sla_started_at","sla_due_at","intake_form_id","intake_answers" FROM legal_items', '88636c83d5cc324be45b78ed3cbcefc6d63bcf0e2e6fc770d6c460555e2f7830'],
  ['legal_native_references', 'SELECT "item_id","target_module","target_id","ordinal","created_by","created_at" FROM legal_native_references', 'd9f357524ba2d810178f2565207f363bad51b0d0af5a855a96c72f126a68b5f4'],
  ['legal_documents', 'SELECT "id","item_id","version","name","media_type","checksum","bytes","body","object_key","locked","created_by","created_at","attachment_link_id","attachment_source_id" FROM legal_documents', 'a1f51e7ed90e8f518b9fe58411b9baafdacd4d46acd9a0d8d8c8b45592df7b01'],
  ['legal_file_preservations', 'SELECT "object_key","item_id","source_type","source_id","checksum","created_by","created_at" FROM legal_file_preservations', 'cb3720a4986077b8cdc34dcbe9881c86fa4398340bb32c718fefbd10de0cf588'],
  ['legal_hold_targets', 'SELECT "hold_id","item_id" FROM legal_hold_targets', 'f5d1b5c3ae433b3553612cd24738e240b86e64edd5abfef26724e526ee319c84'],
  ['legal_holds', 'SELECT "id","item_id","reason","state","created_by","release_requested_by","released_by","created_at","released_at" FROM legal_holds', 'bdb78eec0e47f15de8d69c90ab4c5af272d280fae297ef396934c0c3aa75d274'],
  ['legal_items(spaced)', 'SELECT "id", "company_id", "branch_id", "department_id", "kind", "title", "description", "classification", "country", "jurisdiction", "language", "currency", "timezone", "priority", "due_date", "owner_id", "created_by", "state", "version", "created_at", "updated_at", "escalation_owner", "sla_started_at", "sla_due_at", "intake_form_id", "intake_answers" FROM legal_items', '0754ac381c2f3abb10985f5e417c74a42ae4ebe87639b165f9a0dc8d571b873f'],
  ['legal_settings', 'JOIN (SELECT "company_id", "enabled", "ai_enabled", "version", "reminder_days" FROM legal_settings) s', '9501815ea2f05c1752bc753fa242f18c61c5c468d4ab5edd80f384f6fb4bfc5b'],
  ['legal_obligations', 'JOIN (SELECT "item_id", "data" FROM legal_obligations) o', 'ab0583390b69646535b87144a4cf4698ffd0343f4b51a50ccb77fdfbbcb989fd'],
  ['legal_members', 'FROM (SELECT "item_id", "user_id", "access" FROM legal_members) m', 'e9ed1acbc15020d2231623265b2038ae49ab3bb97ed1f9fe69f00af74b9db811'],
] as Array<[string, string, string]>;

const n = collaborationNotificationSnapshotSources();
const spacedItems = n.legal_hold_notifications_legal_items_i_join.slice('JOIN ('.length, -') i'.length);
const ACTUAL: Record<string, string> = {
  legal_events: finalRootOwnerProjectionSources.legal_events,
  'legal_items(tight)': finalRootOwnerProjectionSources.legal_items,
  legal_native_references: finalRootOwnerProjectionSources.legal_native_references,
  legal_documents: hospitalityOwnerProjectionSources.legal_documents,
  legal_file_preservations: hospitalityOwnerProjectionSources.legal_file_preservations,
  legal_hold_targets: hospitalityOwnerProjectionSources.legal_hold_targets,
  legal_holds: hospitalityOwnerProjectionSources.legal_holds,
  'legal_items(spaced)': spacedItems,
  legal_settings: n.legal_reminder_notifications_legal_settings_s_join,
  legal_obligations: n.legal_reminder_notifications_legal_obligations_o_join,
  legal_members: legalMembersSnapshotSource,
};

describe('legal read-tail r55 closed projection dedupe', () => {
  it('renders all 11 replacements byte-identically to the HEAD oracles', () => {
    expect(ORIGINALS).toHaveLength(11);
    for (const [name, value, digest] of ORIGINALS) {
      expect(ACTUAL[name], name).toBe(value);
      expect(sha256(ACTUAL[name]), name).toBe(digest);
    }
  });

  it('preserves both separator families and every wrapper', () => {
    const items = ORIGINALS[7][1];
    expect(n.legal_hold_notifications_legal_items_i_join).toBe(`JOIN (${items}) i`);
    expect(n.legal_law_source_notifications_legal_items_i_from).toBe(`FROM (${items}) i`);
    expect(n.legal_reminder_notifications_legal_items_i_join).toBe(`JOIN (${items}) i`);
    expect(n.legal_reminder_notifications_legal_items_i_from).toBe(`FROM (${items}) i`);
    expect(n.legal_law_source_notifications_legal_members_m_from).toBe(legalMembersSnapshotSource);
    expect(n.legal_reminder_notifications_legal_members_m_from).toBe(legalMembersSnapshotSource);
  });

  it('routes every projection through one closed renderer with no inline literal left', async () => {
    const names = [
      'adapters/closed-projection-source.ts',
      'adapters/final-root-owner-projection-sources.ts',
      'adapters/hospitality-owner-projection-sources.ts',
      'adapters/collaboration-notification-snapshot-sources.ts',
      'adapters/legal-members-read-sources.ts',
    ];
    const texts = await Promise.all(names.map(src));
    expect(texts[0].match(/SELECT /g) ?? []).toHaveLength(1);
    expect(texts[0]).toContain('cols.map((c) => `"${c}"`).join(sep)');
    for (const text of texts.slice(1)) {
      expect(text).toContain("from './closed-projection-source'");
      expect(text).not.toContain('SELECT \\"');
    }
    expect(texts[3]).toContain("from './legal-members-read-sources'");
    expect(texts[4]).toContain("legalMembersWhereHead('item_id=? AND user_id=?')");
  });

  it('keeps the public re-exports and sibling consumers unchanged', async () => {
    const [fr, ho, cn, identity, signer] = await Promise.all([
      src('public/final-root-owner-projection-sources.ts'),
      src('public/hospitality-owner-projection-sources.ts'),
      src('public/collaboration-notification-snapshot-sources.ts'),
      src('adapters/identity-snapshot-sources.ts'),
      src('adapters/legal-signer-access-sources.ts'),
    ]);
    expect(fr).toContain('export { finalRootOwnerProjectionSources } from "@erpx/server-legal/adapters/final-root-owner-projection-sources";');
    expect(ho).toContain('export { hospitalityOwnerProjectionSources } from "@erpx/server-legal/adapters/hospitality-owner-projection-sources";');
    expect(cn).toContain("export {collaborationNotificationSnapshotSources} from '@erpx/server-legal/adapters/collaboration-notification-snapshot-sources';");
    expect(identity).toContain("legalMembersSnapshotSource");
    expect(signer).toContain("legalMembershipSql");
  });

  it('keeps public-path values identical for server-files and customer_360 consumers', async () => {
    const hoPub = await import('@erpx/server-legal/public/hospitality-owner-projection-sources');
    const frPub = await import('@erpx/server-legal/public/final-root-owner-projection-sources');
    expect(hoPub.hospitalityOwnerProjectionSources.legal_documents).toBe(ORIGINALS[3][1]);
    expect(hoPub.hospitalityOwnerProjectionSources.legal_file_preservations).toBe(ORIGINALS[4][1]);
    expect(hoPub.hospitalityOwnerProjectionSources.legal_hold_targets).toBe(ORIGINALS[5][1]);
    expect(hoPub.hospitalityOwnerProjectionSources.legal_holds).toBe(ORIGINALS[6][1]);
    expect(frPub.finalRootOwnerProjectionSources.legal_events).toBe(ORIGINALS[0][1]);
    expect(frPub.finalRootOwnerProjectionSources.legal_items).toBe(ORIGINALS[1][1]);
    expect(frPub.finalRootOwnerProjectionSources.legal_native_references).toBe(ORIGINALS[2][1]);
  });

  it('leaves advisory, FOR UPDATE OF, LOCK TABLE and command_receipts text intact', async () => {
    const [r9, worker, retention, purchases] = await Promise.all([
      src('adapters/legal-read-tail-r9-sources.ts'), src('adapters/legal/destruction-worker.ts'),
      src('adapters/retention-execution.ts'), src('adapters/purchases.ts'),
    ]);
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(retention).toContain('LOCK TABLE legal_settings,legal_retention_execution_authorizations');
    expect(purchases).toContain('command_receipts');
  });

  it('retirement:ts-statements is BEFORE=163 -> AFTER=153', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(163);
    expect(total).toBeLessThanOrEqual(153);
  });
});
