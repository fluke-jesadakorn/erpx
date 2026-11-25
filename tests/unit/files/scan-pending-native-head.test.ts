import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { scanPendingSql } from '../../../packages/server-files/src/adapters/scan-pending-dedupe-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');

/** The retired TypeScript literal, pinned byte-exact from before the cut. */
const RETIRED = (ref: string) =>
  `EXISTS(SELECT 1 FROM attachment_scan_attempts scan WHERE scan.attachment_id=${ref}.id AND scan.state IN ('pending','running','error'))`;

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files scan-pending bind witness is native-owned', () => {
  test('the TS helper renders the retired statement byte-exactly from the contract', () => {
    for (const ref of ['a', 'source', 'attachment']) {
      expect(scanPendingSql(ref)).toBe(RETIRED(ref));
    }
    expect(scanPendingSql('a')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
  });

  test('no statement token remains in the TypeScript helper', () => {
    const helper = src('adapters/scan-pending-dedupe-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(helper).not.toContain(token);
    expect(helper).toContain('scan-pending-head-sources.json');
    expect(helper).toContain('import contract from');
    expect(helper).toContain('head.replace("{attachmentRef}", attachmentRef)');
    expect(helper).not.toContain('prepare(');
    expect(helper).not.toContain('.bind(');
  });

  test('the statement token lives in the Rust-owned contract and native port', () => {
    const json = JSON.parse(src('adapters/scan-pending-head-sources.json'));
    expect(json.scanPending).toBe(RETIRED('{attachmentRef}'));
    const rust = src('adapters/scan_pending_head.rs');
    expect(rust).toContain('include_str!("scan-pending-head-sources.json")');
    expect(rust).toContain('pub fn scan_pending_head()');
    expect(rust).toContain('pub fn render_scan_pending(attachment_ref: &str)');
    expect(src('public/scan_pending_head.rs')).toContain('pub use crate::adapters::scan_pending_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod scan_pending_head;');
    expect(src('public/mod.rs')).toContain('pub mod scan_pending_head;');
  });

  test('upload scan bind primary witness stays reachable from the three package-obligation readers', () => {
    for (const rel of [
      'adapters/ecommerce-package-obligation-sources.ts',
      'adapters/marketing-package-obligation-sources.ts',
      'adapters/performance-package-obligation-sources.ts',
    ]) {
      const text = src(rel);
      expect(text).toContain("from './scan-pending-dedupe-sql'");
      expect(text).toContain('${scanPendingSql(');
    }
  });

  test('retirement:ts-statements is 207, strictly below the scan-pending BEFORE=210 baseline', () => {
    let literal = 0, total = 0, broad = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
      broad += (text.match(/\b(SELECT|INSERT|UPDATE|DELETE)\b/g) ?? []).length;
    }
    expect(literal).toBe(207);
    expect(total).toBe(208);
    expect(broad).toBe(226);
    expect(literal).toBeLessThan(210);
    expect(total).toBeLessThan(211);
  });
});
