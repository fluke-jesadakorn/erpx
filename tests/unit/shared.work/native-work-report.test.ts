import {describe,expect,it,vi} from 'vitest';

vi.mock('server-only',()=>({}));

import {
  NativeWorkReportError,
  nativeWorkReportConfigured,
  readNativeWorkReport,
  validateReportProjection,
  type NativeWorkReportTransport,
} from '../../../modules/work-projection/ui/src/lib/native-work-report';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44993',
  holdingSlug: 'acme',
  origin: 'https://acme.example.com',
  timeoutMs: 1000,
});
const actor = {
  companyId: 'company',
  assignmentId: 'assignment',
  revision: 7,
  assignmentVersion: 3,
  branchSlug: 'chiang-mai',
  departmentSlug: 'ops',
};
const input = {
  mode: 'open' as const,
  scoped: '[{"id":"one"}]',
  start: 0,
  end: 10,
  page: 1,
  exportAll: true,
};
const projection = {
  counts: {total: 2, open: 2, drafts: 0, unknown: 0, blocked: 1, units: 3.5},
  money: [{currency: 'THB', amount: 1000}],
  estimates: [{currency: 'THB', amount: 2000}],
  stageCounts: [{stage: 'Review', count: 2, oldest: 4}],
  rows: [{id: 'one'}],
  allRows: [{id: 'one'}, {id: 'two'}],
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {status: 200, headers: {'content-type': 'application/json'}});
const read = (transport: NativeWorkReportTransport) => {
  vi.stubEnv('ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG', config);
  return readNativeWorkReport(input, actor, transport);
};

describe('native Work report transport', () => {
  it('forwards the exact loopback target, selected context and scoped rows', async () => {
    const transport = vi.fn().mockResolvedValue(ok({data: projection}));
    const result = await read(transport);
    expect(result).toEqual(projection);
    const [url, headers, body] = transport.mock.calls[0]!;
    expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/work/native-report');
    expect(headers.host).toBe('acme.example.com');
    expect(headers.origin).toBe('https://acme.example.com');
    expect(headers['x-erpx-context-company']).toBe('company');
    expect(headers['x-erpx-context-assignment']).toBe('assignment');
    expect(headers['x-erpx-context-revision']).toBe('7');
    expect(headers['x-erpx-context-assignment-version']).toBe('3');
    expect(headers['x-erpx-workspace-branch']).toBe('chiang-mai');
    expect(headers['x-erpx-workspace-department']).toBe('ops');
    expect(JSON.parse(body)).toEqual({
      mode: 'open', scoped: input.scoped, start: 0, end: 10, page: 1, exportAll: true,
    });
  });

  it('fails closed on a non-200 response', async () => {
    const error = await read(vi.fn().mockResolvedValue(new Response(null, {status: 503}))).then(
      () => null,
      reason => reason,
    );
    expect(error).toBeInstanceOf(NativeWorkReportError);
    expect(error.status).toBe(503);
  });

  it('fails closed on an invalid projection instead of returning partial facts', async () => {
    const bad = {...projection, counts: {...projection.counts, total: -1}};
    const error = await read(vi.fn().mockResolvedValue(ok({data: bad}))).then(
      () => null,
      reason => reason,
    );
    expect(error).toBeInstanceOf(NativeWorkReportError);
  });

  it('keeps allRows empty unless export was requested', () => {
    const compact = validateReportProjection({...projection, allRows: []}, {...input, exportAll: false});
    expect(compact.allRows).toEqual([]);
    expect(compact.rows).toEqual([{id: 'one'}]);
  });

  it('reports configuration presence for the consumer switch', () => {
    vi.stubEnv('ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG', config);
    expect(nativeWorkReportConfigured()).toBe(true);
    vi.unstubAllEnvs();
    expect(
      process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG === undefined &&
        nativeWorkReportConfigured(),
    ).toBe(false);
  });
});
