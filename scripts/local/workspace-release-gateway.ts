import { readFile } from 'node:fs/promises';
import { isAbsolute } from 'node:path';
import { verifyWorkspaceRelease } from './workspace-release-manifest';
import { startCompiledWorkspaceGateway } from './compiled-workspace-gateway';

const args=process.argv.slice(2),release=args.find(arg=>arg.startsWith('--release='))?.slice(10),routing=args.find(arg=>arg.startsWith('--routing='))?.slice(10),rawPort=args.find(arg=>arg.startsWith('--port='))?.slice(7);
if(args.length!==3||!release||!isAbsolute(release)||!routing||!isAbsolute(routing)||!rawPort||!/^\d+$/.test(rawPort))throw new Error('EXPLICIT_RELEASE_ROUTING_AND_PORT_REQUIRED');
if(process.version!=='v26.11.1')throw new Error('PINNED_RELEASE_RUNTIME_REQUIRED');
await verifyWorkspaceRelease(release);
const gateway=await startCompiledWorkspaceGateway(JSON.parse(await readFile(routing,'utf8')),Number(rawPort));
for(const signal of ['SIGINT','SIGTERM']as const)process.once(signal,()=>{void gateway.close().then(()=>process.exit(0));});
