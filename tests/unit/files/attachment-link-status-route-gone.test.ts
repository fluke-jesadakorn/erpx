import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { holdingHosts } from '@erpx/platform-contracts/holding-hosts';
import { describe, expect, it } from 'vitest';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';

const root = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const suffix = 'api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/status/route.ts';
const apps = [
  'recruit', 'launcher', 'learn', 'compat', 'sales', 'projects', 'admin', 'pms',
  'manufacturing', 'supply', 'pos', 'support', 'people', 'finance', 'service', 'legal', 'assets',
] as const;
const retired = [
  `platform/workplace/src/routes/${suffix}`,
  ...apps.map(app => `apps/${app}/src/app/${suffix}`),
];

describe('attachment link status is retired native transport', () => {
  it('has no Next route and no app re-export for the status read', () => {
    expect(retired).toHaveLength(18);
    for (const file of retired) expect(existsSync(resolve(root, file)), file).toBe(false);
  });

  it('serves the status payload from the exact native axum route backed by Files SQL', () => {
    const http = read('modules/files/server/src/adapters/attachment_http.rs');
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/status"');
    expect(http).toContain('get(status_of_link)');
    expect(http).toContain('.status(AttachmentReadHttpStatusRequest {');
    const useCase = read('modules/files/server/src/adapters/attachment_read_use_cases.rs');
    expect(useCase).toContain('pub async fn attachment_link_status');
    expect(useCase).toContain('ports.authorize_target(');
    const sql = read('modules/files/server/src/adapters/attachment_read_postgres.rs');
    expect(sql).toContain('const STATUS_SQL');
    expect(sql).toContain('sqlx::query_scalar(STATUS_SQL)');
  });

  it('keeps the browser {data} envelope and the client presentation helper', () => {
    expect(read('modules/files/ui/src/lib/attachment-client.ts')).toContain('return body.data;');
    expect(read('platform/workplace/src/lib/native-attachments.ts')).toContain(
      'canonicalDestinationLinks({ data: status })',
    );
  });

  it('planned gateway entry matches every registered Holding without a rewrite', () => {
    const gateway = {
      NODE_ENV: 'test' as const,
      ERPX_DEV_NATIVE_PATHS: holdingHosts
        .map(
          ({ hostname, holdingSlug }) =>
            `${hostname}|GET|/api/v1/holdings/${holdingSlug}/attachment-links/{linkId}/status`,
        )
        .join(','),
      ERPX_DEV_NATIVE_REWRITES: '',
    };
    for (const { hostname, holdingSlug } of holdingHosts) {
      const path = `/api/v1/holdings/${holdingSlug}/attachment-links/link-1/status`;
      expect(nativeApiPath(gateway, path, hostname, 'GET'), hostname).toBe(true);
      expect(nativeApiPath(gateway, path, hostname, 'POST'), hostname).toBe(false);
      expect(nativeApiPath(gateway, path, 'unregistered.example.test', 'GET')).toBe(false);
    }
  });
});
