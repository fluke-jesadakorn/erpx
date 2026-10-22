import { beforeAll,afterAll,vi,inject,expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { setupDatabases,cleanupDatabases,configureDatabaseFixture } from "./runtime-env";
declare module 'vitest' { interface ProvidedContext { erpxFixtures: string } }
const manifest=inject('erpxFixtures'),file=expect.getState().testPath;
if(!manifest||!file)throw new Error('NATIVE_FIXTURE_CONTEXT_REQUIRED');
configureDatabaseFixture(JSON.parse(readFileSync(manifest,'utf8')),file);
beforeAll(setupDatabases);
afterAll(async()=>{try{const {closeRuntime}=await vi.importActual<typeof import("@erpx/server-runtime/adapters/runtime/env")>("@erpx/server-runtime/adapters/runtime/env");await closeRuntime();}finally{await cleanupDatabases();}});
