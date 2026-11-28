import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, it } from 'vitest';
import { holdingNativeCutoverPath, normalizeHoldingNativePath } from '../../../scripts/local/native-cutover-routes';
const root = resolve(__dirname, '../../..');
const base = '/api/v1/holdings/acme/routine-templates';
it('retires the full Next route and sends scoped template commands to Axum', () => {
  const route = 'api/v1/holdings/[holdingSlug]/routine-templates/[[...path]]/route';
  for (const directory of ['products/apps/ui/src/routes', 'apps/launcher/src/app', 'apps/compat/src/app'])
    expect(existsSync(resolve(root, directory, route + '.ts'))).toBe(false);
  const manifest = JSON.parse(readFileSync(resolve(root, 'products/apps/ui/package.json'), 'utf8'));
  expect(manifest.exports['./routes/' + route]).toBeUndefined();
  expect(holdingNativeCutoverPath(base, 'acme', 'POST')).toBe('routine-command');
  for (const suffix of ['/template', '/template/clone', '/template/run', '/template/schedules', '/template/schedules/schedule']) {
    expect(holdingNativeCutoverPath(base + suffix, 'acme', 'POST')).toBe('routine-command');
    expect(holdingNativeCutoverPath(base + suffix, 'acme', 'GET')).toBe('routine-templates');
  }
  const scoped = normalizeHoldingNativePath('/api/v1/holdings/acme/branches/bangkok/departments/ops/routine-templates/template/run', 'acme');
  expect(scoped).toEqual({url: base + '/template/run', workspace: {branch: 'bangkok', department: 'ops'}});
  expect(holdingNativeCutoverPath(scoped.url, 'acme', 'POST')).toBe('routine-command');
  expect(holdingNativeCutoverPath(base + '/template/schedules/schedule', 'northstar', 'GET')).toBeUndefined();
});

it('retires the schedule catch-all and routes all three state commands and legacy methods to Axum', () => {
  const route = 'api/v1/holdings/[holdingSlug]/routine-schedules/[[...path]]/route';
  for (const directory of ['products/apps/ui/src/routes', 'apps/launcher/src/app', 'apps/compat/src/app'])
    expect(existsSync(resolve(root, directory, route + '.ts'))).toBe(false);
  const feature = JSON.parse(readFileSync(resolve(root, 'products/apps/ui/package.json'), 'utf8'));
  const owner = JSON.parse(readFileSync(resolve(root, 'modules/operations/server/package.json'), 'utf8'));
  expect(feature.exports['./routes/' + route]).toBeUndefined();
  expect(owner.exports['./public/native-routines-page']).toBeUndefined();
  expect(existsSync(resolve(root, 'modules/operations/server/src/public/native-routines-page.ts'))).toBe(false);

  const schedules = '/api/v1/holdings/acme/routine-schedules';
  for (const operation of ['pause', 'resume', 'end'])
    expect(holdingNativeCutoverPath(schedules + '/schedule/' + operation + '?ignored=true', 'acme', 'POST')).toBe('routine-command');
  for (const method of ['GET', 'HEAD', 'OPTIONS', 'PUT'])
    expect(holdingNativeCutoverPath(schedules + '/schedule/unknown', 'acme', method)).toBe('routine-schedules');
  expect(holdingNativeCutoverPath(schedules, 'acme', 'GET')).toBe('routine-schedules');
  expect(holdingNativeCutoverPath(schedules + '/schedule/unknown', 'acme', 'POST')).toBe('routine-command');
  expect(holdingNativeCutoverPath(schedules + '/%2Fother/pause', 'acme', 'POST')).toBeUndefined();
  expect(holdingNativeCutoverPath(schedules + '/schedule/end', 'northstar', 'POST')).toBeUndefined();
});
