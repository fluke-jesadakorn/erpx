import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
vi.mock('server-only', () => ({}));
import {
  lane29ContextForCompany,
  nativeOccurrenceException,
  parseNativeLane29Response,
  readNativeLane29,
  selectLane29Holding,
  validateNativeLane29Input,
  type Lane29Context,
  type NativeLane29Transport,
} from '../../../packages/server-work_projection/src/adapters/native-lane-29/client';

const envKey = 'ERPX_NATIVE_WORK_PROJECTION_CONFIG';
const original = process.env[envKey];
const company = 'company-acme';
const appointment = 'appointment-1';
const context: Lane29Context = {holdingId: 'h001', holdingSlug: 'acme', companyId: company};

/** Exact development configuration for ERPX_NATIVE_WORK_PROJECTION_CONFIG. */
const DEV_CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    {origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-acme'},
    {origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'company-northstar'},
    {origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'company-erpx'},
  ],
};

const src = (relative: string) =>
  new URL(`../../../packages/server-work_projection/src/${relative}`, import.meta.url);
const hash = (value: string) => createHash('sha256').update(value).digest('hex');
const config = () => JSON.parse(JSON.stringify(DEV_CONFIG)) as unknown;

const json = (payload: unknown, status = 200): Response =>
  new Response(JSON.stringify(payload), {
    status,
    headers: {'content-type': 'application/json'},
  });

const ok = (payload: unknown): NativeLane29Transport => async () => json(payload);

const expectCode = (run: () => unknown, code: string) => {
  try {
    run();
  } catch (error) {
    expect((error as {code?: string}).code).toBe(code);
    return;
  }
  throw new Error(`EXPECTED_FAILURE:${code}`);
};

beforeEach(() => {
  process.env[envKey] = JSON.stringify(config());
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
});

describe('native lane 29 contract', () => {
  it('accepts the exact owner response shapes', () => {
    expect(
      parseNativeLane29Response(
        {exception: {start_local: '09:00', end_local: null, cancelled: false}},
        'occurrence_exception',
        company,
      ),
    ).toEqual({start_local: '09:00', end_local: null, cancelled: false});
    expect(parseNativeLane29Response({exception: null}, 'occurrence_exception', company)).toBeNull();
    expect(
      parseNativeLane29Response(
        {occurrences: [{occurrence_key: 'k', start_local: 'a', end_local: 'b'}]},
        'moved_occurrences',
        company,
      ),
    ).toEqual([{occurrence_key: 'k', start_local: 'a', end_local: 'b'}]);
    const availability = {
      id: appointment,
      starts_at: 1,
      ends_at: 2,
      timezone: 'Asia/Bangkok',
      status: 'scheduled',
      all_day: false,
      frequency: null,
      interval_count: null,
      weekdays: null,
      end_kind: null,
      occurrence_count: null,
      until_date: null,
      user_id: 'user-1',
      response: 'accepted',
    };
    expect(
      parseNativeLane29Response({appointments: [availability]}, 'availability_sources', company),
    ).toEqual([availability]);
    const series = {
      appointment_id: appointment,
      company_id: company,
      frequency: 'weekly',
      interval_count: 1,
      weekdays: '1',
      end_kind: 'never',
      occurrence_count: null,
      until_date: null,
      version: 1,
    };
    expect(parseNativeLane29Response({series: [series]}, 'series_batch', company)).toEqual([series]);
  });

  it('rejects malformed, widened or extra-field responses', () => {
    const bad = [
      [{}, 'occurrence_exception'],
      [{exception: {start_local: 'x', cancelled: false}}, 'occurrence_exception'],
      [{exception: {start_local: 'x', end_local: null, cancelled: 'no'}}, 'occurrence_exception'],
      [{occurrences: [{occurrence_key: 'k', start_local: 'a'}]}, 'moved_occurrences'],
      [{appointments: [{id: appointment}]}, 'availability_sources'],
      [{series: [{appointment_id: appointment}]}, 'series_batch'],
    ] as const;
    for (const [payload, kind] of bad)
      expectCode(
        () => parseNativeLane29Response(payload, kind, company),
        'NATIVE_WORK_LANE29_RESPONSE_INVALID',
      );
  });

  it('selects the exact registered Holding from WorkContext, never the first', async () => {
    const seen: {url: string; host: string | null}[] = [];
    const transport: NativeLane29Transport = async (url, headers) => {
      seen.push({url: url.toString(), host: headers.get('host')});
      return json({occurrences: []});
    };
    await expect(
      readNativeLane29(
        {kind: 'moved_occurrences', companyId: 'company-northstar', appointmentId: appointment},
        {holdingId: 'h002', holdingSlug: 'northstar', companyId: 'company-northstar'},
        transport,
      ),
    ).resolves.toEqual([]);
    expect(seen[0]).toEqual({
      url: 'http://127.0.0.1:3198/api/v1/holdings/northstar/work/native-lane-29',
      host: 'northstar.jesadakorn.com',
    });
    expectCode(
      () => selectLane29Holding(DEV_CONFIG, {holdingId: 'h002', holdingSlug: 'acme'}),
      'NATIVE_WORK_LANE29_TARGET_UNAVAILABLE',
    );
    await expect(
      readNativeLane29(
        {kind: 'moved_occurrences', companyId: company, appointmentId: appointment},
        {holdingId: 'h404', holdingSlug: 'acme'},
        transport,
      ),
    ).rejects.toMatchObject({status: 503});
    expect(seen).toHaveLength(1);
  });

  it('resolves Company-only callers through configured company metadata', () => {
    expect(lane29ContextForCompany('company-erpx')).toEqual({
      holdingId: 'h003',
      holdingSlug: 'erpx',
      companyId: 'company-erpx',
    });
    expectCode(
      () => lane29ContextForCompany('unknown-company'),
      'NATIVE_WORK_LANE29_TARGET_UNAVAILABLE',
    );
  });

  it('fails closed when configuration, transport or response is unavailable', async () => {
    delete process.env[envKey];
    await expect(nativeOccurrenceException(context, company, appointment, 'k')).rejects.toMatchObject({
      status: 503,
    });
    process.env[envKey] = JSON.stringify(config());
    const down: NativeLane29Transport = async () => {
      throw new Error('ECONNREFUSED');
    };
    await expect(
      readNativeLane29(
        {kind: 'moved_occurrences', companyId: company, appointmentId: appointment},
        context,
        down,
      ),
    ).rejects.toMatchObject({status: 503});
    await expect(
      readNativeLane29(
        {kind: 'moved_occurrences', companyId: company, appointmentId: appointment},
        context,
        async () => json({occurrences: 'not-an-array'}),
      ),
    ).rejects.toMatchObject({status: 502});
    await expect(
      readNativeLane29(
        {kind: 'moved_occurrences', companyId: company, appointmentId: appointment},
        context,
        async () => json({error: {code: 'LANE29_REQUEST_INVALID', message: 'nope'}}, 422),
      ),
    ).rejects.toMatchObject({status: 422, code: 'LANE29_REQUEST_INVALID'});
  });

  it('rejects unbounded caller input before transport', async () => {
    expectCode(
      () => validateNativeLane29Input({kind: 'availability_sources', companyId: company, before: 1000, userIds: []}),
      'LANE29_RECIPIENTS_INVALID',
    );
    const transport = vi.fn(async () => json({appointments: []}));
    await expect(
      readNativeLane29(
        {kind: 'occurrence_exception', companyId: company, appointmentId: appointment, occurrenceKey: 'bad key'},
        context,
        transport,
      ),
    ).rejects.toMatchObject({status: 422});
    expect(transport).not.toHaveBeenCalled();
  });

  it('pins the owner bind order on the wire through the generic reader', async () => {
    const seen: string[] = [];
    const transport: NativeLane29Transport = async (_url, headers, body) => {
      seen.push(String(headers.get('x-erpx-context-company')));
      const parsed = JSON.parse(body) as Record<string, unknown>;
      expect(parsed.kind).toBe('availability_sources');
      expect(parsed.before).toBe(1_764_000_000_000);
      expect(parsed.userIds).toEqual(['user-1']);
      return json({appointments: []});
    };
    await expect(
      readNativeLane29(
        {kind: 'availability_sources', companyId: company, before: 1_764_000_000_000, userIds: ['user-1']},
        context,
        transport,
      ),
    ).resolves.toEqual([]);
    expect(seen).toEqual([company]);
    await expect(
      readNativeLane29({kind: 'moved_occurrences', companyId: company, appointmentId: appointment}, context, ok({occurrences: []})),
    ).resolves.toEqual([]);
    await expect(
      readNativeLane29(
        {kind: 'occurrence_exception', companyId: company, appointmentId: appointment, occurrenceKey: 'k'},
        context,
        ok({exception: null}),
      ),
    ).resolves.toBeNull();
    await expect(
      readNativeLane29(
        {kind: 'series_batch', companyId: company, appointmentIds: [appointment]},
        context,
        async (_url, headers, body) => {
          expect(String(headers.get('x-erpx-context-company'))).toBe(company);
          expect(JSON.parse(body).appointmentIds).toEqual([appointment]);
          return json({series: []});
        },
      ),
    ).resolves.toEqual([]);
  });

  it('pins the SQL oracle and proves TypeScript SQL was deleted', async () => {
    const rust = await readFile(src('adapters/native_lane_29/postgres.rs'), 'utf8');
    const facts = await readFile(src('adapters/calendar-availability-facts.ts'), 'utf8');
    expect(hash(
      'SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=$1 AND company_id=$2 AND occurrence_key=$3',
    )).toBe('29cc9700b4a8a2f0e6a2015581c51141b0d4787b9a550386e83f609ad21c413d');
    expect(hash(
      "SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions WHERE appointment_id=$1 AND company_id=$2 AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL",
    )).toBe('a57f2c260b73dd27eb8ab93a5bb4d5ecc13e6b1aee1a0327354dc2137addddd8');
    expect(rust).toContain('calendar_occurrence_exceptions');
    expect(rust).toContain("t.response!='declined' AND t.user_id IN (");
    expect(hash(
      'SELECT * FROM calendar_series WHERE company_id=$1 AND appointment_id IN (SELECT jsonb_array_elements_text($2::jsonb))',
    )).toBe('46ae301749a20a402fc7dfc7560f0007a805bace47c5df793e1280aafec2d336');
    const appointments = await readFile(src('adapters/calendar-appointments-facts.ts'), 'utf8');
    expect(appointments).not.toMatch(/byAppointmentIds/);
    expect(appointments).toContain('nativeSeriesBatch');
    expect(facts).not.toMatch(/\bSELECT\b/);
    expect(facts).toContain('nativeAvailabilitySources');
    expect(facts).toContain('nativeMovedOccurrences');
    expect(facts).toContain('nativeOccurrenceException');
  });
});
