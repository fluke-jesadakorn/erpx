import { describe, expect, it, vi } from 'vitest';
import { readFile } from 'node:fs/promises';
import { readFileSync, readdirSync } from 'node:fs';

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/support-incident-read', () => ({
  getSupportIncident: vi.fn(),
  supportIncidentReadPredicate: vi.fn(),
}));

import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createSupportIncidentReadClient,
  projectSupportIncidentRead,
  type SupportIncidentRead,
} from '@erpx/server-support/public/support-incident-read';

const actor = { companyId: 'company' } as WorkContext;
const row: SupportIncidentRead = {
  id: 'incident-1',
  version: 3,
  status: 'investigating',
  title: 'Checkout outage',
  href: '/support/incidents/incident-1',
};

describe('bounded Support incident read client', () => {
  it('projects exactly the five consumer fields', () => {
    expect(projectSupportIncidentRead({ ...row })).toEqual(row);
    expect(Object.keys(projectSupportIncidentRead({ ...row })).sort()).toEqual(
      ['href', 'id', 'status', 'title', 'version'],
    );
  });

  it('rejects wider, malformed and misleading projections', () => {
    for (const value of [
      { ...row, severity: 'critical' },
      { ...row, version: 0 },
      { ...row, status: 'unknown' },
      { ...row, title: '' },
      { ...row, href: '/elsewhere' },
      { id: 'incident-1' },
      null,
    ]) {
      expect(() => projectSupportIncidentRead(value)).toThrowError(ApiError);
    }
  });

  it('validates the id before dispatch and projects the native result', async () => {
    const read = vi.fn().mockResolvedValue({ ...row });
    const client = createSupportIncidentReadClient(read);
    expect(await client(actor, 'incident-1')).toEqual(row);
    expect(read).toHaveBeenCalledWith(actor, 'incident-1');
    read.mockClear();
    const invalid = await client(actor, '').catch((value) => value);
    expect(invalid).toBeInstanceOf(ApiError);
    expect(invalid).toMatchObject({ status: 422, code: 'SUPPORT_INCIDENT_ID_INVALID' });
    expect(read).not.toHaveBeenCalled();
  });

  it('surfaces invalid native rows as closed owner errors', async () => {
    const client = createSupportIncidentReadClient(vi.fn().mockResolvedValue(null));
    const error = await client(actor, 'incident-1').catch((value) => value);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({ status: 503, code: 'NATIVE_SUPPORT_INCIDENT_READ_INVALID' });
  });
});

describe('Support incident read-tail r16 canonical projection', () => {
  const src = (name: string) =>
    readFile(new URL(`../../../packages/server-support/src/${name}`, import.meta.url), 'utf8');

  it('retires the incident eligibility projection wrapper', async () => {
    const source = await src('adapters/support-incident-read.ts');
    expect(source).not.toContain('incidentEligibilitySelect');
    expect(source).not.toContain('eligibilitySelect');
    expect(source).toContain('supportIncidentReadPredicate');
    expect(source).toContain('nativeSupportReadFirst<Row>(SUPPORT_RD_INCIDENT_BY_ID');
    expect(source).not.toMatch(/EXISTS\(SELECT 1 FROM \(\$\{identity_accessProjectionSources\.assignments\}\) m WHERE/);
    expect(source).not.toMatch(/\(SELECT COUNT\(DISTINCT m\.user_id\) FROM/);
  });

  it('renders the byte-identical eligibility fragments from one shared owner', async () => {
    const shared = await src('adapters/eligibility-projection-sources.ts');
    expect(shared.match(/EXISTS\(SELECT 1 FROM \(\$\{identity_accessProjectionSources\.assignments\}\) m WHERE/g)).toHaveLength(1);
    expect(shared.match(/\(SELECT COUNT\(DISTINCT m\.user_id\) FROM/g)).toHaveLength(1);
    expect(shared).toContain('supportEligibleFlagSql');
    expect(shared).toContain('supportReviewerCountSql');
    expect(shared).not.toContain('export const eligibilitySelect');
  });

  it('composes the shared eligibility fragments in every remaining TS Support read tail', async () => {
    const names = [
      'adapters/support-ticket-read-sources.ts',
      'adapters/support-sla.ts',
    ];
    for (const name of names) {
      const text = await src(name);
      expect(text, name).not.toMatch(/EXISTS\(SELECT 1 FROM \(\$\{identity_accessProjectionSources\.assignments\}\) m WHERE/);
      expect(text, name).not.toMatch(/\(SELECT COUNT\(DISTINCT m\.user_id\) FROM/);
    }
    expect(await src('adapters/support-ticket-read-sources.ts')).toContain('supportReviewerCountSql(');
    const sla = await src('adapters/support-sla.ts');
    expect(sla).toContain('SUPPORT_RD_SLA_POLICY_WORKSPACE');
    expect(sla).toContain('SUPPORT_RD_SLA_POLICY_WORK');
    expect(sla).not.toContain('supportEligibleFlagSql(');
    expect(sla).not.toContain('supportReviewerCountSql(');
    const alerts = await src('adapters/support-sla-alerts.ts');
    expect(alerts).toContain('SUPPORT_RD_SLA_ALERTS');
  });

  it('retirement:ts-statements stays strictly below the r17 BEFORE=91 baseline', () => {
    const root = new URL('../../../packages/server-support/src/', import.meta.url);
    let total = 0;
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const child = new URL(`${entry.name}${entry.isDirectory() ? '/' : ''}`, dir);
        if (entry.isDirectory()) walk(child);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(child, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(91);
  });

  it('composes the projection in both incident readers with no local copy', async () => {
    const byId = await src('adapters/support-incident-read.ts');
    const work = await src('adapters/support-incidents.ts');
    expect(byId).toContain('nativeSupportReadFirst<Row>(SUPPORT_RD_INCIDENT_BY_ID, [a.userId, id, ...args(a), ...supportIncidentReadPredicate(a).bindings], write)');
    expect(work).toContain('nativeSupportReadAll<Row>(SUPPORT_RD_INCIDENT_WORK, [a.userId, ...args(a), ...permission.bindings], write)');
    expect(work).not.toContain('owner_eligible');
    expect(work).not.toContain('reviewer_eligible');
  });

  it('no longer re-exports the retired helper from the owner public surface', async () => {
    const surface = await src('public/support-incident-read.ts');
    expect(surface).not.toContain('incidentEligibilitySelect');
  });
});

describe('Support ticket read-tail r17 canonical eligibility projection', () => {
  const src = (name: string) =>
    readFile(new URL(`../../../packages/server-support/src/${name}`, import.meta.url), 'utf8');

  it('retires the ticket eligibility projection wrapper', async () => {
    const source = await src('adapters/support-ticket-read.ts');
    expect(source).not.toContain('ticketEligibilitySelect');
    expect(source).not.toContain('eligibilitySelect');
    expect(source).toContain('supportTicketReadPredicate');
    expect(source).not.toMatch(/CASE WHEN \$\{ownerEligible\} THEN 1 ELSE 0 END owner_eligible/g);
  });

  it('keeps the by-id reader native and routes the work list through its native kind', async () => {
    const byId = await src('adapters/support-ticket-read.ts');
    const work = await src('adapters/support.ts');
    expect(byId).toContain('nativeSupportReadFirst<Row>(SUPPORT_RD_TICKET_BY_ID, [id, ...args(a), ...supportTicketReadPredicate(a).bindings], write)');
    expect(work).toContain('nativeSupportReadAll<Row>(SUPPORT_RD_TICKET_WORK, [...args(a), ...supportTicketReadPredicate(a).bindings], write)');
    expect(work).not.toContain('${ticketEligibilitySelect(eligible, reviewers)}');
    expect(work).not.toContain('owner_eligible');
    expect(work).not.toContain('reviewer_count');
  });

  it('no longer re-exports the retired helper from the owner public surface', async () => {
    const surface = await src('public/support-ticket-read.ts');
    expect(surface).not.toContain('ticketEligibilitySelect');
  });
});

describe('Support evidence read-tail r18 canonical eligibility witness', () => {
  const src = (name: string) =>
    readFile(new URL(`../../../packages/server-support/src/${name}`, import.meta.url), 'utf8');

  it('keeps one canonical evidence eligibility witness projection', async () => {
    const source = await src('adapters/support-evidence.ts');
    expect(source).toContain('const evidenceEligibilitySelect = () =>');
    expect(source.match(/ticketByIdSql\('1 AS ok', eligible\.replaceAll/g)).toHaveLength(1);
    const shared = await src('adapters/ticket-guard-sources.ts');
    expect(shared.match(/SELECT \$\{projection\} FROM support_tickets t WHERE t\.id=\? AND/g)).toHaveLength(1);
    expect(source.match(/eligible\.replaceAll\('t\.owner_id', '\?::text'\)/g)).toHaveLength(2);
  });

  it('composes the witness in both evidence readers with no local copy', async () => {
    const source = await src('adapters/support-evidence.ts');
    expect(source.match(/write\.prepare\(evidenceEligibilitySelect\(\)\)/g)).toHaveLength(2);
    expect(source).not.toContain("prepare(`SELECT 1 AS ok FROM support_tickets");
  });
});

describe('Support ticket-portal image read-tail r17 parameterized-family dedupe', () => {
  const src = (name: string) =>
    readFile(new URL(`../../../packages/server-support/src/${name}`, import.meta.url), 'utf8');

  it('renders byte-identical bound-row image SQL from one canonical helper', async () => {
    const shared = await src('adapters/ticket-portal-image-sql.ts');
    expect(shared.match(/readTailR44ImageAggregateSql\(/g)).toHaveLength(2);
    expect(shared).toContain('readTailR44ImageAggregateSql(alias, `${alias}.${orderBy}`, `${table} ${alias} WHERE EXISTS(SELECT 1 FROM selected binding WHERE ${predicate})`)');
    expect(shared).toContain('portalBoundImageSql');
    expect(shared).toContain('portalImageSelectedCte');
    expect(shared).toContain('portalBindingImageSql');
  });

  it('composes the shared image fragments in both portal image readers with no local copy', async () => {
    const source = await src('adapters/ticket-portal-sources.ts');
    expect(source.match(/portalImageSelectedCte\(bindingIds\)/g)).toHaveLength(2);
    expect(source.match(/portalBindingImageSql\(\)/g)).toHaveLength(2);
    expect(source.match(/portalBoundImageSql\(/g)).toHaveLength(9);
    expect(source).not.toContain('jsonb_agg(to_jsonb(binding) ORDER BY binding.id)');
    expect(source).not.toContain('EXISTS(SELECT 1 FROM selected binding WHERE binding.id=');
  });

  it('retirement:ts-statements stays strictly below the r17 BEFORE=86 baseline', () => {
    const root = new URL('../../../packages/server-support/src/', import.meta.url);
    let total = 0;
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const child = new URL(`${entry.name}${entry.isDirectory() ? '/' : ''}`, dir);
        if (entry.isDirectory()) walk(child);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(child, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(86);
  });
});
