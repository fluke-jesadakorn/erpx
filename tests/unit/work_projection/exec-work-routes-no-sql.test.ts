import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const route = (path: string) =>
  new URL(`../../../platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/${path}`, import.meta.url);

const read = (path: string) => readFile(route(path), 'utf8');

const sqlMarkers = [
  'SELECT ',
  'INSERT ',
  'UPDATE ',
  'DELETE ',
  'db.prepare(',
  'sqlx::',
  'query_scalar(',
];

describe('native exec.work routes carry no SQL', () => {
  for (const path of [
    'work/card-preferences/route.ts',
    'navigation/route.ts',
    'page-layouts/[pageKey]/route.ts',
  ]) {
    it(`${path} only delegates execution`, async () => {
      const source = await read(path);
      for (const marker of sqlMarkers) expect(source).not.toContain(marker);
    });
  }

  it('card-preferences delegates to the native owner client and wraps only {data}', async () => {
    const source = await read('work/card-preferences/route.ts');
    expect(source).toContain('readNativeWorkCardPreferences');
    expect(source).toContain('saveNativeWorkCardPreferences');
    expect(source).toContain('canonicalDestinationLinks({ data:');
    expect(source).not.toMatch(/\{\s*data:[^}]*,\s*context:/);
  });

  it('navigation delegates both reads and the read-home command', async () => {
    const source = await read('navigation/route.ts');
    expect(source).toContain('navigationSnapshot');
    expect(source).toContain('markHomeRead');
    expect(source).toContain('unreadHomeCount');
    expect(source).toContain("searchParams.get(\"include\") === \"attention\"");
    expect(source).toContain('canonicalDestinationLinks({ data }');
  });

  it('page-layouts is a pre-cutover shell over the owner request', async () => {
    const source = await read('page-layouts/[pageKey]/route.ts');
    expect(source).toContain('pageLayoutRequest');
    expect(source).toContain('export { handle as GET, handle as POST }');
  });
});

describe('Rust native ingress owns the wire envelope and SQL', () => {
  const rust = (path: string) =>
    new URL(`../../../modules/work-projection/server/src/${path}`, import.meta.url);

  it('card-preferences executes the owner SQL readers', async () => {
    const source = await readFile(rust('adapters/work_card_preferences.rs'), 'utf8');
    expect(source).toContain('SELECT pinned_ids_json,version FROM work_card_preferences');
    expect(source).toContain('INSERT INTO work_card_preferences');
    expect(source).toContain('execute_work_card_preferences');
  });

  it('navigation router wraps {data} and enforces the exact body', async () => {
    const source = await readFile(rust('adapters/work_navigation_http.rs'), 'utf8');
    expect(source).toContain('canonical_envelope');
    expect(source).toContain('{"action": "read-home"}');
    expect(source).toContain('/api/v1/holdings/{holding_slug}/navigation');
  });

  it('page-layouts router owns the configured page fence and strict input', async () => {
    const source = await readFile(rust('adapters/page_layouts_http.rs'), 'utf8');
    expect(source).toContain('is_configurable_page');
    expect(source).toContain('parse_page_layout_input');
    expect(source).toContain('/api/v1/holdings/{holding_slug}/page-layouts/{page_key}');
  });

  it('page-layouts SQL is a bound owner read', async () => {
    const source = await readFile(rust('adapters/page_layouts_read.rs'), 'utf8');
    expect(source).toContain('SELECT owner_id,layout_json,version FROM page_layouts');
    expect(source).toContain('fetch_all(&mut *connection)');
  });
});
