import { describe, expect, it, vi } from 'vitest';
import { readFile } from 'node:fs/promises';

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

  it('keeps one canonical eligibility projection owner', async () => {
    const source = await src('adapters/support-incident-read.ts');
    expect(source).toContain('export const incidentEligibilitySelect');
    expect(source.match(/SELECT t\.\*,CASE WHEN \$\{ownerEligible\}/g)).toHaveLength(1);
    expect(source.match(/EXISTS\(SELECT 1 FROM \(\$\{identity_accessProjectionSources\.assignments\}\) m WHERE/g)).toHaveLength(1);
    expect(source.match(/\(SELECT COUNT\(DISTINCT m\.user_id\) FROM/g)).toHaveLength(1);
  });

  it('composes the projection in both incident readers with no local copy', async () => {
    const byId = await src('adapters/support-incident-read.ts');
    const work = await src('adapters/support-incidents.ts');
    expect(byId).toContain('${incidentEligibilitySelect(eligible, reviewer)}');
    expect(work).toContain('${incidentEligibilitySelect(eligible, reviewer)}');
    expect(work).not.toContain('owner_eligible');
    expect(work).not.toContain('reviewer_eligible');
  });

  it('re-exports the shared helper from the owner public surface', async () => {
    const surface = await src('public/support-incident-read.ts');
    expect(surface).toContain('incidentEligibilitySelect');
  });
});

describe('Support ticket read-tail r17 canonical eligibility projection', () => {
  const src = (name: string) =>
    readFile(new URL(`../../../packages/server-support/src/${name}`, import.meta.url), 'utf8');

  it('keeps one canonical ticket eligibility projection owner', async () => {
    const source = await src('adapters/support-ticket-read.ts');
    expect(source).toContain('export const ticketEligibilitySelect');
    expect(source.match(/SELECT t\.\*,\$\{extra\}CASE WHEN \$\{ownerEligible\}/g)).toHaveLength(1);
    expect(source.match(/CASE WHEN \$\{ownerEligible\} THEN 1 ELSE 0 END owner_eligible/g)).toHaveLength(1);
  });

  it('composes the projection in both ticket readers with no local copy', async () => {
    const byId = await src('adapters/support-ticket-read.ts');
    const work = await src('adapters/support.ts');
    expect(byId).toContain('${ticketEligibilitySelect(eligible, reviewers,');
    expect(work).toContain('${ticketEligibilitySelect(eligible, reviewers)}');
    expect(work).not.toContain('owner_eligible');
    expect(work).not.toContain('reviewer_count');
  });

  it('re-exports the shared helper from the owner public surface', async () => {
    const surface = await src('public/support-ticket-read.ts');
    expect(surface).toContain('ticketEligibilitySelect');
  });
});
