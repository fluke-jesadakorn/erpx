import { describe, expect, it } from 'vitest';
import { createWebPushApplication, type WebPushPorts } from '@erpx/server-collaboration/application/web-push';
import type { Preference } from '@erpx/server-collaboration/domain/web-push';

type Settings = Parameters<ReturnType<typeof createWebPushApplication>['pushSettings']>[0];
const actor = {
  companyId: 'company-1',
  userId: 'user-1',
  authority: { sessionId: 'session-1' },
} as unknown as Settings;

const stored: Preference = {
  enabled: true,
  work_enabled: true,
  mention_enabled: false,
  system_enabled: true,
  calendar_enabled: false,
  notice_enabled: false,
  quiet_start_minute: 540,
  quiet_end_minute: 600,
};

function ports(preferences: Preference | null, count: number | null): WebPushPorts {
  return {
    repository: {
      preferences: async () => preferences,
      sessionSubscriptionCount: async () => (count === null ? null : { n: count }),
    } as unknown as WebPushPorts['repository'],
    clock: { now: () => 0 },
    identifiers: { next: () => 'id' },
    configuration: () => ({
      publicKey: 'vapid-public',
      privateKey: 'private',
      subject: 'mailto:owner@example.test',
      dataKey: new Uint8Array(32),
    }),
    allowedOrigin: () => true,
    validate: (raw: unknown) => raw as never,
    hashEndpoint: (value: string) => value,
    seal: () => '',
    open: (value: string) => ({ endpoint: value, keys: { p256dh: '', auth: '' } }),
    currentSession: async () => true,
    send: async () => ({ statusCode: 201 }),
  } as unknown as WebPushPorts;
}

describe('native GET /push settings parity', () => {
  it('returns the exact payload the native owner query assembles', async () => {
    expect(await createWebPushApplication(ports(stored, 2)).pushSettings(actor)).toEqual({
      available: true,
      publicKey: 'vapid-public',
      preferences: stored,
      deviceCount: 2,
    });
  });

  it('uses the same defaults and zero count as the native query', async () => {
    expect(await createWebPushApplication(ports(null, null)).pushSettings(actor)).toEqual({
      available: true,
      publicKey: 'vapid-public',
      preferences: {
        enabled: false,
        work_enabled: true,
        mention_enabled: true,
        system_enabled: true,
        calendar_enabled: false,
        notice_enabled: false,
        quiet_start_minute: null,
        quiet_end_minute: null,
      },
      deviceCount: 0,
    });
  });

  it('short circuits without touching user state when push is unconfigured', async () => {
    const p = ports(stored, 9);
    p.configuration = () => null;
    expect(await createWebPushApplication(p).pushSettings(actor)).toEqual({
      available: false,
      publicKey: null,
      preferences: null,
      deviceCount: 0,
    });
  });
});
