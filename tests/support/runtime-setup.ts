import { beforeAll,afterAll,vi } from 'vitest';
import { setupDatabases,cleanupDatabases } from "./runtime-env";
beforeAll(setupDatabases);
afterAll(async()=>{try{const {closeRuntime}=await vi.importActual<typeof import("../../src/server/modules/runtime/adapters/runtime/env")>("../../src/server/modules/runtime/adapters/runtime/env");await closeRuntime();}finally{await cleanupDatabases();}});
