import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { cargoCaseResults, cargoChildEnvironment, cargoQualityFeatureArguments, cargoSuiteArguments, cargoTestTargets, validateCargoSuite } from '../../../scripts/testing/cargo';
import { parseTestArguments } from '../../../scripts/testing/arguments';
import type { Suite } from '../../../scripts/testing/catalogue';
import { createWorkspaceGraph, dependencyClosure, selectCandidateSuites, selectWorkspaceSuites, workspaceFingerprint } from '../../../scripts/testing/workspace-graph';
import { runtimeOwnerCycles } from '../../../scripts/architecture/owner-cycles';

const shared = 'packages/server-shared/Cargo.toml', runtime = 'packages/server-runtime/Cargo.toml';
const sharedTest = 'tests/unit/shared.architecture/rust-shared-core.test.rs', runtimeTest = 'tests/unit/shared.architecture/rust-runtime-foundation.test.rs';
function nativeSources() {
  return new Map([
    ['Cargo.toml', '[workspace]\nmembers=["packages/server-shared","packages/server-runtime"]\n[workspace.dependencies]\nerpx-server-shared = { path = "packages/server-shared" }\n'],
    ['Cargo.lock', 'version=4\n'], ['rust-toolchain.toml', '[toolchain]\nchannel="1.99.1"\n'],
    [shared, '[package]\nname="erpx-server-shared"\n[[test]]\nname="shared_core"\npath="../../tests/unit/shared.architecture/rust-shared-core.test.rs"\n'],
    [runtime, '[package]\nname="erpx-server-runtime"\n[dependencies]\nerpx-server-shared.workspace = true\n[[test]]\nname="runtime_foundation"\npath="../../tests/unit/shared.architecture/rust-runtime-foundation.test.rs"\n'],
    ['packages/server-shared/src/lib.rs', 'pub mod public;'], ['packages/server-shared/src/domain/money.rs', 'pub struct Amount(i128);'],
    ['packages/server-runtime/src/lib.rs', 'pub mod public;'],
    [sharedTest, '#[test]\nfn exact_money() {}\n'], [runtimeTest, '#[tokio::test]\nasync fn readiness() {}\n'],
  ]);
}
function nativeSuite(packageName = 'erpx-server-shared', target = 'shared_core', file = sharedTest, manifest = shared): Suite {
  return { id: `shared.${target}.rust.unit`, lane: 'unit', owner: 'shared.architecture', modules: ['finance'], files: [file], framework: 'cargo', cargo: { package: packageName, targets: [target] }, verificationInputs: [manifest, 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'], fixture: 'none', requires: { jobs: false, scanner: false }, apps: ['finance'], packages: ['@erpx/server-shared'] };
}
const temporary: string[] = [];
afterEach(() => { for (const directory of temporary.splice(0)) rmSync(directory, { recursive: true, force: true }); });
function fixtureRoot() {
  const root = mkdtempSync(join(tmpdir(), 'erpx-cargo-catalogue-')); temporary.push(root);
  for (const [file, contents] of nativeSources()) { mkdirSync(dirname(join(root, file)), { recursive: true }); writeFileSync(join(root, file), contents); }
  return root;
}

describe('Rust catalogue and source selection', () => {
  it('requires explicit native provider prerequisites and keeps them outside automatic candidate selection',()=>{
    const root=fixtureRoot(),provider:Suite={...nativeSuite(),id:'files.scanner.provider',lane:'provider',fixture:'provider',provider:{environment:{ERPX_SCANNER_TEST:'1'},required:['ERPX_CLAMD_SOCKET','ERPX_CLAMAV_DATABASE_DIR']}};
    expect(()=>validateCargoSuite(provider,root)).not.toThrow();
    expect(()=>validateCargoSuite({...provider,provider:undefined},root)).toThrow('CARGO_PROVIDER_PREREQUISITES_REQUIRED');
    expect(()=>validateCargoSuite({...provider,postgres:{mode:'raw',slots:['CONTROL_DB'],migratedSlots:[]}},root)).toThrow('CARGO_PROVIDER_PREREQUISITES_REQUIRED');
    expect(()=>cargoChildEnvironment(provider,{ERPX_CLAMD_SOCKET:'/owned/clamd.sock'})).toThrow('PROVIDER_PREREQUISITE_REQUIRED:files.scanner.provider:ERPX_CLAMAV_DATABASE_DIR');
    const ambient={ERPX_CLAMD_SOCKET:'/owned/clamd.sock',ERPX_CLAMAV_DATABASE_DIR:'/owned/definitions',ERPX_TEST_ADMIN_URL:'private-admin',PGPASSWORD:'private',ERPX_SCANNER_TEST:'ambient',PATH:'/owned/tools',HOME:'/owned/home',CARGO_HOME:'/owned/cargo',DATABASE_URL:'private-database',BETTER_AUTH_SECRET:'private-auth',STRIPE_SECRET_KEY:'private-payment',OPENAI_API_KEY:'private-ai',AI_CREDENTIAL_KEYS:'private-keyring',CARGO_REGISTRIES_EXAMPLE_TOKEN:'private-registry'};
    const selected=cargoChildEnvironment(provider,ambient);
    expect(selected).toMatchObject({ERPX_SELECTED_PROVIDER:'1',ERPX_SCANNER_TEST:'1',ERPX_CLAMD_SOCKET:'/owned/clamd.sock'});
    expect(selected.ERPX_TEST_ADMIN_URL).toBeUndefined();expect(selected.PGPASSWORD).toBeUndefined();expect(ambient.ERPX_SCANNER_TEST).toBe('ambient');
    for(const key of ['DATABASE_URL','BETTER_AUTH_SECRET','STRIPE_SECRET_KEY','OPENAI_API_KEY','AI_CREDENTIAL_KEYS','CARGO_REGISTRIES_EXAMPLE_TOKEN'])expect(selected[key],key).toBeUndefined();
    expect(selected).toMatchObject({PATH:'/owned/tools',HOME:'/owned/home',CARGO_HOME:'/owned/cargo'});
    const graph=createWorkspaceGraph(nativeSources());expect(selectCandidateSuites(graph,graph,[provider],['Cargo.lock']).suites).toEqual([]);
    expect(cargoChildEnvironment(nativeSuite(),{ERPX_TEST_ADMIN_URL:'private-admin'}).ERPX_TEST_ADMIN_URL).toBeUndefined();
  });
  it('refuses exit-zero Cargo summaries with ignored, filtered, missing or empty cases', () => {
    const green = 'running 1 test\ntest money_is_exact ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n';
    expect(cargoCaseResults(green, sharedTest)).toMatchObject({ passed: true, acceptance: false, cases: [{ runner: 'cargo', file: sharedTest, title: 'money_is_exact', status: 'passed' }] });
    for (const output of ['', green.replace('1 passed', '2 passed'), green.replace('0 ignored', '1 ignored'), green.replace('0 filtered out', '1 filtered out'), green.replace('money_is_exact ... ok', 'money_is_exact ... ignored'), 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;']) expect(cargoCaseResults(output, sharedTest).passed).toBe(false);
  });
  it('dispatches exact registered Cargo targets with locked dependencies and rejects runner-specific filters', () => {
    expect(cargoSuiteArguments(nativeSuite())).toEqual(['test', '--locked', '--package', 'erpx-server-shared', '--test', 'shared_core']);
    expect(() => cargoSuiteArguments(nativeSuite(), ['--testNamePattern=not-a-rust-filter'])).toThrow('CARGO_FORWARD_ARGUMENTS_UNSUPPORTED');
    expect(parseTestArguments([sharedTest]).files).toEqual([sharedTest]);
    expect(parseTestArguments([sharedTest]).forwarded).toEqual([]);
  });

  it('requires a real declared test target and prevents manifest-to-file substitution', () => {
    const root = fixtureRoot();
    expect(() => validateCargoSuite(nativeSuite(), root)).not.toThrow();
    expect(() => validateCargoSuite({ ...nativeSuite(), cargo: { package: 'erpx-server-shared', targets: ['missing'] } }, root)).toThrow('CARGO_TEST_TARGET_UNDECLARED');
    expect(() => validateCargoSuite({ ...nativeSuite(), files: [runtimeTest] }, root)).toThrow('CARGO_TARGET_FILE_MISMATCH');
    expect(() => validateCargoSuite({ ...nativeSuite(), verificationInputs: [shared] }, root)).toThrow('CARGO_SOURCE_INPUT_REQUIRED');
    expect(() => validateCargoSuite({ ...nativeSuite(), framework: undefined }, root)).toThrow('CARGO_CONFIG_WITHOUT_FRAMEWORK');
  });

  it('recognizes multiple explicit Cargo targets without swallowing later manifest sections', () => {
    expect(cargoTestTargets('[package]\nname="owner"\n[[test]]\nname="one"\npath="first.test.rs"\n[[test]]\nname="two"\npath="second.test.rs"\n[lints]\nworkspace=true\n')).toEqual([{ name: 'one', path: 'first.test.rs' }, { name: 'two', path: 'second.test.rs' }]);
    expect(() => cargoTestTargets('[[test]]\nname="unnamed-file"\n')).toThrow('EXPLICIT_CARGO_TEST_TARGET_REQUIRED');
  });
  it('requires registered worker features so conditional native cases cannot disappear from a passing receipt',()=>{
    const root=fixtureRoot(),sources=nativeSources();
    writeFileSync(join(root,shared),sources.get(shared)!+'required-features = ["worker"]\n[features]\nworker=[]\n');
    expect(()=>validateCargoSuite(nativeSuite(),root)).toThrow('CARGO_REQUIRED_FEATURE_MISSING');
    expect(()=>validateCargoSuite({...nativeSuite(),cargo:{...nativeSuite().cargo!,features:['worker']}},root)).not.toThrow();
    expect(cargoTestTargets('[[test]]\nname="worker"\npath="worker.test.rs"\nrequired-features = [\n "worker",\n "contract",\n]\n')).toEqual([{name:'worker',path:'worker.test.rs',requiredFeatures:['worker','contract']}]);
  });

  it('maps shared Rust source changes through local Cargo dependencies without creating JavaScript owner cycles', () => {
    const files = nativeSources(), graph = createWorkspaceGraph(files);
    const suites = [nativeSuite(), nativeSuite('erpx-server-runtime', 'runtime_foundation', runtimeTest, runtime)];
    expect(dependencyClosure(graph, [runtimeTest])).toContain('packages/server-shared/src/domain/money.rs');
    expect(selectWorkspaceSuites(graph, suites, { changed: ['packages/server-shared/src/domain/money.rs'] }).suites).toEqual(suites);
    expect(selectWorkspaceSuites(graph, suites, { changed: ['packages/server-runtime/src/lib.rs'] }).suites).toEqual([suites[1]]);
    expect(runtimeOwnerCycles(graph)).toEqual([]);
    expect(graph.runtimeImports.get('packages/server-runtime/src/lib.rs')?.size).toBe(0);
  });

  it('invalidates a Rust pure-test cache for source, lockfile and toolchain changes', () => {
    const files = nativeSources(), before = createWorkspaceGraph(files), fingerprint = workspaceFingerprint(before, [runtimeTest]);
    for (const input of ['packages/server-shared/src/domain/money.rs', 'Cargo.lock', 'rust-toolchain.toml']) {
      const changed = new Map(files); changed.set(input, changed.get(input)! + '\n# changed');
      expect(workspaceFingerprint(createWorkspaceGraph(changed), [runtimeTest]), input).not.toBe(fingerprint);
    }
  });

  it('retains the deleted Rust source edge from the base graph for candidate selection', () => {
    const files = nativeSources(), before = createWorkspaceGraph(files), changed = new Map(files);
    changed.delete('packages/server-shared/src/domain/money.rs');
    const suites = [nativeSuite(), nativeSuite('erpx-server-runtime', 'runtime_foundation', runtimeTest, runtime)];
    expect(selectCandidateSuites(createWorkspaceGraph(changed), before, suites, ['packages/server-shared/src/domain/money.rs']).suites).toEqual(suites);
  });
  it('checks selected worker and contract features without enabling vendored packaging dependencies', () => {
    const sources = nativeSources();
    sources.set('packages/server-identity_access/Cargo.toml', '[package]\nname="erpx-server-identity-access"\n[features]\ncontracts=["dep:ts-rs"]\n');
    const legal = { ...nativeSuite(), cargo: { package: 'erpx-server-legal', targets: ['pdf_validation'], features: ['pdf-verifier'] } };
    expect(cargoQualityFeatureArguments(sources, [legal])).toEqual(['--features', 'erpx-server-identity-access/contracts,erpx-server-legal/pdf-verifier']);
    expect(cargoQualityFeatureArguments(nativeSources(), [nativeSuite()])).toEqual([]);
  });
  it('tracks a separate composition binary through runtime infrastructure without an owner dependency cycle', () => {
    const sources = nativeSources(), service = 'src/composition/Cargo.toml', test = 'tests/unit/shared.architecture/rust-composition.test.rs';
    sources.set('Cargo.toml', sources.get('Cargo.toml')! + 'erpx-server-runtime = { path = "packages/server-runtime" }\n');
    sources.set(service, '[package]\nname="erpx-service"\n[dependencies]\nerpx-server-runtime.workspace = true\n[[test]]\nname="composition"\npath="../../tests/unit/shared.architecture/rust-composition.test.rs"\n');
    sources.set('src/composition/src/main.rs', 'fn main() {}'); sources.set(test, '#[test] fn owners_are_pending() {}');
    const suite = nativeSuite('erpx-service', 'composition', test, service), graph = createWorkspaceGraph(sources);
    expect(dependencyClosure(graph, [test])).toContain('packages/server-shared/src/domain/money.rs');
    expect(selectWorkspaceSuites(graph, [nativeSuite(), suite], { changed: ['src/composition/src/main.rs'] }).suites).toEqual([suite]);
    expect(runtimeOwnerCycles(graph)).toEqual([]);
  });

  it('includes added and removed migrations through declared native resource roots', () => {
    const sources = nativeSources(), migration = 'migrations/postgres/control/0001_init.sql', added = 'migrations/postgres/control/0002_next.sql';
    sources.set(runtime, sources.get(runtime)! + '[package.metadata.erpx]\nverification-roots = [\n  "../../migrations/postgres",\n]\n');
    sources.set(migration, 'CREATE TABLE initial(id int);');
    const before = createWorkspaceGraph(sources), suites = [nativeSuite(), nativeSuite('erpx-server-runtime', 'runtime_foundation', runtimeTest, runtime)];
    const candidate = new Map(sources); candidate.set(added, 'ALTER TABLE initial ADD COLUMN next int;');
    expect(selectCandidateSuites(createWorkspaceGraph(candidate), before, suites, [added]).suites).toEqual([suites[1]]);
    expect(workspaceFingerprint(createWorkspaceGraph(candidate), [runtimeTest])).not.toBe(workspaceFingerprint(before, [runtimeTest]));
    const invalid = new Map(sources); invalid.set(runtime, sources.get(runtime)!.replace('../../migrations/postgres', '../../../outside'));
    expect(() => createWorkspaceGraph(invalid)).toThrow('CARGO_RESOURCE_ROOT_INVALID');
    candidate.delete(migration);
    expect(selectCandidateSuites(createWorkspaceGraph(candidate), before, suites, [migration]).suites).toEqual([suites[1]]);
  });

});
