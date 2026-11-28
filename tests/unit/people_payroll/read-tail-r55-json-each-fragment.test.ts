import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import {
  ppRd55AttachmentPresenceV1,
  ppRd55AttachmentPresenceV2,
} from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r55-sources';

const module = readFileSync(
  new URL('../../../modules/people-payroll/server/src/adapters/pp-read-tail-r55-sources.ts', import.meta.url),
  'utf8',
);
const fragment = ' IN(SELECT value FROM erpx_json_each(?))';

describe('People Payroll r55 json-each list predicate family', () => {
  test('the byte-identical fragment has exactly one owner-owned definition', () => {
    expect((module.match(/IN\(SELECT value FROM erpx_json_each\(\?\)\)/g) ?? []).length).toBe(1);
    expect(module).toContain(`const JSON_EACH_IN = '${fragment}';`);
    expect(module.match(/JSON_EACH_IN/g)?.length).toBe(3);
  });

  test('V1 and V2 render the original SQL byte-identically with the shared fragment', () => {
    for (const [name, render, column] of [
      ['V2', (json: boolean) => ppRd55AttachmentPresenceV2('probe', false, false, json), 'a.owner_id=?'],
      ['V1', (json: boolean) => ppRd55AttachmentPresenceV1('probe', false, false, json), 'owner_id=?'],
    ] as const) {
      const scalar = render(false);
      const listed = render(true);
      expect(scalar, name).toContain(column);
      expect(listed, name).toBe(scalar.replace(column, column.replace('=?', fragment)));
      expect((listed.match(/IN\(SELECT value FROM erpx_json_each\(\?\)\)/g) ?? []).length, name).toBe(1);
      expect(listed).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the family fold lowers the owner SELECT count below 1151', () => {
    const root = new URL('../../../modules/people-payroll/server/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) files.push(path);
      }
    };
    walk(root);
    const total = files.reduce((n, path) => n + (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1151);
    expect(total).toBeLessThanOrEqual(1150);
  });
});
