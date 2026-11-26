import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const owner = new URL(
  '../../../packages/server-agent/src/adapters/native_assistant_sources/',
  import.meta.url,
);
const read = (name: string) => readFileSync(new URL(name, owner), 'utf8');

describe('native Agent assistant source slice', () => {
  it('executes the scope-bound statement in the Rust adapter, not a SQL plan', () => {
    const postgres = read('postgres.rs');
    const domain = read('domain.rs');
    expect(postgres).toContain('sqlx::query(SQL_ASSISTANT_SOURCES)');
    expect(postgres).toContain('.fetch_all(&mut *connection)');
    expect(postgres.match(/\.bind\(/g)).toHaveLength(5);
    expect(postgres).toContain('PgUnitOfWork::new');
    expect(domain).toContain("company_id=$1 AND t.user_id=$2 AND t.assignment_id=$3 AND t.scope_key=$4");
    expect(postgres + domain).not.toContain('include_str!');
  });

  it('serves the complete sourceReference shape with a guide floor', () => {
    const domain = read('domain.rs');
    const application = read('application.rs');
    for (const field of ['pub id:', 'pub title:', 'pub path:', 'pub version:', 'pub source:'])
      expect(domain).toContain(field);
    expect(domain).toContain('skip_serializing_if = "Option::is_none"');
    expect(application).toContain('if sources.is_empty()');
    expect(application).toContain('guide_reference(&scoped.path)');
    expect(application).toContain('json!({"sources": sources})');
  });

  it('exposes one axum GET route with exact Holding targets', () => {
    const http = read('http.rs');
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/assistant/sources"');
    expect(http).toContain('routing::get');
    expect(http).toContain('router_for_holdings');
    expect(http).toContain('router_with_reader');
  });
});
