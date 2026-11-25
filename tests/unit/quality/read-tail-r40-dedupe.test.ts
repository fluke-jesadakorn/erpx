import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { evidenceHead } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { qualitySourceArmHead } from '../../../packages/server-quality/src/adapters/quality-source-arm-head';
import { qualityReturnAnchorSql } from '../../../packages/server-quality/src/adapters/quality-return-anchor-sql';
import { qualityObligationFactArm } from '../../../packages/server-quality/src/adapters/quality-obligation-fact-arm-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const writer = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i;

// Byte oracles captured at HEAD before the r40 fold, rendered by the retired
// inline `SELECT ${columns} FROM ${source}${predicate}` in qualitySourceArmHead
// and by the two wrappers that share it (return anchors / obligation facts).
// The head now routes through evidenceHead, so every rendering, bind order and
// spacing must stay byte-identical to these digests. `head` is the exact
// evidenceHead(projection, from, tail) triple each rendering must equal.
type Arm = {
  name: string;
  render: () => string;
  head: [string, string, string];
  before: string;
  digest: string;
  binds: number;
};
const arms: Arm[] = [
  {
    name: 'plain source arm (no predicate)',
    render: () => qualitySourceArmHead('x', 'y'),
    head: ['x', ' FROM y', ''],
    before: 'SELECT x FROM y',
    digest: '08b4aef2127eac679fc3018ea289955a1faf6dd909d501cb9fb4b3f0f751909c',
    binds: 0,
  },
  {
    name: 'source arm with predicate bind',
    render: () => qualitySourceArmHead('x', 'y', ' WHERE z=?'),
    head: ['x', ' FROM y', ' WHERE z=?'],
    before: 'SELECT x FROM y WHERE z=?',
    digest: '2e9dbf22e2400d9b55900184a1d492f79d5989cf8ebd3e6b57e2a6c4be2e6836',
    binds: 1,
  },
  {
    name: 'inventory hold source rows',
    render: () => qualitySourceArmHead('"id", "company_id", "status"', 'inventory_quality_holds'),
    head: ['"id", "company_id", "status"', ' FROM inventory_quality_holds', ''],
    before: 'SELECT "id", "company_id", "status" FROM inventory_quality_holds',
    digest: '96e83c1e6528e2d433e55874f5afe40319447b5934810727c72a5439717284dd',
    binds: 0,
  },
  {
    name: 'alerts join source arm',
    render: () => qualitySourceArmHead('a.id,s.company_id,s.branch_id,NULL::text,s.kind', 'inventory_quality_alerts a JOIN (source_rows) s ON s.id=a.hold_id'),
    head: ['a.id,s.company_id,s.branch_id,NULL::text,s.kind', ' FROM inventory_quality_alerts a JOIN (source_rows) s ON s.id=a.hold_id', ''],
    before: 'SELECT a.id,s.company_id,s.branch_id,NULL::text,s.kind FROM inventory_quality_alerts a JOIN (source_rows) s ON s.id=a.hold_id',
    digest: '77e74d404da7c31c8ea752b5a86bdcfc7e9f450a10d896c40ba229c53076b503',
    binds: 0,
  },
  {
    name: 'return anchor over holds',
    render: () => qualityReturnAnchorSql('h.id,h.company_id source_company', 'inventory_quality_holds h', " WHERE h.source_type='customer_return'"),
    head: ['h.id,h.company_id source_company', ' FROM inventory_quality_holds h', " WHERE h.source_type='customer_return'"],
    before: "SELECT h.id,h.company_id source_company FROM inventory_quality_holds h WHERE h.source_type='customer_return'",
    digest: '8521756878774de260cb09449e9b570cf3cfd710b6638562ea25cd9fccf6be07',
    binds: 0,
  },
  {
    name: 'return anchor over origins',
    render: () => qualityReturnAnchorSql('origin.hold_id,origin.company_id', '(origins) origin', " WHERE origin.kind='customer-return'"),
    head: ['origin.hold_id,origin.company_id', ' FROM (origins) origin', " WHERE origin.kind='customer-return'"],
    before: "SELECT origin.hold_id,origin.company_id FROM (origins) origin WHERE origin.kind='customer-return'",
    digest: '7c7dccd6c6edd711c939d667b426bf1e821b4b41c8641590dfb1064b8dd5af68',
    binds: 0,
  },
  {
    name: 'obligation fact hold arm',
    render: () => qualityObligationFactArm("'hold' kind,hold.id,to_jsonb(hold)::text fact", 'holds h', " WHERE h.status NOT IN ('released','cancelled','disposed')"),
    head: ["'hold' kind,hold.id,to_jsonb(hold)::text fact", ' FROM holds h', " WHERE h.status NOT IN ('released','cancelled','disposed')"],
    before: "SELECT 'hold' kind,hold.id,to_jsonb(hold)::text fact FROM holds h WHERE h.status NOT IN ('released','cancelled','disposed')",
    digest: '733f71e37f12c1b55ba601b2946e6a9f7737780ed63967282455b11831431b0f',
    binds: 0,
  },
  {
    name: 'obligation fact alert arm',
    render: () => qualityObligationFactArm("'quality-alert',a.id,to_jsonb(a)::text", 'inventory_quality_alerts a JOIN holds h ON h.id=a.hold_id', ' WHERE a.acknowledged_by IS NULL'),
    head: ["'quality-alert',a.id,to_jsonb(a)::text", ' FROM inventory_quality_alerts a JOIN holds h ON h.id=a.hold_id', ' WHERE a.acknowledged_by IS NULL'],
    before: "SELECT 'quality-alert',a.id,to_jsonb(a)::text FROM inventory_quality_alerts a JOIN holds h ON h.id=a.hold_id WHERE a.acknowledged_by IS NULL",
    digest: '43d5c1f051b4796b50119f4d2792898e1843327a8b3412826b4521f0afa87eea',
    binds: 0,
  },
];

const kpi = (pattern: RegExp) => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(pattern) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

describe('quality read-tail r40 source-arm evidence head dedupe', () => {
  test.each(arms)('$name: byte-identical to the pre-dedupe rendering', (t) => {
    const sql = t.render();
    expect(sql).toBe(t.before);
    expect(sha(sql)).toBe(t.digest);
    expect(sql).toBe(evidenceHead(...t.head));
    expect(sql.startsWith('SELECT ')).toBe(true);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect((sql.match(/\?/g) ?? []).length).toBe(t.binds);
    expect(sql.match(/\$[0-9]/g)).toBeNull();
    expect(sql).not.toMatch(writer);
  });

  test('the helper delegates to evidenceHead and owns no inline SELECT', () => {
    const helper = read('adapters/quality-source-arm-head.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain('evidenceHead(columns, ` FROM ${source}`, predicate)');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(helper).not.toMatch(writer);
    const anchor = read('adapters/quality-return-anchor-sql.ts');
    const fact = read('adapters/quality-obligation-fact-arm-sql.ts');
    expect(anchor).toContain('qualitySourceArmHead(columns, source, predicate)');
    expect(fact).toContain('qualitySourceArmHead(columns, source, predicate)');
    expect(anchor.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(fact.match(/SELECT /g) ?? []).toHaveLength(0);
  });

  test('readers still consume the shared head with no residual inline arms', () => {
    const sites = [
      ['adapters/attachment-read-sources.ts', "import {qualitySourceArmHead} from './quality-source-arm-head';"],
      ['adapters/record-purchases.ts', 'import {qualitySourceArmHead} from "./quality-source-arm-head";'],
      ['adapters/sales-package-sources.ts', "import {qualityReturnAnchorSql} from './quality-return-anchor-sql';"],
      ['adapters/ecommerce-package-sources.ts', "import {qualityReturnAnchorSql} from './quality-return-anchor-sql';"],
      ['adapters/manufacturing-package-obligations.ts', "import {qualityObligationFactArm} from './quality-obligation-fact-arm-sql';"],
      ['adapters/supply-package-obligations.ts', "import {qualityObligationFactArm} from './quality-obligation-fact-arm-sql';"],
    ] as const;
    for (const [path, pin] of sites) expect(read(path)).toContain(pin);
    for (const t of arms) {
      expect(read('adapters/quality-source-arm-head.ts')).not.toContain(t.before);
      expect(read('adapters/quality-return-anchor-sql.ts')).not.toContain(t.before);
      expect(read('adapters/quality-obligation-fact-arm-sql.ts')).not.toContain(t.before);
    }
  });

  test('native registry, CAS guards and all protected locks are untouched', () => {
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_HOLD_CAS');
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_SPEC_CAS');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND company_id=? AND version=?)');
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('salesQualitySettlementSourceReadLocks');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=22 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBeLessThan(22);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(20);
    expect(kpi(/SELECT /g)).toBe(20);
  });
});
