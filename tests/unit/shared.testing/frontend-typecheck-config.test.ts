import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { appDefinition, appDefinitions } from '@erpx/contracts/apps';
import { repositoryRoot, type Suite } from '../../../scripts/testing/catalogue';
import { createWorkspaceGraph, selectCandidateSuites } from '../../../scripts/testing/workspace-graph';
import { workspaceTaskDefinition } from '../../../scripts/testing/workspace-tasks';

const configuration = 'tests/config/tsconfig.frontend.json';
const read = (file: string) => JSON.parse(readFileSync(resolve(repositoryRoot, file), 'utf8'));

describe('standalone frontend package typechecks', () => {
  it('declares Next ambient CSS types without relying on an app-generated next-env file', () => {
    expect(read(configuration)).toEqual({ extends: '../../tsconfig.base.json', compilerOptions: { types: ['node', 'bun', 'next'] } });
    expect(read('tsconfig.base.json').compilerOptions.types ?? []).not.toContain('next');
  });

  it('applies frontend ambient types to feature and shell packages while keeping backend contracts separate', () => {
    const directories = readdirSync(resolve(repositoryRoot, 'packages'), { withFileTypes: true }).filter(entry => entry.isDirectory());
    for (const entry of directories) {
      if (!existsSync(resolve(repositoryRoot, `packages/${entry.name}/package.json`))) continue;
      const manifest = read(`packages/${entry.name}/package.json`);
      if (!manifest.name) continue;
      const frontend = entry.name.startsWith('feature-') || ['ui', 'workplace', 'front'].includes(entry.name);
      expect(read(`packages/${entry.name}/tsconfig.json`).extends, entry.name).toBe(frontend ? '../../tests/config/tsconfig.frontend.json' : '../../tsconfig.base.json');
    }
  });

  it('includes the shared frontend config in verification cache inputs and affected app selection', () => {
    const files = new Map([['apps/finance/src/app/page.tsx', 'export const Page=()=>null;'], [configuration, '{"compilerOptions":{"types":["next"]}}']]);
    const before = createWorkspaceGraph(files), afterFiles = new Map(files); afterFiles.set(configuration, '{"compilerOptions":{"types":[]}}');
    const after = createWorkspaceGraph(afterFiles), task = workspaceTaskDefinition(before, appDefinition('finance'));
    expect(task.tasks.typecheck.inputs).toContain('$TURBO_ROOT$/' + configuration);
    const suite: Suite = { id: 'finance.fixture.unit', owner: 'finance', lane: 'unit', modules: ['finance'], files: ['tests/unit/finance/fixture.test.ts'], fixture: 'none', requires: { jobs: false, scanner: false }, apps: ['finance'], packages: [] };
    expect(selectCandidateSuites(after, before, [suite], [configuration]).apps).toEqual(appDefinitions.map(app => app.id));
  });
});
