import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';

const read = (file: string) =>
  readFileSync(fileURLToPath(new URL(`../../../${file}`, import.meta.url)), 'utf8');

const route = 'modules/pms/ui/src/routes/api/v1/holdings/[holdingSlug]/pms/imports/route.ts';
const compat = 'apps/compat/src/app/api/v1/holdings/[holdingSlug]/pms/imports/route.ts';
const pms = 'apps/pms/src/app/api/v1/holdings/[holdingSlug]/pms/imports/route.ts';

describe('PMS import-batch list Next route retirement', () => {
  it('removes only the batch-list GET from the shared handler and both app re-exports', () => {
    const source = read(route);
    expect(source).not.toContain('export async function GET');
    expect(source).not.toContain('pmsImportBatches');
    expect(source).toContain('export async function POST');
    expect(source).toContain('Idempotency-Key');
    for (const action of ['previewPmsImport', 'approvePmsImport', 'applyPmsImport'])
      expect(source).toContain(action);
    for (const file of [compat, pms]) {
      const reexport = read(file);
      expect(reexport).toContain('export { POST }');
      expect(reexport).not.toContain('GET');
    }
  });

  it('executes PMS SQL in the native owner and owns the bounded read and {data} envelope', () => {
    const sql = read('modules/pms/server/src/adapters/import_batches_postgres.rs');
    expect(sql).toContain('IMPORT_BATCHES_SQL');
    expect(sql).toContain('FROM pms_import_batches WHERE company_id=$1 AND property_id=$2');
    expect(sql).toContain('sqlx::query(IMPORT_BATCHES_SQL)');
    expect(sql).toContain('.fetch_all(&mut *connection)');
    const http = read('modules/pms/server/src/adapters/import_batches_http.rs');
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/pms/imports"');
    expect(http).toContain('get(import_batches)');
    expect(http).toContain('valid_token(&value, 160)');
    expect(http).toContain('if property.is_some() {');
    expect(http).toContain('json!({"data": batches})');
    expect(http).toContain('header::CACHE_CONTROL, "private, no-store"');
    const view = read('modules/pms/server/src/domain/import_batches.rs');
    expect(view).toContain('pub report: serde_json::Value');
    expect(read('platform/composition/src/native_lane_mounts.rs')).toContain(
      'erpx_server_pms::public::import_batches::router_for_holdings(',
    );
  });

  it('keeps the browser batch read on the same URL while the POST command stays on Next', () => {
    const client = read('modules/pms/ui/src/lib/import-command.ts');
    expect(client).toContain('new URLSearchParams({property:attempt.source.propertyId})');
    expect(client).toContain('method:"POST"');
    expect(client).toContain('"Idempotency-Key":attempt.key');
    expect(client).toContain('"data" in body?body.data:undefined');
    expect(client).not.toContain('canonicalDestinationLinks');
    const presenter = read(
      'platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/pms/import/screen.tsx',
    );
    expect(presenter).toContain('/pms/imports');
  });

  it('routes only the bounded GET read through the proposed native path entry', () => {
    const environment = {
      ERPX_DEV_NATIVE_PATHS: 'acme.jesadakorn.com|GET|/api/v1/holdings/acme/pms/imports',
    } as unknown as NodeJS.ProcessEnv;
    const base = '/api/v1/holdings/acme/pms/imports';
    expect(nativeApiPath(environment, `${base}?property=property-1`, 'acme.jesadakorn.com', 'GET')).toBe(true);
    expect(nativeApiPath(environment, base, 'acme.jesadakorn.com', 'POST')).toBe(false);
    expect(nativeApiPath(environment, `${base}?property=property-1`, 'unregistered.example.test', 'GET')).toBe(false);
  });
});
