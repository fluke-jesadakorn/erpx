import { describe, expect, it, vi } from 'vitest';

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
