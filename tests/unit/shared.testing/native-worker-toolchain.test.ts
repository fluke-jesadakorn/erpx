import { afterEach, describe, expect, it } from 'vitest';
import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, realpathSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { inspectNativeWorkerPrerequisites, nativeBuildEnvironment, nativeFileObservation, nativeWorkerArguments, nativeWorkerMode, runNativeWorker } from '../../../scripts/testing/native-worker-toolchain';
import { nativeProvisionPrefix, verifyNativeArtifact } from '../../../scripts/testing/provision-native-worker';
import sources from '../../../fixtures/files/native-toolchain/sources.json';
import { nativePrerequisitePlan, nativeWorkerPlanArguments } from '../../../scripts/testing/native-worker-plan';
import { requireCleanScopedCandidate, scopedCargoQualityCommand } from '../../../scripts/testing/scoped';
import { cargoChildEnvironment } from '../../../scripts/testing/cargo';
import type { Suite } from '../../../scripts/testing/catalogue';
import { pinnedBunBinary } from '../../../scripts/testing/runtime-tooling';
import { nativeElfRunpath, nativeWorkerLinkFlags, qualifiedNativeLinkEnvironment } from '../../../scripts/testing/native-worker-link-flags';
import { qualifiedCargoBuildProfile } from '../../../scripts/testing/cargo-build-profile';

const compactProfile={ERPX_CI_CARGO_PROFILE:'compact-v1',CARGO_INCREMENTAL:'0',CARGO_PROFILE_DEV_DEBUG:'0',CARGO_PROFILE_TEST_DEBUG:'0'};

const temporary: string[] = [];
afterEach(() => { for (const path of temporary.splice(0)) rmSync(path, { recursive: true, force: true }); });
function directory() { const path=mkdtempSync(join(tmpdir(),'erpx-native-toolchain-'));temporary.push(path);return path; }
function fixture() {
 const root=directory(),prefix=join(root,'prefix');for(const part of ['lib/pkgconfig','include/vips'])mkdirSync(join(prefix,part),{recursive:true});
 const pc=join(prefix,'lib/pkgconfig/vips.pc');writeFileSync(pc,'fixture development metadata\n');writeFileSync(join(prefix,'include/vips/version.h'),'#define VIPS_VERSION "8.18.7"\n');
 const library=join(prefix,'lib',process.platform==='darwin'?'libvips.42.dylib':'libvips.so.42');writeFileSync(library,'fixture library bytes; not a qualified decoder');
 const tool=(name:string,body:string)=>{const path=join(root,name);writeFileSync(path,`#!${process.execPath}\n${body}\n`);chmodSync(path,0o700);return path;};
 const host=`${process.arch==='arm64'?'aarch64':'x86_64'}-${process.platform==='darwin'?'apple-darwin':'unknown-linux-gnu'}`;
 const rustc=tool('rustc',`console.log(${JSON.stringify(`rustc 1.99.1 (fixture)\nhost: ${host}\nrelease: 1.99.1`)});`);
 const argvFile=join(root,'argv.json');
 const cargo=tool('cargo',`if(process.argv[2]==='--version')console.log('cargo 1.99.1 (fixture)');else{require('node:fs').writeFileSync(${JSON.stringify(argvFile)},JSON.stringify({argv:process.argv.slice(2),secret:process.env.STRIPE_SECRET_KEY??null,database:process.env.ERPX_TEST_ADMIN_URL??null}));process.exitCode=0;}`);
 const pkgBody=(version='8.18.7',wrongPrefix=false,missing=false)=>`const args=process.argv.slice(2).join(' ');const prefix=${JSON.stringify(prefix)};const output={'--modversion vips':${JSON.stringify(version)},'--variable=pcfiledir vips':prefix+'/lib/pkgconfig','--variable=prefix vips':${wrongPrefix?'prefix+"-foreign"':'prefix'},'--variable=libdir vips':prefix+'/lib','--variable=includedir vips':prefix+'/include','--cflags --libs vips':${missing?'null':"'-I'+prefix+'/include -L'+prefix+'/lib -lvips'"}};if(!output[args])process.exit(7);console.log(output[args]);`;
 const pkg=tool('pkg-config',pkgBody());
 const environment={...process.env,ERPX_NATIVE_VIPS_PREFIX:prefix,ERPX_NATIVE_PKG_CONFIG_BIN:pkg,ERPX_CARGO_BIN:cargo,ERPX_RUSTC_BIN:rustc,PKG_CONFIG_PATH:undefined,PKG_CONFIG_LIBDIR:undefined,PKG_CONFIG_SYSROOT_DIR:undefined,PKG_CONFIG_ALLOW_CROSS:undefined,CARGO_BUILD_TARGET:undefined,CARGO_ENCODED_RUSTFLAGS:undefined,ERPX_NATIVE_VIPS_LIBDIR:undefined};
 return {root,prefix,pc,library,tool,cargo,pkg,pkgBody,environment,argvFile};
}
describe('explicit native worker compiler lane',()=>{
 it('requires a complete fixed CI build profile while preserving absent-marker developer behavior',()=>{
  expect(qualifiedCargoBuildProfile({CARGO_INCREMENTAL:'1',CARGO_PROFILE_DEV_DEBUG:'2'})).toEqual({});
  expect(qualifiedCargoBuildProfile({...compactProfile,CARGO_REGISTRIES_PRIVATE_TOKEN:'private'})).toEqual(compactProfile);
  for(const ERPX_CI_CARGO_PROFILE of ['', 'unknown'])expect(()=>qualifiedCargoBuildProfile({...compactProfile,ERPX_CI_CARGO_PROFILE})).toThrow('CI_CARGO_PROFILE_UNKNOWN');
  for(const key of Object.keys(compactProfile).filter(key=>key!=='ERPX_CI_CARGO_PROFILE')){
   for(const value of [undefined,'1','false',''])expect(()=>qualifiedCargoBuildProfile({...compactProfile,[key]:value})).toThrow(`CI_CARGO_PROFILE_MISMATCH:${key}`);
  }
  const native=nativeBuildEnvironment({NODE_ENV:'test',...compactProfile,CARGO_PROFILE_DEV_DEBUG_ASSERTIONS:'false',CARGO_REGISTRIES_PRIVATE_TOKEN:'private'},'/opt/vips');
  expect(native).toMatchObject({...compactProfile,CARGO_BUILD_JOBS:'2'});expect(native.CARGO_PROFILE_DEV_DEBUG_ASSERTIONS).toBeUndefined();expect(native.CARGO_REGISTRIES_PRIVATE_TOKEN).toBeUndefined();
  expect(nativeBuildEnvironment({NODE_ENV:'test',CARGO_INCREMENTAL:'1',CARGO_PROFILE_DEV_DEBUG:'2'},'/opt/vips').CARGO_INCREMENTAL).toBeUndefined();
 });
 it('passes the selected profile to actual child tools and binds native before/after receipts and exports',async()=>{
  const f=fixture(),keys=Object.keys(compactProfile);
  writeFileSync(f.cargo,`#!${process.execPath}\nif(process.argv[2]==='--version')console.log('cargo 1.99.1 (fixture)');else require('node:fs').writeFileSync(${JSON.stringify(f.argvFile)},JSON.stringify(Object.fromEntries(${JSON.stringify(keys)}.map(key=>[key,process.env[key]]))));\n`);
  expect(await runNativeWorker(['check'],{...f.environment,...compactProfile},f.root)).toBe(0);
  expect(JSON.parse(readFileSync(f.argvFile,'utf8'))).toEqual(compactProfile);
  const receipt=JSON.parse(readFileSync(join(f.root,'.scratch/dev/test-suites/native-worker/check.json'),'utf8'));
  expect(receipt).toMatchObject({cargoBuildProfile:compactProfile,status:'succeeded',exitCode:0});
  const exported=join(f.root,'github-env');
  expect(await runNativeWorker(['export-env'],{...f.environment,...compactProfile,CI:'true',GITHUB_ACTIONS:'true',GITHUB_ENV:exported},f.root)).toBe(0);
  for(const[key,value]of Object.entries(compactProfile))expect(readFileSync(exported,'utf8')).toContain(`${key}=${value}\n`);
  rmSync(f.argvFile);
  await expect(runNativeWorker(['check'],{...f.environment,...compactProfile,CARGO_INCREMENTAL:'1'},f.root)).rejects.toThrow('CI_CARGO_PROFILE_MISMATCH:CARGO_INCREMENTAL');
  expect(existsSync(f.argvFile)).toBe(false);
 });
 it('opts in only the native-worker and affected verification CI jobs',()=>{
  const workflow=readFileSync('.github/workflows/nextjs.yml','utf8');
  for(const job of [workflow.split('  native-worker:')[1].split('  verification:')[0],workflow.split('  verification:')[1].split('  acceptance:')[0]]){
   const environment=job.split('    steps:')[0];
   for(const[key,value]of Object.entries(compactProfile))expect(environment).toContain(`${key}: ${key==='ERPX_CI_CARGO_PROFILE'?value:`'${value}'`}`);
  }
  expect(workflow.split('  acceptance:')[1]).not.toContain('ERPX_CI_CARGO_PROFILE');
 });
 it('requires exactly one approved mode and never forwards arbitrary Cargo flags',()=>{
  for(const mode of ['probe','check','clippy','build','export-env','inspect-linked']){expect(nativeWorkerMode([mode])).toBe(mode);expect(nativeWorkerMode(['--',mode])).toBe(mode);}
  for(const args of [[],['--'],['--','--','clippy'],['clippy','--all-features'],['check','--no-default-features'],['test'],['install'],['build','--release']])expect(()=>nativeWorkerMode(args)).toThrow('EXACT_MODE_REQUIRED');
  expect(nativeWorkerArguments('check')).toEqual(['check','--locked','-p','erpx-service','--all-targets','--features','files-scan-worker,contracts']);
  expect(nativeWorkerArguments('clippy')).toEqual(['clippy','--locked','-p','erpx-service','--all-targets','--features','files-scan-worker,contracts','--','-D','warnings']);
  expect(nativeWorkerArguments('build')).toEqual(['build','--locked','-p','erpx-server-files','--features','preview-provider','--bin','erpx-render-preview']);
 });
 it('constructs finite build-only environment without database, auth or deployment credentials',()=>{
  const env=nativeBuildEnvironment({NODE_ENV:'test',PATH:'/usr/bin',PKG_CONFIG_PATH:'/opt/deps/lib/pkgconfig',DATABASE_URL:'private',ERPX_TEST_ADMIN_URL:'private',STRIPE_SECRET_KEY:'private',CLOUDFLARE_API_TOKEN:'private',BETTER_AUTH_SECRET:'private',RUSTC_WRAPPER:'private'},'/opt/vips');
  expect(env.PKG_CONFIG_PATH).toBe('/opt/vips/lib/pkgconfig:/opt/deps/lib/pkgconfig');expect(env.CARGO_BUILD_JOBS).toBe('2');
  for(const key of ['DATABASE_URL','ERPX_TEST_ADMIN_URL','STRIPE_SECRET_KEY','CLOUDFLARE_API_TOKEN','BETTER_AUTH_SECRET','RUSTC_WRAPPER'])expect(env[key]).toBeUndefined();
  for(const PKG_CONFIG_PATH of ['relative','/ok:','/ok::/other','/ok\n/other'])expect(()=>nativeBuildEnvironment({NODE_ENV:'test',PKG_CONFIG_PATH},'/opt/vips')).toThrow('ABSOLUTE_PKG_CONFIG_PATH_REQUIRED');
  expect(()=>nativeBuildEnvironment({NODE_ENV:'test',PKG_CONFIG_LIBDIR:'/foreign'},'/opt/vips')).toThrow('CROSS_OR_OVERRIDE');
 });
 it('observes real fixture files through child tools without claiming archive or codec qualification',()=>{
  const f=fixture(),observed=inspectNativeWorkerPrerequisites(f.environment,f.root);
  expect(observed.observation.vipsVersion).toBe('8.18.7');expect(observed.observation.files).toHaveLength(3);expect(observed.observation.sourceArchiveVerifiedByThisProbe).toBe(false);expect(observed.observation.runtimeCodecQualified).toBe(false);expect(observed.childEnvironment.RUSTC).toBe(f.environment.ERPX_RUSTC_BIN);
 });
 it('refuses missing prefix, mismatched metadata/header/target and missing dependency closure',()=>{
  expect(()=>inspectNativeWorkerPrerequisites({NODE_ENV:'test'})).toThrow('VIPS_PREFIX_REQUIRED');
  const f=fixture();
  writeFileSync(f.pkg,`#!${process.execPath}\n${f.pkgBody('8.18.8')}\n`);expect(()=>inspectNativeWorkerPrerequisites(f.environment,f.root)).toThrow('EXACT_VIPS_VERSION_REQUIRED');
  writeFileSync(f.pkg,`#!${process.execPath}\n${f.pkgBody('8.18.7',false,true)}\n`);expect(()=>inspectNativeWorkerPrerequisites(f.environment,f.root)).toThrow('TOOL_PROBE_FAILED');
  writeFileSync(f.pkg,`#!${process.execPath}\n${f.pkgBody()}\n`);writeFileSync(join(f.prefix,'include/vips/version.h'),'#define VIPS_VERSION "8.15.0"\n');expect(()=>inspectNativeWorkerPrerequisites(f.environment,f.root)).toThrow('HEADER_VERSION_MISMATCH');
  expect(()=>inspectNativeWorkerPrerequisites({...f.environment,CARGO_BUILD_TARGET:'foreign-target'},f.root)).toThrow('EXACT_NATIVE_RUST_TARGET_REQUIRED');
 });
 it('refuses a library symlink that resolves outside the selected prefix',()=>{
  const f=fixture(),outside=join(f.root,'foreign-library');writeFileSync(outside,'foreign library');rmSync(f.library);symlinkSync(outside,f.library);
  expect(()=>inspectNativeWorkerPrerequisites(f.environment,f.root)).toThrow('LIBRARY_OUTSIDE_PREFIX');
 });
 it('enforces actual file and pinned download byte/hash bounds',()=>{
  const root=directory(),file=join(root,'bounded');writeFileSync(file,'12345');expect(nativeFileObservation(file,5).bytes).toBe(5);expect(()=>nativeFileObservation(file,4)).toThrow('FILE_LIMIT');expect(()=>nativeFileObservation(root,100)).toThrow('FILE_LIMIT');
  const bytes=Buffer.from('exact artifact'),artifact={file:'owned.tar.xz',url:'https://github.com/example',bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')};expect(()=>verifyNativeArtifact(bytes,artifact)).not.toThrow();expect(()=>verifyNativeArtifact(Buffer.from('wrong artifact'),artifact)).toThrow('SOURCE_HASH_MISMATCH');expect(()=>verifyNativeArtifact(bytes,{...artifact,bytes:bytes.length+1})).toThrow('SOURCE_HASH_MISMATCH');
 });
 it('executes exact real child argv and propagates failure without exposing ambient runtime secrets',async()=>{
  const f=fixture();expect(await runNativeWorker(['check'],{...f.environment,STRIPE_SECRET_KEY:'must-not-reach-build',ERPX_TEST_ADMIN_URL:'must-not-reach-build'},f.root)).toBe(0);
  expect(JSON.parse(readFileSync(f.argvFile,'utf8'))).toEqual({argv:['check','--locked','-p','erpx-service','--all-targets','--features','files-scan-worker,contracts'],secret:null,database:null});
  writeFileSync(f.cargo,`#!${process.execPath}\nif(process.argv[2]==='--version')console.log('cargo 1.99.1 (fixture)');else process.exitCode=9;\n`);
  expect(await runNativeWorker(['clippy'],f.environment,f.root)).toBe(9);expect(JSON.parse(readFileSync(join(f.root,'.scratch/dev/test-suites/native-worker/clippy.json'),'utf8')).status).toBe('failed');
 });
 it('rejects changed prerequisites even when the actual Cargo child exits successfully',async()=>{
  const f=fixture();writeFileSync(f.cargo,`#!${process.execPath}\nif(process.argv[2]==='--version')console.log('cargo 1.99.1 (fixture)');else require('node:fs').appendFileSync(${JSON.stringify(f.pc)},'# changed during compile\\n');\n`);
  await expect(runNativeWorker(['build'],f.environment,f.root)).rejects.toThrow('PREREQUISITES_CHANGED');
 });
 it('requires explicit fresh provisioning destination and pins both Linux architecture artifacts',()=>{
  expect(nativeProvisionPrefix(['--prefix=/tmp/owned-build'])).toBe('/tmp/owned-build');
  for(const args of [[],['--prefix=relative'],['--prefix=/'],['--prefix=/tmp/a','--force'],['--','--','--prefix=/tmp/a']])expect(()=>nativeProvisionPrefix(args)).toThrow();
  expect(sources.vips.sha256).toBe('5baaead3b0bb20ffdb9e9ff09aa9fda08620923df77b63b436654cb5e0b3bf94');expect(sources.buildJobs).toBe(2);expect(sources.mesonOptions).toContain('--auto-features=disabled');expect(sources.ninja.x64.file).toContain('x86_64');expect(sources.ninja.arm64.file).toContain('aarch64');expect(sources.ninja.x64.sha256).not.toBe(sources.ninja.arm64.sha256);
  const job=readFileSync('.github/workflows/nextjs.yml','utf8').split('  native-worker:')[1].split('  verification:')[0];
  // GitHub permits runner context at step scope, not jobs.<id>.env.
  expect(job.split('    steps:')[0]).not.toContain('runner.');expect(job).toContain('"$RUNNER_TEMP" >> "$GITHUB_ENV"');
  for(const mode of ['check','clippy','build'])expect(job).toContain(`run: bun scripts/testing/native-worker-toolchain.ts ${mode}`);
  expect(job).not.toContain('services:');expect(job).not.toContain('secrets.');expect(job).not.toContain('release:acceptance');

 });
 it('actual pinned-Bun CLI rejects unknown modes before any missing prerequisite or installation',()=>{
  const bun=pinnedBunBinary(resolve('.')); expect(execFileSync(bun,['--version'],{encoding:'utf8'}).trim()).toBe('1.4.3');
  // Bun consumes its own first launcher separator before program argv.
  for(const args of [['install'],['--','--','--','clippy']]){
   const cwd=directory(),result=spawnSync(bun,[resolve('scripts/testing/native-worker-toolchain.ts'),...args],{cwd,env:{NODE_ENV:'test',PATH:process.env.PATH},encoding:'utf8'});
   expect(result.status).toBe(1);expect(result.stderr).toContain('NATIVE_WORKER_EXACT_MODE_REQUIRED');expect(existsSync(join(cwd,'.scratch'))).toBe(false);
  }
 });
});


it('plans from the exact actual global Cargo feature union, including an unselected provider',()=>{
 const suite=(packageName:string,features:string[]=[],lane:Suite['lane']='unit'):Suite=>({id:packageName,lane,owner:'shared.testing',modules:['finance'],files:['tests/unit/shared.testing/owned.test.rs'],fixture:'none',requires:{jobs:false,scanner:false},framework:'cargo',cargo:{package:packageName,targets:['owned'],features}});
 const core=suite('erpx-server-shared'),worker=suite('erpx-service',['files-scan-worker'],'provider'),files=new Map<string,string>();
 const planned=nativePrerequisitePlan([core],[],files,[core,worker]);
 expect(planned.required).toBe(true);expect(planned.requiringFeatures).toEqual(['erpx-service/files-scan-worker']);
 expect(planned.qualityCommand).toEqual(['cargo','clippy','--workspace','--all-targets','--features','erpx-service/files-scan-worker','--locked','--','-D','warnings']);
 expect(planned.qualityCommand).toEqual(scopedCargoQualityCommand([core],[],files,[core,worker]));
 expect(nativePrerequisitePlan([],['docs/README.md'],files,[core,worker]).required).toBe(false);
 expect(nativePrerequisitePlan([],['Cargo.lock'],files,[core,worker]).required).toBe(true);
 expect(nativePrerequisitePlan([core],[],files,[core,suite('erpx-server-legal',['document-provider'])]).required).toBe(false);
 expect(nativePrerequisitePlan([core],[],files,[core,suite('erpx-server-files',['preview-provider'],'provider')]).required).toBe(true);
});
it('persists the real selected compiler and provider environment before affected verification',()=>{
 const workflow=readFileSync('.github/workflows/nextjs.yml','utf8'),job=workflow.split('  verification:')[1].split('  acceptance:')[0];
 const stages=['Resolve the exact event candidate','Plan actual selected native compiler prerequisites','Install selected native development packages','Select actual affected-job native paths','Provision the selected hash-pinned native prefix','Persist selected compiler link configuration for quality and provider rebuilds','Preflight and link the selected actual preview executable','Inspect selected actual ELF RUNPATH and isolated startup','Run affected checks and development smoke'];
 for(let index=1;index<stages.length;index++)expect(job.indexOf(stages[index])).toBeGreaterThan(job.indexOf(stages[index-1]));
 expect(job.match(/if: steps.selected-native.outputs.required == 'true'/g)).toHaveLength(6);
 for(const variable of ['ERPX_NATIVE_VIPS_PREFIX','PKG_CONFIG_PATH','ERPX_FILES_PREVIEW_BIN','CARGO_TARGET_DIR'])expect(job).toContain(`printf '${variable}=`);
 expect(job).toContain('native-worker-plan.ts --base="$ERPX_CI_BASE" --head="$ERPX_CI_HEAD"');
 const provider:Suite={id:'files.selected.provider',lane:'provider',owner:'files',modules:['finance'],files:[],fixture:'provider',requires:{jobs:false,scanner:false},provider:{environment:{},required:['PKG_CONFIG_PATH','ERPX_FILES_PREVIEW_BIN']}};
 const env=cargoChildEnvironment(provider,{NODE_ENV:'test',PATH:'/usr/bin',PKG_CONFIG_PATH:'/qualified/lib/pkgconfig',ERPX_FILES_PREVIEW_BIN:'/owned/target/debug/erpx-render-preview',STRIPE_SECRET_KEY:'must-not-reach-child',ERPX_TEST_ADMIN_URL:'must-not-reach-child'});
 expect(env.PKG_CONFIG_PATH).toBe('/qualified/lib/pkgconfig');expect(env.ERPX_FILES_PREVIEW_BIN).toBe('/owned/target/debug/erpx-render-preview');expect(env.STRIPE_SECRET_KEY).toBeUndefined();expect(env.ERPX_TEST_ADMIN_URL).toBeUndefined();
});
it('requires exact clean checked-out revisions and does not execute a plan merely by import',()=>{
 const root=directory(),git=(...args:string[])=>execFileSync('git',args,{cwd:root,encoding:'utf8'}).trim();
 git('init','--quiet');writeFileSync(join(root,'owned.txt'),'original');git('add','.');git('-c','user.name=OwnedFixture','-c','user.email=fixture@example.invalid','commit','--quiet','-m','owned fixture');const first=git('rev-parse','HEAD');
 expect(requireCleanScopedCandidate(first,root)).toBe(first);writeFileSync(join(root,'owned.txt'),'changed');expect(()=>requireCleanScopedCandidate(first,root)).toThrow('CLEAN_CHECKED_OUT_CANDIDATE');
 git('add','.');git('-c','user.name=OwnedFixture','-c','user.email=fixture@example.invalid','commit','--quiet','-m','second');expect(()=>requireCleanScopedCandidate(first,root)).toThrow('CLEAN_CHECKED_OUT_CANDIDATE');expect(requireCleanScopedCandidate('HEAD',root)).toBe(git('rev-parse','HEAD'));
 for(const args of [[],['--base=HEAD'],['--base=HEAD','--head=HEAD','--base=other'],['--base=HEAD','--head=HEAD','--skip-native']])expect(()=>nativeWorkerPlanArguments(args)).toThrow();
 expect(nativeWorkerPlanArguments(['--base=base','--head=head'])).toEqual({base:'base',head:'head'});
 const bun=pinnedBunBinary(resolve('.'));
 const result=spawnSync(bun,['--eval',`await import(${JSON.stringify(resolve('scripts/testing/native-worker-plan.ts'))});`],{cwd:root,env:{NODE_ENV:'test',PATH:process.env.PATH},encoding:'utf8'});
 expect(result.status).toBe(0);expect(result.stdout).toBe('');expect(existsSync(join(root,'.scratch'))).toBe(false);
});

it('encodes the exact Linux RUNPATH without shell splitting spaces/commas or permitting extra flags',()=>{
 const libdir='/opt/owned libs,checked/lib',flags=nativeWorkerLinkFlags('linux',libdir);
 expect(flags.split('\u001f')).toEqual(['-C','link-arg=-Xlinker','-C','link-arg=--enable-new-dtags','-C','link-arg=-Xlinker','-C','link-arg=-rpath','-C','link-arg=-Xlinker','-C',`link-arg=${libdir}`]);
 expect(qualifiedNativeLinkEnvironment({ERPX_NATIVE_VIPS_PREFIX:'/opt/owned libs,checked',ERPX_NATIVE_VIPS_LIBDIR:libdir,CARGO_ENCODED_RUSTFLAGS:flags},'linux').CARGO_ENCODED_RUSTFLAGS).toBe(flags);
 for(const path of ['/opt/bad:split/lib','/opt/$ORIGIN/lib','/opt/bad\u001f/lib'])expect(()=>nativeWorkerLinkFlags('linux',path)).toThrow();
 expect(()=>qualifiedNativeLinkEnvironment({ERPX_NATIVE_VIPS_PREFIX:'/opt/owned',ERPX_NATIVE_VIPS_LIBDIR:'/foreign/lib',CARGO_ENCODED_RUSTFLAGS:flags},'linux')).toThrow('LINK_FLAGS_MISMATCH');
 expect(()=>qualifiedNativeLinkEnvironment({ERPX_NATIVE_VIPS_PREFIX:'/opt/owned libs,checked',ERPX_NATIVE_VIPS_LIBDIR:libdir,CARGO_ENCODED_RUSTFLAGS:flags+'\u001f-Ctarget-cpu=native'},'linux')).toThrow('LINK_FLAGS_MISMATCH');
 const nativeFlags=nativeWorkerLinkFlags(process.platform,libdir);
 const provider:Suite={id:'files.link.provider',lane:'provider',owner:'files',modules:['finance'],files:[],fixture:'provider',requires:{jobs:false,scanner:false},provider:{environment:{},required:['PKG_CONFIG_PATH']}};
 const selected=cargoChildEnvironment(provider,{NODE_ENV:'test',PKG_CONFIG_PATH:'/opt/owned libs,checked/lib/pkgconfig',ERPX_NATIVE_VIPS_PREFIX:'/opt/owned libs,checked',ERPX_NATIVE_VIPS_LIBDIR:libdir,CARGO_ENCODED_RUSTFLAGS:nativeFlags,LD_LIBRARY_PATH:'/unreviewed',DYLD_LIBRARY_PATH:'/unreviewed'});
 expect(selected.CARGO_ENCODED_RUSTFLAGS).toBe(nativeFlags);expect(selected.ERPX_NATIVE_VIPS_LIBDIR).toBe(libdir);expect(selected.LD_LIBRARY_PATH).toBeUndefined();expect(selected.DYLD_LIBRARY_PATH).toBeUndefined();
});
it('exports only verified compiler configuration and requires real ELF RUNPATH plus direct vips linkage',async()=>{
 const f=fixture(),envfile=join(f.root,'github-env');
 expect(await runNativeWorker(['export-env'],{...f.environment,CI:'true',GITHUB_ACTIONS:'true',GITHUB_ENV:envfile},f.root)).toBe(0);
 const exported=readFileSync(envfile,'utf8'),canonical=realpathSync(f.prefix);expect(exported).toContain(`ERPX_NATIVE_VIPS_PREFIX=${canonical}\n`);expect(exported).toContain(`ERPX_NATIVE_VIPS_LIBDIR=${canonical}/lib\n`);expect(exported).toContain(`CARGO_ENCODED_RUSTFLAGS=${nativeWorkerLinkFlags(process.platform,join(canonical,'lib'))}\n`);expect(exported).not.toContain('LD_LIBRARY_PATH');expect(existsSync(f.argvFile)).toBe(false);
 // Node on macOS initializes __CF_USER_TEXT_ENCODING itself. Prove the
 // exact runtime-only metadata against an independent direct POSIX spawn;
 // never accept arbitrary keys from the Bun parent or broaden the Rust env.
 const keys=f.tool('minimal-keys',`process.stdout.write(JSON.stringify(Object.keys(process.env).sort()));`);
 const control=spawnSync(process.execPath,[keys],{env:{NODE_ENV:'test',LANG:'C',TZ:'UTC'},encoding:'utf8'});expect(control.status).toBe(0);
 const metadata=JSON.parse(control.stdout) as string[];
 expect(metadata).toEqual(['LANG','NODE_ENV','TZ',...(process.platform==='darwin'?['__CF_USER_TEXT_ENCODING']:[])].sort());
 const expectedKeys=metadata.filter(key=>key!=='NODE_ENV');
 const fake=f.tool('minimal-startup',`if(JSON.stringify(Object.keys(process.env).sort())!==JSON.stringify(${JSON.stringify(expectedKeys)}))process.exit(3);process.exit(1);`);
 const bun=pinnedBunBinary(resolve('.'));
 const result=spawnSync(bun,['--eval',`const {readOnlyNativeStartup}=await import(${JSON.stringify(resolve('scripts/testing/native-worker-toolchain.ts'))});const r=readOnlyNativeStartup(${JSON.stringify(fake)},${JSON.stringify(f.root)});process.stdout.write(JSON.stringify({exitCode:r.exitCode,signal:r.signalCode??null,out:r.stdout.length,err:r.stderr.length}));`],{env:{NODE_ENV:'test',PATH:process.env.PATH},encoding:'utf8'});
 expect(result.status).toBe(0);expect(JSON.parse(result.stdout)).toEqual({exitCode:1,signal:null,out:0,err:0});
 const dynamic='0x1 (NEEDED) Shared library: [libvips.so.42]\n0x1d (RUNPATH) Library runpath: [/opt/owned libs,checked/lib]';
 expect(nativeElfRunpath(dynamic,'/opt/owned libs,checked/lib')).toBe('/opt/owned libs,checked/lib');
 for(const output of [dynamic.replace('RUNPATH','RPATH'),dynamic.replace('/opt/owned libs,checked/lib','/wrong/lib'),dynamic.replace('libvips.so.42','libc.so.6')])expect(()=>nativeElfRunpath(output,'/opt/owned libs,checked/lib')).toThrow('EXACT_ELF_RUNPATH_REQUIRED');
 const workflow=readFileSync('.github/workflows/nextjs.yml','utf8');expect(workflow.match(/run: bun scripts\/testing\/native-worker-toolchain.ts export-env/g)).toHaveLength(2);expect(workflow.match(/run: bun scripts\/testing\/native-worker-toolchain.ts inspect-linked/g)).toHaveLength(2);
});
