// Grep guard: no remaining permission-gating `role_name === 'X'` literal
// checks. The matrix (rbac.*) is the source of truth for permission
// decisions. Display-only checks (workspace switching, persona labels)
// are allowed because they reflect the persona the actor sees themselves
// as, not whether they can perform an action.
//
// A "permission gate" is one that:
//   - is followed by throw / 403 / forbidden, OR
//   - controls a button visibility / route access
//
// We only catch the former for now (throws) — that's the strict guarantee.

import { readFileSync, readdirSync, statSync } from 'fs';
import { join } from 'path';

const ROOT = new URL('../src/', import.meta.url).pathname;

let pass = 0;
let fail = 0;
const offenders = [];

const walk = (dir) => {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const s = statSync(full);
    if (s.isDirectory()) {
      walk(full);
      continue;
    }
    if (!entry.endsWith('.ts') && !entry.endsWith('.tsx')) continue;
    const text = readFileSync(full, 'utf8');
    const lines = text.split('\n');
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const m = line.match(/role_name\s*===\s*['"]([a-z_]+)['"]/);
      if (!m) continue;
      // Look ahead 6 lines for a throw / forbidden / 403
      const window = lines.slice(i, i + 6).join('\n');
      if (/throw\s+(new\s+)?(Error|GuardError)|forbidden\(\)|status:\s*403|return\s+forbidden/.test(window)) {
        offenders.push(`${full.slice(ROOT.length)}:${i + 1}  role_name === '${m[1]}'`);
      }
    }
  }
};

walk(ROOT);

if (offenders.length === 0) {
  pass++;
  console.log('PASS: no leftover permission-gating role_name === "X" checks');
} else {
  fail++;
  console.error('FAIL: permission-gating role_name === "X" checks still present:');
  for (const f of offenders) console.error('  ' + f);
}

console.log(`Result: ${pass} pass, ${fail} fail`);
process.exit(fail > 0 ? 1 : 0);