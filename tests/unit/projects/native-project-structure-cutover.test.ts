import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';
import {
  createNativeProjectStructureClient,
  type NativeProjectsPageContext,
} from '../../../modules/projects/ui/src/public/native-project-structure';
import { nativeProjectsConfiguration } from '../../../modules/projects/ui/src/public/native-project-wbs';

const root = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const handler =
  'modules/projects/ui/src/routes/api/v1/holdings/[holdingSlug]/projects/[id]/structure/route.ts';
const reexports = [
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/projects/[id]/structure/route.ts',
  'apps/projects/src/app/api/v1/holdings/[holdingSlug]/projects/[id]/structure/route.ts',
];

describe('native Projects structure command cutover', () => {
  it('has no Next route or app re-export; the native owner serves the exact path', () => {
    expect(existsSync(resolve(root, handler)), handler).toBe(false);
    for (const path of reexports)
      expect(existsSync(resolve(root, path)), path).toBe(false);
    const http = read('modules/projects/server/src/adapters/project_structure_http.rs');
    expect(http).toContain('pub fn project_structure_router');
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/projects/{project_id}/structure"');
    expect(http).toContain('post(structure)');
  });

  it('accepts the exact host|POST|template gateway entry the lead must add', () => {
    const env = {
      ERPX_DEV_NATIVE_PATHS:
        'acme.jesadakorn.com|POST|/api/v1/holdings/{holding}/projects/{id}/structure',
    } as unknown as NodeJS.ProcessEnv;
    const path = '/api/v1/holdings/acme/projects/p1/structure';
    expect(nativeApiPath(env, path, 'acme.jesadakorn.com', 'POST')).toBe(true);
    expect(nativeApiPath(env, path, 'acme.jesadakorn.com', 'GET')).toBe(false);
  });

  it('returns the native {data,context} receipt consumed by the browser client as data', async () => {
    const config = nativeProjectsConfiguration(
      JSON.stringify({
        privateBaseUrl: 'http://127.0.0.1:44992',
        timeoutMs: 1000,
        holdings: [
          { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
        ],
      }),
    );
    const actor: NativeProjectsPageContext = {
      userId: 'user',
      holdingId: 'h001',
      holdingSlug: 'acme',
      companyId: 'company',
      branchId: null,
      departmentId: null,
      assignmentId: 'assignment',
      role: 'owner',
      assignmentScope: 'holding',
      scope: { kind: 'holding' },
      authority: { revision: 7, assignmentVersion: 3 },
      modules: ['projects'],
      iam: { permissions: ['records.read'] },
    };
    const context = {
      userId: 'user',
      holdingId: 'h001',
      companyId: 'company',
      branchId: null,
      departmentId: null,
      branchSlug: null,
      departmentSlug: null,
      assignmentId: 'assignment',
      assignmentVersion: 3,
      revision: 7,
      role: 'owner',
      assignmentScopeKind: 'holding',
      modules: ['projects'],
      permissions: ['records.read'],
      expiresAt: 9_007_199_254_740_991,
    };
    const data = {
      id: 'project',
      version: 5,
      status: 'active',
      task_id: 'task',
      binding_version: 2,
    };
    const envelope = { data, context };
    const transport = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(envelope), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const result = await createNativeProjectStructureClient(config, transport).command(
      new Headers({
        host: 'acme.example.com',
        origin: 'https://acme.example.com',
        cookie: '__Host-erpx-product=session',
        'x-erpx-context-company': 'company',
        'x-erpx-context-assignment': 'assignment',
        'x-erpx-context-revision': '7',
        'x-erpx-context-assignment-version': '3',
      }),
      actor,
      {
        projectId: 'project',
        key: 'command-key-0001',
        body: {
          action: 'assign-task',
          expectedVersion: 4,
          taskId: 'task',
          bindingVersion: 1,
        },
      },
    );
    expect(result).toEqual({
      id: 'project',
      version: 5,
      status: 'active',
      taskId: 'task',
      bindingVersion: 2,
    });
    const [method, url] = transport.mock.calls[0]!;
    expect(method).toBe('POST');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44992/api/v1/holdings/acme/projects/project/structure',
    );
  });
});
