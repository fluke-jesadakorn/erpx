// Grep guard: no remaining hardcoded boolean flags derived from
// role_name strings (e.g. `isHrManager = role === 'hr_manager'`).
// All such flags should now come from the matrix.

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
      // Match: const isXxx = role === 'Y' OR role === 'Y' || role === 'Z'
      const m = line.match(/const\s+(is\w+)\s*[:=]\s*(role[a-z_]*|a\.role_name|actor\.role_name|currentUser\.role_name)\s*===\s*['"]([a-z_]+)['"]/);
      if (m) {
        offenders.push(`${full.slice(ROOT.length)}:${i + 1}  ${m[1]} = ${m[2]} === '${m[3]}'`);
      }
    }
  }
};

walk(ROOT);

if (offenders.length === 0) {
  pass++;
  console.log('PASS: no leftover hardcoded isXxx = role === "Y" checks');
} else {
  fail++;
  console.error('FAIL: hardcoded role-derived booleans still present:');
  for (const f of offenders) console.error('  ' + f);
}

console.log(`Result: ${pass} pass, ${fail} fail`);
process.exit(fail > 0 ? 1 : 0);