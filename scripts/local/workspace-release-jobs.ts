import { isAbsolute, join } from 'node:path';
import { verifyWorkspaceRelease } from './workspace-release-manifest';

const args=process.argv.slice(2),release=args.find(arg=>arg.startsWith('--release='))?.slice(10);
if(args.length!==1||!release||!isAbsolute(release))throw new Error('EXPLICIT_JOBS_RELEASE_REQUIRED');
if(process.version!=='v26.11.1')throw new Error('PINNED_RELEASE_RUNTIME_REQUIRED');
const manifest=await verifyWorkspaceRelease(release);
process.env.ERPX_RESOURCE_ROOT=join(release,manifest.jobs.path);
process.env.ERPX_REPOSITORY_ROOT=process.env.ERPX_RESOURCE_ROOT;
process.env.ERPX_PROCESS_ROLE='jobs';
// Import after verification and explicit resource composition; no duplicate app schedulers.
await import('./jobs');
