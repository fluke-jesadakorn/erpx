// tests/safe_ui.test.mjs
// Parser tests for the safe interactive UI contract.

import { test } from 'node:test';
import assert from 'node:assert/strict';

const ALLOWED = new Set(['root','stack','grid','card','heading','text','metric','table','tabs','accordion','button','note']);

function asNode(v, depth = 0) {
  if (depth > 6) return null;
  if (!v || typeof v !== 'object') return null;
  const o = v;
  const t = String(o.type ?? '');
  if (!ALLOWED.has(t)) return null;
  if (Array.isArray(o.children)) {
    for (const child of o.children) {
      const c = asNode(child, depth + 1);
      if (!c) return null;
    }
  }
  return { type: t, ok: true };
}

function parse(body) {
  try { return JSON.parse(body); } catch { return null; }
}

test('rejects unknown component type', () => {
  const n = asNode({ type: 'script', children: [] });
  assert.equal(n, null);
});

test('accepts allow-listed component types', () => {
  for (const t of ['root','stack','grid','card','heading','text','metric','table','tabs','accordion','button','note']) {
    assert.ok(asNode({ type: t }), t);
  }
});

test('rejects raw <script> wrapped JSON', () => {
  const body = '{"type":"root","children":[{"type":"text","text":"<script>alert(1)</script>"}]}';
  const p = parse(body);
  const n = asNode(p, 0);
  assert.ok(n);
  const text = p.children[0];
  // text payload is treated as literal string and never executed
  assert.match(text.text, /<script>/);
});

test('depth cap blocks deeply nested payload', () => {
  let v = { type: 'text', text: 'x' };
  for (let i = 0; i < 8; i++) v = { type: 'card', children: [v] };
  const n = asNode(v, 0);
  assert.equal(n, null);
});

test('malformed JSON returns null', () => {
  const p = parse('not-json');
  assert.equal(p, null);
});