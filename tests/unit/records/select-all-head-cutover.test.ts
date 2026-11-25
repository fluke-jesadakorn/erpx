import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { selectAllWhereSql } from '../../../packages/server-records/src/adapters/read-tail-select-all';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const shared = read('packages/server-records/src/adapters/read-tail-select-all.ts');

const oracles = [
  ['employeeRequestByRow', 'employee_requests', 'id=? AND company_id=?', '674e1a56dee56828348221ec92c70be361f46d29144f2aad326153215b3cbc42'],
  ['requestDocument', 'employee_requests', 'company_id=? AND id', '9683c80e5487bd59307309a77d7d227d0e946923fb0b5fea520924de325b5fb6'],
  ['recordDocument', 'workspace_records', 'company_id=? AND id', '0b57a96c14a79c5e008247d70b8dc178c20bea2c080f964f3bbfd1b889609a31'],
] as const;

describe('Records selectAllWhereSql r20 cutover', () => {
  test.each(oracles)('%s renders byte-identically through the shared r32 head', (_name, table, predicate, expected) => {
    const actual = selectAllWhereSql(table, predicate);
    expect(actual).toBe(`SELECT * FROM ${table} WHERE ${predicate}`);
    expect(sha(actual)).toBe(expected);
    expect(actual).toContain('SELECT * FROM ');
    expect(actual).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the replaced literal is deleted and both consumers delegate', () => {
    expect(shared).not.toMatch(/SELECT /);
    expect(shared).toContain("import { projectionColumnHeadSql } from './read-tail-r32-sql';");
    expect(shared).toContain("projectionColumnHeadSql('*', table, ` WHERE ${predicate}`)");
    for (const consumer of ['employee-requests-read.ts', 'workflow-document-sources.ts']) {
      const source = read(`packages/server-records/src/adapters/${consumer}`);
      expect(source).toContain('selectAllWhereSql(');
      expect(source).not.toContain('SELECT * FROM');
    }
    expect(read('packages/server-records/src/adapters/workflow-document-sources.ts')).not.toMatch(/SELECT /);
  });

  test('owner retirement:ts-statements drops 9 -> 8', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, 'packages/server-records/src'));
    expect(total).toBe(6);
    expect(total).toBeLessThan(9);
  });
});
