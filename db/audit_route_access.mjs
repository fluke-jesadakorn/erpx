// CI sweep: hits every page and API route with each role's session and asserts the expected status.
// Run after starting both web-admin (port 3003) and rbac (port 3100):
//   node folio/db/audit_route_access.mjs

import { setTimeout as sleep } from 'node:timers/promises';
import { createHmac } from 'node:crypto';

const BASE = process.env.WEB_BASE || 'http://localhost:3003';
const SECRET = process.env.SESSION_SECRET || '';

if (!SECRET) {
  console.error('SESSION_SECRET is required');
  process.exit(2);
}

const SESSION_SECRET = SECRET;

function b64u(buf) {
  return Buffer.from(buf).toString('base64')
    .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

async function signSession(sub, role) {
  const now = Math.floor(Date.now() / 1000);
  const payload = { sub, role, iat: now, exp: now + 86400 };
  const head = b64u(Buffer.from(JSON.stringify(payload), 'utf8'));
  const sig = b64u(createHmac('sha256', SESSION_SECRET).update(head).digest());
  return `${head}.${sig}`;
}

// Discover seeded users per role
async function fetchUsers() {
  // dev-only endpoint to enumerate users via /api/actor + /api/ai/providers? not great.
  // Use the database via the OCR service? Simplest: use a known dev user-id per role.
  // Operators must populate ACTOR_USERS env as JSON.
  const raw = process.env.ACTOR_USERS;
  if (!raw) {
    console.error('Set ACTOR_USERS={"staff":1,"cfo":7,...} for the audit run.');
    process.exit(2);
  }
  return JSON.parse(raw);
}

async function sessionFor(userId, role) {
  return await signSession(userId, role);
}

const ROUTES = [
  { method: 'GET', path: '/',                       expect: { anyOf: [200] } },
  { method: 'GET', path: '/dashboard',              expect: { anyOf: [200] } },
  { method: 'GET', path: '/expense',                expect: { anyOf: [200, 307, 308] } },
  { method: 'GET', path: '/expense-claim',          expect: { anyOf: [308] } },
  { method: 'GET', path: '/my-waybills',            expect: { anyOf: [200] } },
  { method: 'GET', path: '/waybill/WB-2026-000001', expect: { anyOf: [200, 404] } },
  { method: 'GET', path: '/approve-expense',        expect: { anyOf: [308] } },
  { method: 'GET', path: '/all-approvals',          expect: { anyOf: [308] } },
  { method: 'GET', path: '/my-prs',                 expect: { anyOf: [308] } },
  { method: 'GET', path: '/cockpit',                expect: { roles: { cfo: [200], staff: [404] }, anyOf: [200, 404] } },
  { method: 'GET', path: '/ledger',                 expect: { roles: { accountant: [200], staff: [404] }, anyOf: [200, 404] } },
  { method: 'GET', path: '/org-chart',              expect: { roles: { hr_manager: [200], staff: [404] }, anyOf: [200, 404] } },
  { method: 'GET', path: '/directory',              expect: { roles: { hr_manager: [200], staff: [404] }, anyOf: [200, 404] } },
  { method: 'GET', path: '/departments',            expect: { roles: { hr_manager: [200], hr: [404] }, anyOf: [200, 404] } },
  { method: 'GET', path: '/access-requests',        expect: { roles: { hr_manager: [200], staff: [404] }, anyOf: [200, 404] } },
  { method: 'GET', path: '/api/ai/providers',       expect: { roles: { it: [200], cfo: [403], staff: [403] }, anyOf: [200, 403] } },
  { method: 'GET', path: '/api/ai/invocations',     expect: { roles: { it: [200], cfo: [200], staff: [403] }, anyOf: [200, 403] } },
  { method: 'GET', path: '/api/ai/sections/health', expect: { roles: { it: [200], staff: [403] }, anyOf: [200, 403] } },
  { method: 'GET', path: '/api/slips/file?key=nope',expect: { anyOf: [400, 401, 403, 404] } },
  { method: 'POST', path: '/api/upload',            expect: { anyOf: [400, 401, 403] } },
];

async function hit(method, path, token) {
  const headers = { cookie: `folio_session=${token}` };
  const init = { method, headers, redirect: 'manual' };
  const res = await fetch(BASE + path, init);
  return res.status;
}

async function run() {
  const users = await fetchUsers();
  let total = 0, fail = 0;
  for (const [role, userId] of Object.entries(users)) {
    const token = await sessionFor(userId, role);
    for (const r of ROUTES) {
      total++;
      const status = await hit(r.method, r.path, token);
      let expected = r.expect.anyOf;
      if (r.expect.roles && r.expect.roles[role]) expected = r.expect.roles[role];
      const ok = expected.includes(status);
      if (!ok) {
        fail++;
        console.log(`✗ ${role.padEnd(15)} ${r.method.padEnd(4)} ${r.path.padEnd(28)} → ${status} (expected one of ${expected.join(',')})`);
      } else {
        console.log(`✓ ${role.padEnd(15)} ${r.method.padEnd(4)} ${r.path.padEnd(28)} → ${status}`);
      }
    }
  }
  console.log(`\n${total - fail}/${total} ok`);
  process.exit(fail > 0 ? 1 : 0);
}

await run();