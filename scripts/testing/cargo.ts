import { existsSync, readFileSync } from 'node:fs';
import { posix, resolve } from 'node:path';
import type { Suite } from './catalogue';
import { vitestChildEnvironment } from './runner-environment';
import { qualifiedNativeLinkEnvironment } from './native-worker-link-flags';

export type CargoSuite = { framework: 'cargo'; cargo: { package: string; targets: string[]; features?: string[] } };
export type CargoTestTarget = { name: string; path: string; requiredFeatures?: string[] };
export type CargoCaseObservation = { runner: 'cargo'; file: string; title: string; status: 'passed' | 'failed' | 'skipped' };
export const cargoRootInputs = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'] as const;
export const isCargoInput = (file: string) => file.endsWith('.rs') || /(?:^|\/)Cargo\.(?:toml|lock)$/.test(file) || file === 'rust-toolchain.toml';

/** Declared owner data is admitted by the same manifest at each Git revision.
 * This does not admit arbitrary extensions or private environment/cache paths.
 */
export function cargoResourceRoots(files:ReadonlyMap<string,string>):Map<string,string[]> {
  const result=new Map<string,string[]>();
  for(const[manifest,text]of files){
    if(manifest==='Cargo.toml'||!manifest.endsWith('/Cargo.toml')||!cargoPackageName(text))continue;
    const metadata=/^\[package\.metadata\.erpx\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(text)?.[1];
    const declared=metadata&&/^verification-roots\s*=\s*\[([\s\S]*?)\]\s*(?:#[^\n]*)?$/m.exec(metadata)?.[1];
    if(metadata&&/verification-roots/.test(metadata)&&declared===undefined)throw new Error(`CARGO_RESOURCE_ROOT_INVALID:${manifest}`);
    const roots:string[]=[];
    for(const match of declared?.matchAll(/"([^"\n]+)"/g)??[]){
      const root=posix.normalize(posix.join(posix.dirname(manifest),match[1]));
      if(match[1].startsWith('/')||match[1].includes('\\')||root==='..'||root.startsWith('../')||!cargoResourcePath(root))throw new Error(`CARGO_RESOURCE_ROOT_INVALID:${manifest}`);
      roots.push(root);
    }
    result.set(manifest,roots);
  }
  return result;
}
export function cargoResourcePath(file:string){return !file.split('/').some(part=>/^\.env(?:\.|$)/.test(part)||['.git','.scratch','.runtime','target','node_modules'].includes(part));}

/** Native suites declare exact integration-test targets, never an entire workspace fallback. */
export function cargoTestTargets(manifest: string): CargoTestTarget[] {
  return [...manifest.matchAll(/^\[\[test\]\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/gm)].map(match => {
    const name = /^name\s*=\s*"([A-Za-z0-9_-]+)"\s*$/m.exec(match[1])?.[1];
    const path = /^path\s*=\s*"([^"\n]+)"\s*$/m.exec(match[1])?.[1];
    if (!name || !path) throw new Error('EXPLICIT_CARGO_TEST_TARGET_REQUIRED');
    const required=/^required-features\s*=\s*\[([\s\S]*?)\]\s*$/m.exec(match[1]);
    const requiredFeatures=required?[...required[1].matchAll(/"([A-Za-z0-9_-]+)"/g)].map(feature=>feature[1]):[];
    if(/^required-features\s*=/m.test(match[1])&&!required)throw new Error('EXPLICIT_CARGO_TEST_FEATURES_REQUIRED');
    return { name, path, ...(requiredFeatures.length?{requiredFeatures}:{}) };
  });
}

export function cargoPackageName(manifest: string): string | undefined {
  const section = /^\[package\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(manifest)?.[1];
  return section && /^name\s*=\s*"([A-Za-z0-9_-]+)"\s*$/m.exec(section)?.[1];
}

export function cargoSuiteArguments(suite: Pick<Suite, 'id' | 'framework' | 'cargo'>, forwarded: readonly string[] = []): string[] {
  if (suite.framework !== 'cargo' || !suite.cargo || !suite.cargo.targets.length) throw new Error(`CARGO_SUITE_REQUIRED:${suite.id}`);
  if (forwarded.length) throw new Error(`CARGO_FORWARD_ARGUMENTS_UNSUPPORTED:${suite.id}`);
  return ['test', '--locked', '--package', suite.cargo.package, ...suite.cargo.targets.flatMap(target => ['--test', target]), ...(suite.cargo.features?.length ? ['--features', suite.cargo.features.join(',')] : [])];
}

export function cargoTargetInvocations(suite: Suite, root: string, forwarded: readonly string[] = []) {
  validateCargoSuite(suite, root);
  const manifest = suite.verificationInputs!.find(file => file.endsWith('/Cargo.toml') && cargoPackageName(readFileSync(resolve(root, file), 'utf8')) === suite.cargo!.package)!;
  const targets = cargoTestTargets(readFileSync(resolve(root, manifest), 'utf8'));
  return suite.cargo!.targets.map(target => ({ target, file: posix.normalize(posix.join(posix.dirname(manifest), targets.find(item => item.name === target)!.path)), arguments: cargoSuiteArguments({ ...suite, cargo: { ...suite.cargo!, targets: [target] } }, forwarded) }));
}

/** The pinned stable libtest summary must account for every observed case.
 * Exit zero with ignored, filtered, empty or missing cases is not a pass.
 */
export function cargoCaseResults(output: string, file: string) {
  const cases: CargoCaseObservation[] = [...output.matchAll(/^test (.+?) \.\.\. (ok|FAILED|ignored[^\n]*)$/gm)].map(match => ({ runner: 'cargo', file, title: match[1], status: match[2] === 'ok' ? 'passed' : match[2] === 'FAILED' ? 'failed' : 'skipped' }));
  const summaries = [...output.matchAll(/^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;/gm)];
  const summary = summaries[0];
  const passed = summaries.length === 1 && summary?.[1] === 'ok' && Number(summary[2]) > 0 && [3, 4, 5, 6].every(index => Number(summary[index]) === 0) && cases.length === Number(summary[2]) && cases.every(test => test.status === 'passed') && new Set(cases.map(test => test.title)).size === cases.length;
  return { version: 1 as const, framework: 'cargo' as const, acceptance: false as const, file, passed, cases };
}

export function validateCargoSuite(suite: Suite, root: string): void {
  if (suite.framework !== 'cargo') {
    if (suite.cargo !== undefined) throw new Error(`CARGO_CONFIG_WITHOUT_FRAMEWORK:${suite.id}`);
    if (suite.framework !== undefined && suite.framework !== 'vitest') throw new Error(`UNKNOWN_SUITE_FRAMEWORK:${suite.id}`);
    return;
  }
  const configuration = suite.cargo;
  if (!['unit', 'integration', 'provider'].includes(suite.lane) || !configuration || !/^[a-z][a-z0-9_-]+$/.test(configuration.package) || !Array.isArray(configuration.targets) || !configuration.targets.length || new Set(configuration.targets).size !== configuration.targets.length || configuration.targets.some(target => !/^[A-Za-z0-9_-]+$/.test(target)) || configuration.features?.some(feature => !/^[A-Za-z0-9_/-]+$/.test(feature))) throw new Error(`INVALID_CARGO_SUITE:${suite.id}`);
  if (suite.lane === 'provider' && (suite.fixture !== 'provider' || suite.postgres !== undefined || !suite.provider || !Array.isArray(suite.provider.required) || !suite.provider.required.length || suite.provider.required.some(name => !/^[A-Z][A-Z0-9_]*$/.test(name)) || !suite.provider.environment || Object.entries(suite.provider.environment).some(([name,value]) => !/^[A-Z][A-Z0-9_]*$/.test(name) || typeof value !== 'string'))) throw new Error(`CARGO_PROVIDER_PREREQUISITES_REQUIRED:${suite.id}`);
  const manifestFile = suite.verificationInputs?.find(file => file.endsWith('/Cargo.toml') && existsSync(resolve(root, file)) && cargoPackageName(readFileSync(resolve(root, file), 'utf8')) === configuration.package);
  if (!manifestFile) throw new Error(`CARGO_PACKAGE_MANIFEST_REQUIRED:${suite.id}`);
  const targets = cargoTestTargets(readFileSync(resolve(root, manifestFile), 'utf8'));
  const selected = configuration.targets.map(name => {
    const target = targets.find(target => target.name === name);
    if (!target) throw new Error(`CARGO_TEST_TARGET_UNDECLARED:${suite.id}:${name}`);
    if(target.requiredFeatures?.some(feature=>!configuration.features?.includes(feature)))throw new Error(`CARGO_REQUIRED_FEATURE_MISSING:${suite.id}:${name}`);
    return posix.normalize(posix.join(posix.dirname(manifestFile), target.path));
  });
  if (JSON.stringify([...selected].sort()) !== JSON.stringify([...suite.files].sort()) || suite.files.some(file => !file.endsWith('.test.rs'))) throw new Error(`CARGO_TARGET_FILE_MISMATCH:${suite.id}`);
  for (const file of suite.files) if (!/#\[(?:[A-Za-z_][A-Za-z0-9_]*::)?test(?:\]|\()/.test(readFileSync(resolve(root, file), 'utf8'))) throw new Error(`CARGO_TEST_CASE_REQUIRED:${suite.id}:${file}`);
  for (const file of cargoRootInputs) if (!suite.verificationInputs?.includes(file) || !existsSync(resolve(root, file))) throw new Error(`CARGO_SOURCE_INPUT_REQUIRED:${suite.id}:${file}`);
}

/** A native provider is an explicit invocation, never an ordinary unit/CI fallback.
 * Missing prerequisites fail before spawning Cargo, and scanner-only providers
 * do not inherit the isolated database administrator or ambient PG connection.
 */
export function cargoChildEnvironment(suite: Suite, environment: Record<string,string|undefined>) {
  let selected={...environment};
  if(suite.lane==='provider'){
    if(!suite.provider?.required.length)throw new Error(`CARGO_PROVIDER_PREREQUISITES_REQUIRED:${suite.id}`);
    const tools=new Set(['PATH','HOME','TMPDIR','TMP','TEMP','LANG','LC_ALL','LC_CTYPE','TZ','SYSTEMROOT','SystemRoot','WINDIR','USERPROFILE','XDG_CACHE_HOME','CARGO_HOME','RUSTUP_HOME','RUSTUP_TOOLCHAIN','RUSTC','RUSTDOC','CARGO_TARGET_DIR','CARGO_BUILD_TARGET','ERPX_CARGO_BIN','ERPX_NODE_BIN','ERPX_BUN_BIN']);
    selected=Object.fromEntries(Object.entries(environment).filter(([name])=>tools.has(name)));
    Object.assign(selected,qualifiedNativeLinkEnvironment(environment));
    for(const variable of suite.provider.required)selected[variable]=environment[variable];
    Object.assign(selected,suite.provider.environment);
    Object.assign(selected,qualifiedNativeLinkEnvironment(selected));
    for(const variable of suite.provider.required)if(!selected[variable]?.trim())throw new Error(`PROVIDER_PREREQUISITE_REQUIRED:${suite.id}:${variable}`);
    selected.ERPX_SELECTED_PROVIDER='1';
  }
  const explicitLeasedProvider=suite.lane==='provider'&&suite.provider?.required.includes('ERPX_TEST_ADMIN_URL')===true;
  return vitestChildEnvironment(selected,suite.lane==='integration'||explicitLeasedProvider);
}

/** Cargo manifests bind every source in a crate and local workspace dependencies.
 * These are verification edges, not executable JavaScript import edges.
 */
export function cargoVerificationEdges(files: ReadonlyMap<string, string>): Map<string, Set<string>> {
  const result = new Map<string, Set<string>>();
  const rootManifest = files.get('Cargo.toml') ?? '';
  const workspacePaths = new Map([...rootManifest.matchAll(/^([A-Za-z0-9_-]+)\s*=\s*\{[^\n}]*\bpath\s*=\s*"([^"\n]+)"[^\n}]*\}\s*$/gm)].map(match => [match[1], posix.normalize(`${match[2]}/Cargo.toml`)]));
  const declaredResources=cargoResourceRoots(files);
  const add = (source: string, dependency: string) => {
    if (!files.has(dependency)) return;
    const values = result.get(source) ?? new Set<string>(); values.add(dependency); result.set(source, values);
  };
  for (const [manifest, text] of files) {
    if (manifest === 'Cargo.toml' || !manifest.endsWith('/Cargo.toml') || !cargoPackageName(text)) continue;
    const directory = posix.dirname(manifest);
    for (const input of cargoRootInputs) add(manifest, input);
    for (const resource of declaredResources.get(manifest)??[]) {
      const members = [...files.keys()].filter(file => file === resource || file.startsWith(`${resource}/`));
      if (!members.length) throw new Error(`CARGO_RESOURCE_ROOT_UNMAPPED:${manifest}:${resource}`);
      for (const file of members) add(manifest, file);
    }

    for (const file of files.keys()) if (file.startsWith(`${directory}/src/`) && file.endsWith('.rs')) { add(manifest, file); add(file, manifest); }
    for (const target of cargoTestTargets(text)) {
      const path = posix.normalize(posix.join(directory, target.path)); add(path, manifest);
    }
    for (const match of text.matchAll(/^([A-Za-z0-9_-]+)\.workspace\s*=\s*true\s*$/gm)) {
      const dependency = workspacePaths.get(match[1]); if (dependency) add(manifest, dependency);
    }
    for (const match of text.matchAll(/\bpath\s*=\s*"([^"\n]+)"/g)) add(manifest, posix.normalize(posix.join(directory, match[1], 'Cargo.toml')));
  }
  return result;
}

/** Check worker-only and contract-generation code without forcing packaging choices such as vendored OpenSSL. */
export function cargoQualityFeatureArguments(files: ReadonlyMap<string,string>, suites: readonly Suite[]): string[] {
  const features = new Set(suites.filter(suite => suite.framework === 'cargo').flatMap(suite => (suite.cargo?.features ?? []).map(feature => `${suite.cargo!.package}/${feature}`)));
  for (const [file, source] of files) {
    if (!file.endsWith('/Cargo.toml')) continue;
    const name = cargoPackageName(source);
    const section = /^\[features\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(source)?.[1];
    if (name && section && /^contracts\s*=/m.test(section)) features.add(`${name}/contracts`);
  }
  return features.size ? ['--features', [...features].sort().join(',')] : [];
}
