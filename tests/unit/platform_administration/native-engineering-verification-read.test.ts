import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import {
  parseNativeEngineeringReadConfig,
  nativeEngineeringPriorRead,
  nativeEngineeringActiveRead,
  nativeEngineeringReplyGuard,
  nativeEngineeringReadAvailable,
} from '../../../modules/platform-administration/server/src/adapters/native-engineering-verification-read/client';

const srcRoot = resolve(__dirname, '../../../modules/platform-administration/server/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const validRaw = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:4099',
  holdingSlug: 'acme',
  holdingId: 'H001',
  origin: 'http://127.0.0.1:3104',
  timeoutMs: 1000,
});

describe('Platform Administration native engineering read binding', () => {
  test('registered target configuration is parsed strictly and fails closed', () => {
    expect(parseNativeEngineeringReadConfig(validRaw)).toEqual({
      privateBaseUrl: 'http://127.0.0.1:4099',
      holdingSlug: 'acme',
      holdingId: 'H001',
      origin: 'http://127.0.0.1:3104',
      timeoutMs: 1000,
    });
    for (const raw of [
      undefined,
      '',
      '{',
      validRaw.replace('4099', '3100'),
      validRaw.replace('4099', '8080'),
      validRaw.replace('"timeoutMs":1000', '"timeoutMs":100'),
      validRaw.replace('"holdingSlug":"acme"', '"holdingSlug":"ACME"'),
      validRaw.replace('http://127.0.0.1:3104', 'http://forged.example.test'),
    ]) {
      expect(() => parseNativeEngineeringReadConfig(raw)).toThrow();
    }
    expect(nativeEngineeringReadAvailable({ ERPX_NATIVE_PLATFORM_ENGINEERING_CONFIG: validRaw } as unknown as NodeJS.ProcessEnv)).toBe(true);
    expect(nativeEngineeringReadAvailable({} as unknown as NodeJS.ProcessEnv)).toBe(false);
  });

  test('unbounded read inputs fail closed before any transport', async () => {
    await expect(nativeEngineeringPriorRead('', 'abcdef123456', { ERPX_NATIVE_PLATFORM_ENGINEERING_CONFIG: validRaw } as unknown as NodeJS.ProcessEnv)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID' });
    await expect(nativeEngineeringPriorRead('company-1', 'xyz', { ERPX_NATIVE_PLATFORM_ENGINEERING_CONFIG: validRaw } as unknown as NodeJS.ProcessEnv)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID' });
    await expect(nativeEngineeringPriorRead('company-1', 'abcdef123456', {} as unknown as NodeJS.ProcessEnv)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_CONFIG_REQUIRED' });
  });

  test('active and reply-guard native clients reject unbounded inputs before transport', async () => {
    const env = { ERPX_NATIVE_PLATFORM_ENGINEERING_CONFIG: validRaw } as unknown as NodeJS.ProcessEnv;
    const source = 'a'.repeat(64);
    await expect(nativeEngineeringActiveRead('', 'abcdef123456', source, `release-${source}`, 'b'.repeat(64), env)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID' });
    await expect(nativeEngineeringActiveRead('company-1', 'xyz', source, `release-${source}`, 'b'.repeat(64), env)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID' });
    await expect(nativeEngineeringActiveRead('company-1', 'abcdef123456', source, 'other', 'b'.repeat(64), env)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID' });
    await expect(nativeEngineeringReplyGuard('', env)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID' });
    await expect(nativeEngineeringReplyGuard('company-1', {} as unknown as NodeJS.ProcessEnv)).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ENGINEERING_CONFIG_REQUIRED' });
  });

  test('the TypeScript prior SQL is deleted and the consumer is bound to the native client', () => {
    const engineering = source('adapters/engineering-verification.ts');
    const client = source('adapters/native-engineering-verification-read/client.ts');
    expect(engineering).toContain('from "./native-engineering-verification-read/client"');
    expect(engineering).toContain('prior: (companyId, run) => nativeEngineeringPriorRead(companyId, run)');
    expect(engineering).not.toContain('SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts');
    expect(client).not.toMatch(/\bSELECT\b/);
    expect(client).toContain('nativeEngineeringPriorRead');
    expect(client).toContain('/platform/engineering-verification/read/prior');
  });

  test('the native owner HTTP binding is exported for the lead mount', () => {
    const http = source('adapters/native_engineering_verification_read_http.rs');
    const publicEntry = source('public/native_engineering_verification_read_http.rs');
    const adapters = source('adapters/mod.rs');
    expect(http).toContain('engineering_verification_read_http_router');
    expect(http).toContain('/platform/engineering-verification/read/prior');
    expect(http).toContain('EngineeringVerificationReadHttpBackend');
    expect(publicEntry).toContain('engineering_verification_read_http_router');
    expect(adapters).toContain('pub mod native_engineering_verification_read_http;');
  });

  test('retirement:ts-statements drops the deleted prior statement', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(10);
    expect(total).toBeLessThan(21);
  });

  test('active and reply-guard native routes exist and TS statements are owner-local renderings', () => {
    const engineering = source('adapters/engineering-verification.ts');
    const reply = source('adapters/collaboration-reply-rollout.ts');
    const http = source('adapters/native_engineering_verification_read_http.rs');
    const client = source('adapters/native-engineering-verification-read/client.ts');
    const nativeRead = source('adapters/native_engineering_verification_read.rs');
    const nativeApp = source('application/native_engineering_verification_read.rs');
    const nativePublic = source('public/native_engineering_verification_read.rs');

    // The two TS adapters no longer own inline statements: each renders the
    // byte-identical SQL through an owner-local head, keeping their synchronous
    // SqlStatement callers in server-pms and server-collaboration unchanged.
    expect(engineering).toContain("releaseTableExistsSql('engineering_verification_contexts engineering'");
    expect(engineering).not.toContain('EXISTS(SELECT 1 FROM engineering_verification_contexts engineering');
    expect(reply).toContain("releaseAcceptanceRowSql('company_id','company_id=?',' FOR SHARE','engineering_verification_contexts')");
    expect(reply).not.toContain('SELECT company_id FROM engineering_verification_contexts');

    // Native reads and routes exist; the lead extends the mounted backend.
    expect(http).toContain('/platform/engineering-verification/read/active');
    expect(http).toContain('/platform/engineering-verification/read/reply-guard');
    expect(http).toContain('fn active<');
    expect(http).toContain('fn reply_rollout_guard<');
    expect(client).toContain('nativeEngineeringActiveRead');
    expect(client).toContain('nativeEngineeringReplyGuard');
    expect(nativeRead).toContain('pub const ACTIVE_SQL');
    expect(nativeRead).toContain('pub const REPLY_ROLLOUT_GUARD_SQL');
    expect(nativeRead).toContain('SELECT company_id FROM engineering_verification_contexts WHERE company_id=$1 FOR SHARE');
    expect(nativeApp).toContain('pub async fn read_active');
    expect(nativeApp).toContain('pub async fn read_reply_rollout_guard');
    expect(nativePublic).toContain('read_active');
    expect(nativePublic).toContain('read_reply_rollout_guard');
  });
});
