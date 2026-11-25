import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-runtime/public/http', () => ({
  ApiError: class ApiError extends Error {
    constructor(
      readonly status: number,
      readonly code: string,
      message: string,
    ) {
      super(message);
    }
  },
}));

import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createWorkEvidenceClient,
  nativeWorkEvidenceConfigured,
  parseWorkEvidenceConfig,
  readNativeWorkEvidence,
  workEvidenceHttpTransport,
  type WorkEvidenceActor,
  type WorkEvidenceTransport,
} from '../../../packages/server-files/src/adapters/native-lane-22/client';
import {
  createPersonnelEvidenceClient,
  type PersonnelEvidenceArgs,
  type PersonnelEvidenceKind,
} from '../../../packages/server-files/src/adapters/native-lane-22/personnel-client';

const ENV = 'ERPX_NATIVE_FILES_CONFIG';

/** Exact development value of `ERPX_NATIVE_FILES_CONFIG`. */
const DEV_CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    {
      origin: 'https://acme.jesadakorn.com',
      host: 'acme.jesadakorn.com',
      holdingId: 'h001',
      holdingSlug: 'acme',
    },
    {
      origin: 'https://northstar.jesadakorn.com',
      host: 'northstar.jesadakorn.com',
      holdingId: 'h002',
      holdingSlug: 'northstar',
    },
    {
      origin: 'https://erpx-internal.jesadakorn.com',
      host: 'erpx-internal.jesadakorn.com',
      holdingId: 'h003',
      holdingSlug: 'erpx',
    },
  ],
};

const config = (overrides: Record<string, unknown> = {}) => structuredClone({ ...DEV_CONFIG, ...overrides });
const actor = (overrides: Record<string, unknown> = {}) =>
  ({ holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-1', ...overrides }) as WorkEvidenceActor;
const hash = (value: string) => createHash('sha256').update(value).digest('hex');

const rows = [
  {
    id: 'link-1',
    set_id: 'set-1',
    purpose_key: 'medical_certificate',
    original_name: 'certificate.pdf',
    bytes: 4096,
    media_type: 'application/pdf',
    source_state: 'available',
    link_state: 'bound',
    preview_available: true,
    created_at: 1_760_000_000_000,
  },
  {
    id: 'link-2',
    set_id: 'set-2',
    purpose_key: 'medical_certificate',
    original_name: 'second.pdf',
    bytes: null,
    media_type: null,
    source_state: null,
    link_state: 'draft',
    preview_available: false,
    created_at: 1_760_000_000_001,
  },
];
const payload = { kind: 'sets', rows };
const db = {} as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
const ok: WorkEvidenceTransport = async () => ({ status: 200, value: payload });

async function expect503(call: () => Promise<unknown>, code?: string) {
  try {
    await call();
    throw new Error('expected a fail-closed ApiError');
  } catch (error) {
    expect(error).toBeInstanceOf(ApiError);
    const api = error as ApiError;
    expect(api.status).toBe(503);
    if (code) expect(api.code).toBe(code);
  }
}

beforeEach(() => {
  process.env[ENV] = JSON.stringify(DEV_CONFIG);
});
afterEach(() => {
  delete process.env[ENV];
});

describe('native Files work-evidence lane 22 contract', () => {
  it('pins the exact dev config and the caller-selected Holding route', async () => {
    expect(DEV_CONFIG).toEqual({
      privateBaseUrl: 'http://127.0.0.1:3198',
      timeoutMs: 10000,
      holdings: [
        { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
        { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
        { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
      ],
    });
    expect(nativeWorkEvidenceConfigured()).toBe(true);
    expect(parseWorkEvidenceConfig(DEV_CONFIG).holdings).toHaveLength(3);

    let url: URL | undefined;
    let headers: Record<string, string> | undefined;
    const result = await createWorkEvidenceClient(config(), async (sentUrl, sentHeaders) => {
      url = sentUrl;
      headers = sentHeaders;
      return { status: 200, value: payload };
    }).read(actor(), db, 'leave-1', 'sets', true);
    expect(result.results).toEqual(rows);
    expect(result.success).toBe(true);
    expect(result.meta).toEqual({ changes: 0, duration: 0 });
    expect(url?.toString()).toBe('http://127.0.0.1:3198/api/v1/holdings/acme/files/work-evidence?kind=sets');
    expect(headers?.host).toBe('acme.jesadakorn.com');
    expect(headers?.origin).toBe('https://acme.jesadakorn.com');
    expect(headers?.['x-erpx-context-company']).toBe('company-1');
    expect(headers?.['x-erpx-context-record']).toBe('leave-1');
    expect(headers?.['x-erpx-files-read-only']).toBe('true');
  });

  it('selects all three registered Holdings by holdingId + holdingSlug', async () => {
    for (const [holdingId, holdingSlug, host] of [
      ['h001', 'acme', 'acme.jesadakorn.com'],
      ['h002', 'northstar', 'northstar.jesadakorn.com'],
      ['h003', 'erpx', 'erpx-internal.jesadakorn.com'],
    ]) {
      let url: URL | undefined;
      let headers: Record<string, string> | undefined;
      await createWorkEvidenceClient(config(), async (sentUrl, sentHeaders) => {
        url = sentUrl;
        headers = sentHeaders;
        return { status: 200, value: payload };
      }).read(actor({ holdingId, holdingSlug }), db, 'leave-1', 'sets', false);
      expect(url?.pathname).toBe(`/api/v1/holdings/${holdingSlug}/files/work-evidence`);
      expect(headers?.host).toBe(host);
    }
  });

  it('selects by origin but rejects a conflicting second selector', async () => {
    let url: URL | undefined;
    await createWorkEvidenceClient(config(), async (sentUrl) => {
      url = sentUrl;
      return { status: 200, value: { kind: 'sets', rows } };
    }).read(actor({ holdingId: undefined, holdingSlug: undefined, origin: 'https://erpx-internal.jesadakorn.com' }), db, 'leave-1', 'sets', false);
    expect(url?.pathname).toBe('/api/v1/holdings/erpx/files/work-evidence');

    await expect503(
      () => createWorkEvidenceClient(config(), ok).read(
        actor({ origin: 'https://northstar.jesadakorn.com' }),
        db,
        'leave-1',
        'sets',
        false,
      ),
      'NATIVE_WORK_EVIDENCE_TARGET_UNAVAILABLE',
    );
  });

  it('never defaults to the first Holding and fails closed on unknown targets', async () => {
    const transport = vi.fn();
    const client = createWorkEvidenceClient(config(), transport);
    for (const unknown of [
      actor({ holdingId: undefined, holdingSlug: undefined }),
      actor({ holdingId: 'h999', holdingSlug: 'unknown' }),
      actor({ holdingId: 'h001', holdingSlug: 'northstar' }),
      actor({ holdingId: undefined, holdingSlug: undefined, origin: 'https://unknown.example.com' }),
    ]) {
      await expect503(
        () => client.read(unknown, db, 'leave-1', 'sets', false),
        'NATIVE_WORK_EVIDENCE_TARGET_UNAVAILABLE',
      );
    }
    expect(transport).not.toHaveBeenCalled();
  });

  it('fails closed on an optional companyId mismatch', async () => {
    const withCompany = config({
      holdings: [{ ...DEV_CONFIG.holdings[0], companyId: 'acme-co' }],
    });
    await expect503(
      () => createWorkEvidenceClient(withCompany, ok).read(actor(), db, 'leave-1', 'sets', false),
      'NATIVE_WORK_EVIDENCE_COMPANY_MISMATCH',
    );
  });

  it('pins the rendered SQL oracle hashes with the Rust adapter', () => {
    const sets = `SELECT link.id,link.set_id,set_row.purpose_key,
             COALESCE(source.original_name,intent.original_name,'Pending file') original_name,
             COALESCE(source.bytes,intent.actual_bytes,intent.declared_bytes) bytes,
             COALESCE(source.detected_media_type,source.media_type,intent.detected_media_type,intent.media_type) media_type,
             source.source_state,link.state link_state,
             (source.preview_object_key IS NOT NULL) preview_available,
             link.created_at
           FROM attachment_sets set_row
           JOIN attachment_links link ON link.set_id=set_row.id
           LEFT JOIN attachments source
             ON source.id=link.attachment_id AND source.company_id=set_row.company_id
           LEFT JOIN upload_intents intent
             ON intent.id=link.upload_intent_id AND intent.company_id=set_row.company_id
           WHERE set_row.target_type='leave_request' AND set_row.target_id=$1
             AND set_row.company_id=$2
             AND (link.attachment_id IS NULL OR source.id IS NOT NULL)
             AND (link.upload_intent_id IS NULL OR intent.id IS NOT NULL)
             AND set_row.state IN ('draft','bound')
             AND link.state IN ('draft','bound')
             
           ORDER BY link.created_at DESC,link.id DESC`;
    const setsRo = sets
      .replace("             AND set_row.state IN ('draft','bound')\n             AND link.state IN ('draft','bound')\n             \n", "             AND set_row.state='bound'\n             AND link.state='bound'\n             AND source.source_state='available'\n");
    const legacy =
      "SELECT id,NULL::text set_id,purpose purpose_key,original_name,bytes,COALESCE(detected_media_type,media_type) media_type,source_state,'bound' link_state,(preview_object_key IS NOT NULL) preview_available,created_at FROM attachments WHERE owner_type='leave_request' AND owner_id=$1 AND company_id=$2 AND revoked_at IS NULL ORDER BY created_at DESC";
    expect(hash(sets)).toBe('ae52cad228e444d88686254b19ca7e980c19c489e5d52de8bec8ac374fa185e4');
    expect(hash(setsRo)).toBe('e376be2ebee0b6c176045f3aa67098be4fbc4137806a055ca5e7fb441daa426c');
    expect(hash(legacy)).toBe('cc8a48cf37fc9785374fd62da249df3f017675797d3901b5b6523e5608d207a4');
  });

  it('pins the result oracle hash with the Rust domain test', () => {
    expect(hash(JSON.stringify(payload))).toBe(
      'b10b0ac1f263f64aa48d591ae87edd8f1bc668ab1634dabb7699a1085ea29fce',
    );
  });

  it('rejects malformed and cross-kind responses', async () => {
    const bad: unknown[] = [
      { kind: 'legacy', rows },
      { kind: 'sets', rows: {} },
      { kind: 'sets', rows: [{ ...rows[0], set_id: null }] },
      { kind: 'sets', rows: [{ ...rows[0], preview_available: 'yes' }] },
      { kind: 'sets', rows: [{ ...rows[0], id: 7 }] },
      { rows },
    ];
    for (const value of bad)
      await expect503(
        () => createWorkEvidenceClient(config(), async () => ({ status: 200, value })).read(actor(), db, 'leave-1', 'sets', false),
        'NATIVE_WORK_EVIDENCE_RESPONSE_INVALID',
      );
  });

  it('fails closed on missing config, transport failure and 4xx', async () => {
    delete process.env[ENV];
    await expect503(
      () => readNativeWorkEvidence(actor(), db, 'leave-1', 'sets', false, ok),
      'NATIVE_WORK_EVIDENCE_CONFIGURATION_REQUIRED',
    );
    process.env[ENV] = JSON.stringify(DEV_CONFIG);

    await expect503(
      () => readNativeWorkEvidence(actor(), db, 'leave-1', 'sets', false, async () => { throw new Error('offline'); }),
    );

    try {
      await readNativeWorkEvidence(actor(), db, 'leave-1', 'sets', false, async () => ({
        status: 422,
        value: { error: { code: 'WORK_EVIDENCE_READ_REQUEST_INVALID', message: 'invalid' } },
      }));
      throw new Error('expected an ApiError');
    } catch (error) {
      expect(error).toBeInstanceOf(ApiError);
      expect((error as ApiError).status).toBe(422);
      expect((error as ApiError).code).toBe('WORK_EVIDENCE_READ_REQUEST_INVALID');
    }
  });

  it('retired the TypeScript SQL: the adapter is a transport only', () => {
    const source = readFileSync(
      new URL('../../../packages/server-files/src/adapters/work-read-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toMatch(/SELECT /i);
    expect(source).not.toMatch(/\.prepare\(/);
    expect(source).not.toMatch(/\.bind\(/);
    expect(source).toContain('readNativeWorkEvidence');
  });
});

describe('native Files personnel-evidence lane 22 contract', () => {
  const personnelPayload = (kind: PersonnelEvidenceKind) => ({
    kind,
    rows:
      kind === 'evaluation-link'
        ? [{ id: 'link-1' }]
        : kind === 'evaluation-files'
          ? [{ id: 'link-1', filename: 'file.pdf' }]
          : [
              {
                id: 'attachment-1',
                original_name: 'request.pdf',
                media_type: 'application/pdf',
                bytes: 512,
                checksum_sha256: 'abc',
                purpose: 'medical_certificate',
                revision: 2,
              },
            ],
  });
  const personnelCall = (
    kind: PersonnelEvidenceKind,
    args: PersonnelEvidenceArgs,
    overrides: Record<string, unknown> = {},
    transport?: WorkEvidenceTransport,
  ) =>
    createPersonnelEvidenceClient(
      config(),
      transport ?? (async () => ({ status: 200, value: personnelPayload(kind) })),
    ).read(actor(overrides), db, kind, args);

  it('pins the three byte-identical SQL oracles with the Rust adapter', () => {
    expect(hash('SELECT l.id,a.original_name filename FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE s.company_id=$1 AND s.target_type=\'hr_evaluation_assessment\' AND s.target_id=$2 AND l.state=\'bound\' AND a.source_state=\'available\'')).toBe(
      '2fe88e15a79ddf05c007e5c602765c640ad55d034197bd2346ca994025db3c6b',
    );
    expect(hash('SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments f ON f.id=l.attachment_id WHERE l.id=$1 AND s.target_type=\'hr_evaluation_assessment\' AND s.target_id=$2 AND s.company_id=$3 AND l.state=\'bound\' AND f.source_state=\'available\'')).toBe(
      'b8ab97daf89b673665b79b3513782f4a98fa566932a85541d8a266e18908a08c',
    );
    expect(hash("SELECT id,original_name,media_type,bytes,checksum_sha256,purpose,revision FROM attachments WHERE owner_type='employee_request' AND owner_id=$1 AND revoked_at IS NULL ORDER BY created_at,id")).toBe(
      'd82915c70cc853729de238d6ab83be9b0424e219fac877d6684d1544ae21182a',
    );
  });

  it('routes each kind to the registered Holding with the exact scoped headers', async () => {
    for (const [kind, args, expected] of [
      ['evaluation-files', { assessmentId: 'assessment-1' }, { assessment: 'assessment-1', source: undefined, record: undefined }],
      ['evaluation-link', { assessmentId: 'assessment-1', sourceId: 'source-1' }, { assessment: 'assessment-1', source: 'source-1', record: undefined }],
      ['employee-request-attachments', { recordId: 'request-1' }, { assessment: undefined, source: undefined, record: 'request-1' }],
    ] as const) {
      let url: URL | undefined;
      let headers: Record<string, string> | undefined;
      const result = await createPersonnelEvidenceClient(
        config(),
        async (sentUrl, sentHeaders) => {
          url = sentUrl;
          headers = sentHeaders;
          return { status: 200, value: personnelPayload(kind) };
        },
      ).read(actor({ holdingId: 'h002', holdingSlug: 'northstar' }), db, kind, args);
      expect(result.results).toEqual(personnelPayload(kind).rows);
      expect(result.meta).toEqual({ changes: 0, duration: 0 });
      expect(url?.toString()).toBe(
        `http://127.0.0.1:3198/api/v1/holdings/northstar/files/personnel-evidence?kind=${kind}`,
      );
      expect(headers?.host).toBe('northstar.jesadakorn.com');
      expect(headers?.['x-erpx-context-company']).toBe('company-1');
      expect(headers?.['x-erpx-context-assessment']).toBe(expected.assessment);
      expect(headers?.['x-erpx-context-source']).toBe(expected.source);
      expect(headers?.['x-erpx-context-record']).toBe(expected.record);
    }
  });

  it('rejects malformed rows, wrong kinds and unknown Holdings', async () => {
    await expect503(
      () => personnelCall('evaluation-files', { assessmentId: 'a-1' }, {}, async () => ({ status: 200, value: { kind: 'evaluation-files', rows: [{ id: 1, filename: 'f' }] } })),
      'NATIVE_PERSONNEL_EVIDENCE_RESPONSE_INVALID',
    );
    await expect503(
      () => personnelCall('evaluation-files', { assessmentId: 'a-1' }, {}, async () => ({ status: 200, value: { kind: 'evaluation-link', rows: [{ id: 'x' }] } })),
      'NATIVE_PERSONNEL_EVIDENCE_RESPONSE_INVALID',
    );
    await expect503(
      () => personnelCall('evaluation-files', { assessmentId: 'a-1' }, { holdingId: 'h999', holdingSlug: 'unknown' }),
      'NATIVE_WORK_EVIDENCE_TARGET_UNAVAILABLE',
    );
  });

  it('retired the personnel TS SQL: both adapters delegate to the native client', () => {
    for (const [file, api] of [
      ['people-evaluation-facts.ts', 'readNativePersonnelEvidence'],
      ['records-employee-requests-owned-facts.ts', 'readNativePersonnelEvidence'],
    ] as const) {
      const source = readFileSync(
        new URL(`../../../packages/server-files/src/adapters/${file}`, import.meta.url),
        'utf8',
      );
      expect(source).not.toMatch(/SELECT /i);
      expect(source).not.toMatch(/\.prepare\(/);
      expect(source).not.toMatch(/\.bind\(/);
      expect(source).toContain(api);
    }
    // The shared HTTP transport is exported once for both lane 22 clients.
    expect(typeof workEvidenceHttpTransport).toBe('function');
  });
});
