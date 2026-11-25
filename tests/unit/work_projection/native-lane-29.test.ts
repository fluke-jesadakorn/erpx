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
    expect(parseNativeLane29Response({conflict: true}, 'conflict_probe', company)).toEqual({conflict: true});
    expect(parseNativeLane29Response({conflict: false}, 'conflict_probe', company)).toEqual({conflict: false});
  });

  it('pins candidate-list response shapes', async () => {
    const { readFile } = await import('node:fs/promises');
    const keys = (source: string, name: string) => {
      const match = source.match(new RegExp(`const ${name} = \\[([\\s\\S]*?)\\] as const;`));
      return match ? match[1]!.split(',').map(s => s.trim().replace(/'/g, '')).filter(Boolean) : [];
    };
    const client = await readFile(
      new URL('../../../packages/server-work_projection/src/adapters/native-lane-29/client.ts', import.meta.url),
      'utf8',
    );
    const appointmentKeys = keys(client, 'APPOINTMENT_KEYS');
    const attendeeKeys = keys(client, 'ATTENDEE_KEYS');
    const announcementKeys = keys(client, 'ANNOUNCEMENT_KEYS');
    const row = (names: string[], ids: string[]) => Object.fromEntries(names.map(name => [name, ids.includes(name) ? company : null]));
    expect(parseNativeLane29Response({appointments: [row(appointmentKeys, ['id', 'company_id'])]}, 'appointment_candidates', company)).toHaveLength(1);
    expect(parseNativeLane29Response({attendees: [row(attendeeKeys, ['appointment_id', 'user_id'])]}, 'attendee_rows', company)).toHaveLength(1);
    expect(parseNativeLane29Response({announcements: [row(announcementKeys, ['id', 'company_id'])]}, 'announcement_candidates', company)).toHaveLength(1);
    expect(() => parseNativeLane29Response({appointments: [{}]}, 'appointment_candidates', company)).toThrow();
  });

  it('rejects malformed, widened or extra-field responses', () => {
    const bad = [
      [{}, 'occurrence_exception'],
      [{exception: {start_local: 'x', cancelled: false}}, 'occurrence_exception'],
      [{exception: {start_local: 'x', end_local: null, cancelled: 'no'}}, 'occurrence_exception'],
      [{occurrences: [{occurrence_key: 'k', start_local: 'a'}]}, 'moved_occurrences'],
      [{appointments: [{id: appointment}]}, 'availability_sources'],
      [{series: [{appointment_id: appointment}]}, 'series_batch'],
      [{conflict: 'yes'}, 'conflict_probe'],
      [{conflict: true, extra: 1}, 'conflict_probe'],
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

  it('pins the conflict-probe wire shape and fails closed on unbounded input', async () => {
    const seen: string[] = [];
    await expect(
      readNativeLane29(
        {
          kind: 'conflict_probe',
          companyId: company,
          scope: 'attendee',
          excludeId: 'appointment-2',
          before: 2_000,
          after: 1_000,
          ids: ['person-1'],
        },
        context,
        async (_url, headers, body) => {
          seen.push(String(headers.get('x-erpx-context-company')));
          const parsed = JSON.parse(body) as Record<string, unknown>;
          expect(parsed.scope).toBe('attendee');
          expect(parsed.before).toBe(2_000);
          expect(parsed.after).toBe(1_000);
          expect(parsed.ids).toEqual(['person-1']);
          return json({conflict: true});
        },
      ),
    ).resolves.toEqual({conflict: true});
    expect(seen).toEqual([company]);
    await expect(
      readNativeLane29(
        {
          kind: 'conflict_probe',
          companyId: company,
          scope: 'participant',
          excludeId: 'appointment-2',
          before: 1_000,
          after: 2_000,
          ids: [],
        },
        context,
        ok({conflict: false}),
      ),
    ).rejects.toMatchObject({status: 422});
  });

  it('pins the candidate-list wire arity', async () => {
    const seen: string[] = [];
    await expect(
      readNativeLane29(
        {kind: 'attendee_rows', companyId: company, values: ['[]', '[]']},
        context,
        async (_url, headers, body) => {
          seen.push(String(headers.get('x-erpx-context-company')));
          expect(JSON.parse(body).values).toEqual(['[]', '[]']);
          return json({attendees: []});
        },
      ),
    ).resolves.toEqual([]);
    expect(seen).toEqual([company]);
    await expect(
      readNativeLane29(
        {kind: 'appointment_candidates', companyId: company, values: ['[]']},
        context,
        ok({appointments: []}),
      ),
    ).rejects.toMatchObject({status: 422});
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
    expect(hash(
      'SELECT 1 FROM calendar_appointments a JOIN calendar_appointment_participants p ON p.appointment_id=a.id WHERE a.company_id=$1 AND a.status=\'scheduled\' AND a.id!=$2 AND a.starts_at<$3 AND a.ends_at>$4 AND p.person_id IN (SELECT jsonb_array_elements_text($5::jsonb)) LIMIT 1',
    )).toBe('613db35276b9d29e8ab019c00ad5b4b72381621d7543fe8266eebebfa6e487c1');
    expect(hash(
      'SELECT 1 FROM calendar_appointments a JOIN calendar_attendees p ON p.appointment_id=a.id WHERE a.company_id=$1 AND a.status=\'scheduled\' AND a.id!=$2 AND a.starts_at<$3 AND a.ends_at>$4 AND p.response!=\'declined\' AND p.user_id IN (SELECT jsonb_array_elements_text($5::jsonb)) LIMIT 1',
    )).toBe('fa8e749772b2dc348cf49b54bbb5d4f06a4802ef132e8e49fe7aafc41ae52a31');
    expect(hash(
      'WITH candidates AS (\n SELECT x.*,p.legal_name organizer_name,\n (($1 AND coalesce(x.department_id,x.branch_id,x.company_id) IN (SELECT jsonb_array_elements_text($2::jsonb))) OR x.organizer_person_id=$3 OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=$4)) full_detail\n FROM calendar_appointments x LEFT JOIN (SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people) p ON p.id=x.organizer_person_id AND p.company_id=x.company_id\n WHERE x.company_id=$5 AND x.starts_at<$6 AND (x.ends_at>$7 OR EXISTS(SELECT 1 FROM calendar_series s WHERE s.appointment_id=x.id))\n AND (coalesce(x.department_id,x.branch_id,x.company_id) IN (SELECT jsonb_array_elements_text($8::jsonb)) OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=$9) OR EXISTS(SELECT 1 FROM calendar_appointment_participants ap JOIN (SELECT person_id,company_id,status,branch_id,department_id FROM hr_engagements) e ON e.person_id=ap.person_id AND e.company_id=ap.company_id WHERE ap.appointment_id=x.id AND e.status=\'active\' AND coalesce(e.department_id,e.branch_id,e.company_id) IN (SELECT jsonb_array_elements_text($10::jsonb)))))\n SELECT x.* FROM candidates x WHERE (full_detail OR ($11 AND x.status=\'scheduled\'))\n AND (($12::text IS NOT NULL AND EXISTS(SELECT 1 FROM calendar_appointment_participants ap WHERE ap.appointment_id=x.id AND ap.person_id=$13))\n   OR ($14::text IS NULL AND ($15=\'team\' OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=$16) OR x.organizer_person_id=$17)))\n AND ($18=\'\' OR (CASE WHEN full_detail THEN x.title ELSE \'Busy\' END) ILIKE \'%\'||$19||\'%\') ORDER BY x.id LIMIT $20',
    )).toBe('d6c5f523d29aefbf566e6d80bf3827d5de0e2d4e9b1e395a4fccfaedf5788265');
    const announcements = await readFile(src('adapters/calendar-announcements-facts.ts'), 'utf8');
    expect(announcements).not.toMatch(/\bSELECT\b/);
    expect(announcements).toContain('nativeAnnouncementCandidates');
    const appointments = await readFile(src('adapters/calendar-appointments-facts.ts'), 'utf8');
    expect(appointments).not.toMatch(/byAppointmentIds|overlapProbe|attendeeScopeExistsSql|participantScopeExistsSql|derivedProbeSql|\bSELECT\b/);
    expect(appointments).toContain('nativeSeriesBatch');
    expect(appointments).toContain('nativeConflictProbe');
    expect(facts).not.toMatch(/\bSELECT\b/);
    expect(facts).toContain('nativeAvailabilitySources');
    expect(facts).toContain('nativeMovedOccurrences');
    expect(facts).toContain('nativeOccurrenceException');
  });
});
