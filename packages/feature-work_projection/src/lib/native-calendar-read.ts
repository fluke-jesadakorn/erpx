import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import type { AppointmentRow } from '@erpx/server-work_projection/public/calendar/appointments';
import type { AnnouncementRow } from '@erpx/server-work_projection/public/calendar/announcements';
import type { SeriesRow } from '@erpx/server-work_projection/public/calendar/commands';

/**
 * The single read transport for the twelve calendar facts of the Work command
 * unit. It sends a typed allow-listed `kind` and positional values to the
 * private native `/work/owner-read` axum route, which EXECUTES the owner SELECT
 * on the caller's Holding transaction. No SQL, no JSON SQL mirror and no
 * fallback exist here: an absent transport or invalid reply fails closed.
 *
 * Transport settings come from `ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG`
 * (or the already-provisioned owner-write config when no dedicated read config
 * is set), using the same validated Holding loopback shape.
 */
export type CalendarReadConfig = Readonly<{
  privateBaseUrl: string;
  holdingSlug: string;
  holdingId: string;
  origin: string;
  timeoutMs: number;
}>;

export class NativeCalendarReadError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native Work calendar read could not be completed.');
    this.name = 'NativeCalendarReadError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeCalendarReadError('NATIVE_WORK_CALENDAR_READ_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: string | undefined): CalendarReadConfig {
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string') return fail();
  const timeoutMs = value.timeoutMs;
  if (
    typeof timeoutMs !== 'number' ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 250 ||
    timeoutMs > 30000
  )
    return fail();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  )
    return fail();
  if (
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]+$/.test(value.holdingSlug) ||
    typeof value.holdingId !== 'string' ||
    value.holdingId.length === 0 ||
    typeof value.origin !== 'string' ||
    value.origin !== new URL(value.origin).origin ||
    !value.origin.startsWith('https://')
  )
    return fail();
  return {
    privateBaseUrl: value.privateBaseUrl,
    holdingSlug: value.holdingSlug,
    holdingId: value.holdingId,
    origin: value.origin,
    timeoutMs,
  };
}

function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(
      url,
      { method: 'POST', agent, headers: { ...headers, 'content-length': String(new TextEncoder().encode(body).length) }, signal, maxHeaderSize: 8192 },
      incoming => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          if (status !== 200) {
            incoming.destroy();
            resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
            return;
          }
          const values = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2) values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status, headers: values }));
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.write(body);
    outgoing.end();
  });
}

async function read(kind: string, values: readonly unknown[], companyId: string): Promise<unknown> {
  const config = configuration(
    process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG ??
      process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_WRITE_CONFIG,
  );
  if (
    !/^wp_cal_read_\d{2}$/.test(kind) ||
    values.length < 1 ||
    values.length > 64 ||
    companyId.length > 200
  )
    return fail();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/work/owner-read`,
    config.privateBaseUrl,
  );
  const body = JSON.stringify({ kind, values, companyId });
  if (new TextEncoder().encode(body).length > 128 * 1024) return fail();
  const headers: Record<string, string> = {
    host: new URL(config.origin).host,
    origin: config.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
  };
  if (companyId) headers['x-erpx-context-company'] = companyId;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await directHttp(url, headers, body, controller.signal);
    if (response.redirected || response.status !== 200 || !response.body) return fail(response.status);
    const text = await response.text();
    if (text.length > 256 * 1024) return fail();
    const payload = JSON.parse(text) as { data?: unknown };
    if (
      !record(payload) ||
      !record(payload.data) ||
      payload.data.kind !== kind ||
      !('result' in payload.data)
    )
      return fail();
    return payload.data.result;
  } catch (error) {
    if (error instanceof NativeCalendarReadError) throw error;
    return fail();
  } finally {
    clearTimeout(timer);
  }
}

const rows = (result: unknown): Record<string, unknown>[] => {
  if (!Array.isArray(result) || !result.every(record)) throw new NativeCalendarReadError('NATIVE_WORK_CALENDAR_READ_RESPONSE_INVALID');
  return result as Record<string, unknown>[];
};
const row = (result: unknown): Record<string, unknown> | null => {
  if (result === null) return null;
  if (!record(result)) throw new NativeCalendarReadError('NATIVE_WORK_CALENDAR_READ_RESPONSE_INVALID');
  return result;
};

export type CalendarInterval = {
  starts_at: number;
  ends_at: number;
  start_local: string;
  end_local: string;
};
export type CalendarAttendeeState = {
  user_id: string;
  response: string;
  reminder_minutes: number | null;
  response_at: number | null;
};
export type CalendarOccurrenceException = {
  appointment_id: string;
  occurrence_key: string;
  start_local: string | null;
  end_local: string | null;
  cancelled: boolean;
};
export type CalendarOccurrenceResponse = {
  appointment_id: string;
  occurrence_key: string;
  user_id: string;
  response: 'accepted' | 'tentative' | 'declined';
};
export type CalendarReminderAuthorizationRow = {
  command_id: string;
  appointment_id: string;
  recipient_id: string;
  company_id: string;
  actor_id: string;
  assignment_id: string;
  action: 'appointment.save' | 'appointment.reschedule' | 'appointment.reminder.set';
  authority_json: string;
  source_hash: string;
  source_version: number;
  created_at: number;
};

/**
 * Typed bridge for the twelve calendar command read facts. Each wrapper keeps
 * the exact return shape of the former inline SQL facts adapter.
 */
export const nativeCalendarRead = {
  read1: async (facts: { value1: string; value2: string; value3: string }): Promise<CalendarInterval | null> => {
    const result = row(await read('wp_cal_read_01', [facts.value1, facts.value2, facts.value3, facts.value2, facts.value1, facts.value2, facts.value2, facts.value3, facts.value2, facts.value2], ''));
    if (!result) return null;
    if (
      typeof result.starts_at !== 'number' ||
      typeof result.ends_at !== 'number' ||
      typeof result.start_local !== 'string' ||
      typeof result.end_local !== 'string'
    )
      throw new NativeCalendarReadError('NATIVE_WORK_CALENDAR_READ_RESPONSE_INVALID');
    return result as unknown as CalendarInterval;
  },
  read3: async (facts: { value1: string }): Promise<SeriesRow | null> =>
    row(await read('wp_cal_read_03', [facts.value1], '')) as unknown as SeriesRow | null,
  read4: async (facts: { value1: string; value2: string }): Promise<{ results: { user_id: string }[] }> => ({
    results: rows(await read('wp_cal_read_04', [facts.value1, facts.value2], facts.value2)) as { user_id: string }[],
  }),
  read12: async (facts: { value1: string; value2: string }): Promise<AppointmentRow | null> =>
    row(await read('wp_cal_read_12', [facts.value1, facts.value2], facts.value2)) as unknown as AppointmentRow | null,
  read13: async (facts: { value1: string; value2: string }): Promise<{ person_id: string }[]> =>
    rows(await read('wp_cal_read_13', [facts.value1, facts.value2], facts.value2)) as { person_id: string }[],
  read23: async (facts: { value1: string; value2: string }): Promise<AnnouncementRow | null> =>
    row(await read('wp_cal_read_23', [facts.value1, facts.value2], facts.value2)) as unknown as AnnouncementRow | null,
  read26: async (facts: { value1: string; value2: string }): Promise<AppointmentRow | null> =>
    row(await read('wp_cal_read_26', [facts.value1, facts.value2], facts.value2)) as unknown as AppointmentRow | null,
  read30: async (facts: { value1: string; value2: string }): Promise<{ results: CalendarAttendeeState[] }> => ({
    results: rows(await read('wp_cal_read_30', [facts.value1, facts.value2], facts.value2)) as CalendarAttendeeState[],
  }),
  read37: async (facts: { value1: string; value2: string }): Promise<AppointmentRow | null> =>
    row(await read('wp_cal_read_37', [facts.value1, facts.value2], facts.value2)) as unknown as AppointmentRow | null,
  read38: async (facts: { value1: string; value2: string; value3: string }): Promise<{ attendance: string } | null> =>
    row(await read('wp_cal_read_38', [facts.value1, facts.value2, facts.value3], facts.value1)) as { attendance: string } | null,
  read42: async (facts: { value1: string; value2: string }): Promise<{ user_id: string } | null> =>
    row(await read('wp_cal_read_42', [facts.value1, facts.value2], facts.value2)) as { user_id: string } | null,
  read53: async (facts: { value1: string; value2: string }): Promise<AnnouncementRow | null> =>
    row(await read('wp_cal_read_53', [facts.value1, facts.value2], facts.value2)) as unknown as AnnouncementRow | null,
  read61: async (facts: {
    value1: string;
    value2: string;
    value3: string;
    value4: string;
  }): Promise<{ response: string } | null> => {
    const result = row(await read('wp_cal_read_61', [facts.value1, facts.value2, facts.value3, facts.value4], facts.value2));
    if (!result) return null;
    if (typeof result.response !== 'string')
      throw new NativeCalendarReadError('NATIVE_WORK_CALENDAR_READ_RESPONSE_INVALID');
    return { response: result.response };
  },
  read63: async (facts: { value1: string; value2: string }): Promise<CalendarOccurrenceException[]> =>
    rows(await read('wp_cal_read_63', [facts.value1, facts.value2], facts.value1)) as CalendarOccurrenceException[],
  read64: async (facts: { value1: string; value2: string }): Promise<CalendarOccurrenceResponse[]> =>
    rows(await read('wp_cal_read_64', [facts.value1, facts.value2], facts.value1)) as CalendarOccurrenceResponse[],
  read67: async (facts: { value1: string; value2: string; value3: string }): Promise<CalendarReminderAuthorizationRow | null> => {
    const result = row(await read('wp_cal_read_67', [facts.value1, facts.value2, facts.value3], facts.value2));
    if (!result) return null;
    const strings = [
      'command_id', 'appointment_id', 'recipient_id', 'company_id', 'actor_id',
      'assignment_id', 'action', 'authority_json', 'source_hash',
    ];
    if (
      !strings.every(field => typeof result[field] === 'string') ||
      typeof result.source_version !== 'number' ||
      typeof result.created_at !== 'number'
    )
      throw new NativeCalendarReadError('NATIVE_WORK_CALENDAR_READ_RESPONSE_INVALID');
    return result as unknown as CalendarReminderAuthorizationRow;
  },
};
