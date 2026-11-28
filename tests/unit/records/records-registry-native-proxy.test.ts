import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

describe('Rust owner execution and payload parity pins', () => {
  it('executes owner SQL in the Rust registry adapter under a serializable read-only unit', () => {
    const sql = read('modules/records/server/src/adapters/record_registry.rs');
    expect(sql).toContain('FROM workspace_records WHERE archived_at IS NULL');
    expect(sql).toContain('SHOW transaction_isolation');
    expect(sql).toContain('SHOW transaction_read_only');
    expect(sql).toContain('"serializable"');
    expect(sql).toContain('"on"');
    expect(sql).not.toContain('include_str!');
    const composition = read('platform/composition/src/records.rs');
    expect(composition).toContain('impl<C: Clock + Send + Sync + \'static> RecordsRegistryHttpBackend for RecordsBackend<C>');
    expect(composition).toContain('PgRecordRegistry::in_authorized_work(connection)');
    expect(composition).toContain('record_registry_page(');
    expect(composition).toContain('RecordsRegistryHttpResponse {');
  });

  it('pins the exact Rust wire envelope and link-free field set', () => {
    const http = read('modules/records/server/src/adapters/record_registry_http.rs');
    expect(http).toContain('serde_json::json!({"data": self.data, "context": self.context})');
    for (const route of [
      '/api/v1/holdings/{holding_slug}/records/{module_id}',
      '/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}',
      '/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/references',
      '/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/transition',
    ])
      expect(http).toContain(route);
    const context = read('modules/records/server/src/domain/record_context.rs');
    for (const field of ['moduleId', 'id', 'sectionId', 'title', 'description', 'status', 'fields', 'version', 'createdBy', 'updatedAt'])
      expect(context).toContain(`"${field}"`);
    expect(context).not.toMatch(/"(?:href|\w*Href|\w*_href)"/);
    const registry = read('modules/records/server/src/domain/record_registry.rs');
    for (const field of ['items', 'nextCursor', 'previousCursor', 'hasMore'])
      expect(registry).toContain(`"${field}"`);
  });
});
