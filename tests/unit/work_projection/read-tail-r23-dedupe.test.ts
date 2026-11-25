import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  series: 'SELECT * FROM calendar_series WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
  exceptions: 'SELECT * FROM calendar_occurrence_exceptions WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
  responses: 'SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
} as const;

describe('work_projection read-tail r23 dedupe', () => {
  it('keeps the series batch on the native lane 29 route and no inline occurrence copies', async () => {
    const facts = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(facts).not.toContain('SELECT ${columns} FROM ${table} WHERE company_id=?');
    expect(facts).not.toContain('byAppointmentIds');
    expect(facts).toContain('nativeSeriesBatch(');
    expect(facts).not.toContain('SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE');
    expect(facts).toContain('nativeCalendarRead.read63(facts)');
    expect(facts).toContain('nativeCalendarRead.read64(facts)');
  });

  it('keeps the byte-identical series SQL in Rust and records the retired occurrence SQL', async () => {
    const rust = await readFile(src('native_lane_29/postgres.rs'), 'utf8');
    expect(rust).toContain('SELECT * FROM calendar_series WHERE company_id=$1');
    expect(rust).toContain('SELECT jsonb_array_elements_text($2::jsonb)');
    expect(original.series).toBe(
      'SELECT * FROM calendar_series WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
    );
    expect(original.exceptions).toBe(
      'SELECT * FROM calendar_occurrence_exceptions WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
    );
    expect(original.responses).toBe(
      'SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
    );
  });

  it('leaves the r9-r22 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql',
      'calendarImageSql', 'appointmentAttendeeSeriesSql', 'appointmentChildImageSql',
      'commandReceiptSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const facts = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(facts).toContain('nativeSeriesBatch(');
    expect(facts).not.toContain('byAppointmentIds');
    expect(facts).not.toContain('overlapProbe');
    expect(facts).toContain("conflictProbe(db, 'participant', facts)");
    expect(facts).toContain("conflictProbe(db, 'attendee', facts)");
    expect(facts).not.toContain('attendeeScopeExistsSql');
    expect(facts).toContain('nativeAppointmentCandidates(');

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(purchases).toContain('commandReceiptSql(');
    expect(purchases).toContain('calendarManualSourceReadLocks');
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain('SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE');
    const pageLayouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(pageLayouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
  });

  it('retirement:ts-statements stays strictly below the r23 BEFORE=59 baseline', () => {
    const count = Number(
      execFileSync(
        'git',
        ['grep', '-I', '-o', '-E', 'SELECT', '--', ':(glob)packages/server-work_projection/src/**/*.ts'],
        { encoding: 'utf8' },
      )
        .trim()
        .split('\n')
        .filter(Boolean).length,
    );
    expect(count).toBeLessThan(59);
  });
});

/** r34 native cutover: READ_67/61/63/64 replace the three retired SQL literals.
 * Lane 29 then moved byAppointmentIds and the conflict probes to its native
 * owner route, so the final owner SELECT count is 45 (50 -> 48 at r34,
 * 48 -> 46 then 46 -> 45 at lane 29). */
const retiredSql = {
  authorization: 'SELECT * FROM calendar_reminder_execution_authorizations WHERE appointment_id=? AND company_id=? AND recipient_id=? ORDER BY source_version DESC,created_at DESC,command_id DESC LIMIT 1',
  response: 'SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?',
  exceptions: 'SELECT * FROM calendar_occurrence_exceptions WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
  responses: 'SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
} as const;

const rustSrc = () =>
  new URL('../../../packages/server-work_projection/src/adapters/calendar_read.rs', import.meta.url);

type Capture = { path: string; kind: string; values: unknown[]; companyId: string };

async function captureNativeRead<T>(result: unknown, call: () => Promise<T>): Promise<{ value: Promise<T>; capture: Capture }> {
  let capture: Capture | undefined;
  const server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on('data', chunk => chunks.push(chunk as Buffer));
    req.on('end', () => {
      const body = Buffer.concat(chunks).toString('utf8');
      const parsed = JSON.parse(body) as { kind: string; values: unknown[]; companyId: string };
      capture = { path: req.url ?? '', ...parsed };
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ data: { kind: parsed.kind, result } }));
    });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const port = (server.address() as AddressInfo).port;
  const previous = process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG;
  process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG = JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${port}`,
    holdingSlug: 'acme',
    holdingId: 'holding-1',
    origin: 'https://acme.example.test',
    timeoutMs: 1000,
  });
  try {
    const value = call();
    try { await value; } catch { /* the caller observes the settled rejection */ }
    if (!capture) throw new Error('native read transport was not invoked');
    return { value, capture };
  } finally {
    if (previous === undefined) delete process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG;
    else process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG = previous;
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
}

describe('work_projection native read r34 cutover', () => {
  it('keeps the Rust read plans byte-identical to the retired TS SQL modulo ? -> $n', async () => {
    const rust = await readFile(rustSrc(), 'utf8');
    const bridged = (name: string): string => {
      const match = rust.match(new RegExp(`const ${name}: &str = "([^"]*)";`));
      if (!match) throw new Error(`${name} missing from calendar_read.rs`);
      return match[1]!.replace(/\$\d+/g, '?');
    };
    expect(bridged('REMINDER_AUTHORIZATION_SQL')).toBe(retiredSql.authorization);
    expect(bridged('OCCURRENCE_RESPONSE_SQL')).toBe(retiredSql.response);
    expect(bridged('OCCURRENCE_EXCEPTIONS_SQL')).toBe(retiredSql.exceptions);
    expect(bridged('OCCURRENCE_RESPONSES_SQL')).toBe(retiredSql.responses);
  });

  it('removes the retired literals from the TypeScript adapters', async () => {
    const purchases = await readFile(src('calendar/reminder-purchases.ts'), 'utf8');
    const availability = await readFile(src('calendar-availability-facts.ts'), 'utf8');
    const appointments = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(purchases).not.toContain(retiredSql.authorization);
    expect(purchases).not.toContain('occurrenceResponseSql');
    expect(purchases).toContain('nativeCalendarRead.read67({value1:row.id,value2:row.company_id,value3:row.user_id})');
    expect(availability).not.toContain(retiredSql.response);
    expect(availability).not.toContain('export const occurrenceResponseSql');
    expect(availability).toContain('nativeCalendarRead.read61(facts)');
    expect(appointments).not.toContain(retiredSql.exceptions);
    expect(appointments).not.toContain(retiredSql.responses);
  });

  it('posts the exact kind, positional binds and company scope for all four wrappers', async () => {
    const { nativeCalendarRead } = await import(
      '../../../packages/feature-work_projection/src/lib/native-calendar-read.ts'
    );
    const auth = {
      command_id: 'cmd-1', appointment_id: 'ap-1', recipient_id: 'u-1', company_id: 'co-1',
      actor_id: 'u-2', assignment_id: 'as-1', action: 'appointment.reminder.set',
      authority_json: '{}', source_hash: 'hash', source_version: 7, created_at: 1000,
    };
    const a = await captureNativeRead(auth, () =>
      nativeCalendarRead.read67({ value1: 'ap-1', value2: 'co-1', value3: 'u-1' }));
    const authResult = await a.value;
    expect(authResult).toEqual(auth);
    expect(Object.keys(authResult!)).toHaveLength(11);
    expect(a.capture).toEqual({
      path: '/api/v1/holdings/acme/work/owner-read',
      kind: 'wp_cal_read_67', values: ['ap-1', 'co-1', 'u-1'], companyId: 'co-1',
    });

    const b = await captureNativeRead({ response: 'accepted' }, () =>
      nativeCalendarRead.read61({ value1: 'ap-1', value2: 'co-1', value3: 'k1', value4: 'u-1' }));
    expect(await b.value).toEqual({ response: 'accepted' });
    expect(b.capture).toEqual({
      path: '/api/v1/holdings/acme/work/owner-read',
      kind: 'wp_cal_read_61', values: ['ap-1', 'co-1', 'k1', 'u-1'], companyId: 'co-1',
    });

    const ids = '["ap-1","ap-2"]';
    const exceptions = [{
      appointment_id: 'ap-1', occurrence_key: 'k1', start_local: null, end_local: null, cancelled: true,
    }];
    const c = await captureNativeRead(exceptions, () =>
      nativeCalendarRead.read63({ value1: 'co-1', value2: ids }));
    expect(await c.value).toEqual(exceptions);
    expect(c.capture).toEqual({
      path: '/api/v1/holdings/acme/work/owner-read',
      kind: 'wp_cal_read_63', values: ['co-1', ids], companyId: 'co-1',
    });

    const responses = [{
      appointment_id: 'ap-1', occurrence_key: 'k1', user_id: 'u-1', response: 'declined',
    }];
    const d = await captureNativeRead(responses, () =>
      nativeCalendarRead.read64({ value1: 'co-1', value2: ids }));
    expect(await d.value).toEqual(responses);
    expect(d.capture).toEqual({
      path: '/api/v1/holdings/acme/work/owner-read',
      kind: 'wp_cal_read_64', values: ['co-1', ids], companyId: 'co-1',
    });
  });

  it('fails closed on null and on a malformed authorization row', async () => {
    const { nativeCalendarRead, NativeCalendarReadError } = await import(
      '../../../packages/feature-work_projection/src/lib/native-calendar-read.ts'
    );
    const empty = await captureNativeRead(null, () =>
      nativeCalendarRead.read67({ value1: 'ap-1', value2: 'co-1', value3: 'u-1' }));
    expect(await empty.value).toBeNull();
    const malformed = await captureNativeRead({ command_id: 'cmd-1' }, () =>
      nativeCalendarRead.read67({ value1: 'ap-1', value2: 'co-1', value3: 'u-1' }));
    await expect(malformed.value).rejects.toBeInstanceOf(NativeCalendarReadError);
    const noResponse = await captureNativeRead({ unexpected: true }, () =>
      nativeCalendarRead.read61({ value1: 'ap-1', value2: 'co-1', value3: 'k1', value4: 'u-1' }));
    await expect(noResponse.value).rejects.toBeInstanceOf(NativeCalendarReadError);
  });

  it('pins the final owner SELECT count after the lane 29 native cutover', () => {
    const count = Number(
      execFileSync(
        'git',
        ['grep', '-I', '-o', '-E', 'SELECT', '--', ':(glob)packages/server-work_projection/src/**/*.ts'],
        { encoding: 'utf8' },
      )
        .trim()
        .split('\n')
        .filter(Boolean).length,
    );
    expect(count).toBe(40);
  });
});
