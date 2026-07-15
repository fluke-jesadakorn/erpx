import 'server-only';

export type HookKind = 'line' | 'generic';

export type HookStatus = 'received' | 'processed' | 'failed' | 'rejected';

export interface HookProvider {
  id: string;
  displayName: string;
  kind: HookKind;
  secretEnv: string;
  enabled: boolean;
}

export interface HookVerifyOk {
  ok: true;
  provider: HookProvider;
}

export interface HookVerifyFail {
  ok: false;
  reason: 'unknown_provider' | 'disabled' | 'missing_secret' | 'bad_signature';
}

export type HookVerifyResult = HookVerifyOk | HookVerifyFail;

export interface HookEventInput {
  providerId: string;
  externalId: string | null;
  eventType: string;
  payload: unknown;
  headers: Record<string, string>;
  signatureOk: boolean;
}

export interface HookEventRow {
  id: number;
  providerId: string;
  externalId: string | null;
  eventType: string;
  receivedAt: string;
  status: HookStatus;
  signatureOk: boolean;
  replayCount: number;
}