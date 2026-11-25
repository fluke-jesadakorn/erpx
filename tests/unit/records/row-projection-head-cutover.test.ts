import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  recordRowPageSql,
  recordRowSelectSql,
} from '../../../packages/server-records/src/adapters/records';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const records = read('packages/server-records/src/adapters/records.ts');
const shared = read('packages/server-records/src/adapters/read-tail-r32-sql.ts');

describe('Records row-projection head r12 cutover', () => {
  test('list/get projection renders byte-identically to the retired literal head', () => {
    const sql = recordRowSelectSql(
      ' WHERE archived_at IS NULL AND module_id=? AND section_id=? AND TRUE ORDER BY updated_at DESC,id',
    );
    expect(sql).toBe(
      'SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at\n    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND TRUE ORDER BY updated_at DESC,id',
    );
    expect(sha(sql)).toBe('e3d438fea0c1200524fcdf6e7cbef4f284d18954ca02747efa5a8c6f832a9639');
  });

  test('single-record get projection keeps its exact prior head', () => {
    const sql = recordRowSelectSql(' WHERE archived_at IS NULL AND module_id=? AND id=? AND TRUE');
    expect(sql).toBe(
      'SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at\n    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND id=? AND TRUE',
    );
    expect(sha(sql)).toBe('9390af8a9ede4475af5e0072070bada1bc84398b13bdbbe58932fe5c8b612fe3');
  });

  test('paged projection keeps the page_key column and exact prior head', () => {
    const sql = recordRowPageSql(
      " WHERE archived_at IS NULL AND module_id=? AND section_id=? AND TRUE AND strpos(lower(title||' '||COALESCE(description,'')),lower(?))>0",
    );
    expect(sql).toBe(
      "SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at,updated_at::text page_key FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND TRUE AND strpos(lower(title||' '||COALESCE(description,'')),lower(?))>0",
    );
    expect(sha(sql)).toBe('506914e9ba123b6a81611e307dea289c0b0e59e8cff03497bee015a4850f0d74');
  });

  test('the replaced SQL literal is deleted and all three callers delegate to r32', () => {
    expect(records).not.toMatch(/SELECT /);
    expect(records).toContain("import { projectionColumnHeadSql } from './read-tail-r32-sql';");
    expect(records).toContain('const recordRowSelectSql = (suffix: string) =>');
    expect(records).toContain('const recordRowPageSql = (suffix: string) =>');
    expect(records).toContain('projectionColumnHeadSql(`${recordRowColumns}\\n   `, \'workspace_records\', suffix)');
    expect(records).toContain('projectionColumnHeadSql(`${recordRowColumns},updated_at::text page_key`, \'workspace_records\', suffix)');
    expect(records.match(/database\.prepare\(recordRowSelectSql\(/g) ?? []).toHaveLength(2);
    expect(records.match(/sql: recordRowPageSql\(/g) ?? []).toHaveLength(1);
    expect(records).not.toContain('FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND ${commerce.where}');
    expect(shared.split('`SELECT ${columns} FROM ${table}${tail}`')).toHaveLength(2);
  });

  test('owner retirement:ts-statements drops 12 -> 11', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, 'packages/server-records/src'));
    expect(total).toBe(7);
    expect(total).toBeLessThan(12);
  });
});
