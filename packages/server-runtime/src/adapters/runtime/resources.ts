import { fileURLToPath } from 'node:url';
import { dirname, isAbsolute, resolve, sep } from 'node:path';

/** Composition roots supply the resource root; invocation cwd is never an input. */
export function runtimeResourcePath(path: string) {
  if (!path || isAbsolute(path) || path.includes('\\') || path.split('/').some(part => !part || part === '.' || part === '..'))
    throw new Error('INVALID_RUNTIME_RESOURCE_PATH');
  const configured = process.env.ERPX_RESOURCE_ROOT ?? process.env.ERPX_REPOSITORY_ROOT;
  if (configured && !isAbsolute(configured)) throw new Error('ABSOLUTE_RUNTIME_RESOURCE_ROOT_REQUIRED');
  const root = configured ?? resolve(dirname(fileURLToPath(import.meta.url)), '../../../../../');
  const target = resolve(root, path);
  if (!target.startsWith(resolve(root) + sep)) throw new Error('RUNTIME_RESOURCE_ESCAPES_ROOT');
  return target;
}
