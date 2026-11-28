import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  createProjectStructureCommands,
  type ProjectStructureCurrent,
} from '@erpx/feature-projects/lib/project-structure-command';
import { canonicalDestinationLinks } from '@erpx/feature-work_projection/lib/destination-links';

const root = resolve(__dirname, '../../..');
const retired = [
  'packages/feature-projects/src/routes/api/v1/holdings/[holdingSlug]/projects/[id]/structure/route.ts',
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/projects/[id]/structure/route.ts',
  'apps/projects/src/app/api/v1/holdings/[holdingSlug]/projects/[id]/structure/route.ts',
];

const access = {
  newWork: true,
  settleExisting: true,
  history: true,
  revision: 'native-l11b',
};
const current = (): ProjectStructureCurrent =>
  ({
    base: '/holdings/acme',
    graph: {
      project: { id: 'project', company_id: 'company', version: 4, lifecycle_status: 'active' },
      isProjectOwner: true,
    },
    handoffs: [],
    actorId: 'owner',
    canWriteCosts: false,
    people: [],
    members: [],
    workLogs: [],
    costAllocations: [],
    purchases: {
      'projects.wbs': access,
      'projects.tasks': access,
      'projects.time': access,
      'projects.handover': access,
      'projects.costs': access,
    },
    draftVersion: 1,
  }) as unknown as ProjectStructureCurrent;

describe('L11b native Projects structure cutover', () => {
  it('has no Next route or app re-export for the structure command', () => {
    for (const path of retired) expect(existsSync(resolve(root, path)), path).toBe(false);
  });

  it('keeps canonical destination links in the client helper without changing command fields', () => {
    const receipt = {
      id: 'project',
      version: 5,
      status: 'active',
      taskId: 'task',
      bindingVersion: 2,
      handoffHref: '/api/v1/holdings/acme/projects/project/structure',
    };
    expect(canonicalDestinationLinks(receipt)).toEqual(receipt);
  });

  it('accepts the camel-case receipt shaped by the native response', async () => {
    const input = {
      action: 'assign-task',
      expectedVersion: 4,
      reason: 'Reviewed the exact task and package versions',
      taskId: 'task',
      taskVersion: 2,
      milestoneId: 'milestone',
      milestoneVersion: 1,
      bindingVersion: 1,
    };
    const calls: string[] = [];
    const commands = createProjectStructureCommands(async (url, init) => {
      calls.push(String(url));
      const key = new Headers(init?.headers).get('Idempotency-Key')!;
      // This is the exact {data,context} acknowledgement the native owner now
      // returns: camel-case, already-derived binding version, no TS receipt
      // derivation and no Next route in between.
      return Response.json({
        data: {
          id: 'project',
          version: 5,
          status: 'active',
          taskId: 'task',
          bindingVersion: 2,
        },
        context: { key },
      });
    });
    commands.bind(current);
    const result = await commands.send(input);
    expect(result.data).toMatchObject({ id: 'project', version: 5, bindingVersion: 2 });
    expect(result.acknowledge()).toBe(true);
    expect(calls).toEqual(['/api/v1/holdings/acme/projects/project/structure']);
  });
});
