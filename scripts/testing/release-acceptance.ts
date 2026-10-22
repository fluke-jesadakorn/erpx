import { execFileSync } from 'node:child_process';
import { existsSync,readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { repositoryRoot as root } from './catalogue';
import { appDefinitions } from '@erpx/contracts/apps';
// This entry is deliberately separate from dev/CI selection and is never invoked there.
const args=process.argv.slice(2),candidate=args.find(arg=>arg.startsWith('--candidate='))?.slice(12);
if(!candidate||args.length!==1||candidate.startsWith('-'))throw new Error('EXPLICIT_ACCEPTANCE_CANDIDATE_REQUIRED');
const sha=execFileSync('git',['rev-parse','--verify',`${candidate}^{commit}`],{cwd:root,encoding:'utf8'}).trim();
if(sha!==execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim()||execFileSync('git',['status','--porcelain','--untracked-files=normal'],{cwd:root,encoding:'utf8'}).trim())throw new Error('ACCEPTANCE_REQUIRES_CLEAN_FROZEN_CANDIDATE');
const path=resolve(root,'.runtime/workspace-releases',sha,'release.json');
if(!existsSync(path))throw new Error('COMBINED_CANDIDATE_RELEASE_REQUIRED:build and review all app, gateway and jobs artifacts first');
const manifest=JSON.parse(readFileSync(path,'utf8'));
if(manifest.schema!=='erpx-workspace-release/v2'||manifest.candidate!==sha||manifest.acceptance!==false||appDefinitions.some(app=>!manifest.apps?.[app.id])||!manifest.gateway||!manifest.jobs)throw new Error('EXACT_COMBINED_RELEASE_REQUIRED');
// Keep old full receipts and their single-artifact scope immutable. A v2
// production acceptance adapter must consume this complete candidate manifest.
if(manifest.acceptanceAdapter!=='workspace-v2')throw new Error('WORKSPACE_V2_ACCEPTANCE_ADAPTER_REQUIRED');
throw new Error('WORKSPACE_V2_ACCEPTANCE_ADAPTER_REQUIRED:full acceptance remains deferred until the reviewed release adapter is available');
