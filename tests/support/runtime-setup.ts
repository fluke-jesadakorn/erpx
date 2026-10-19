import { beforeAll,afterAll } from 'vitest';
import { setupDatabases,cleanupDatabases } from "./runtime-env";
beforeAll(setupDatabases);
afterAll(async()=>{try{const {closeRuntime}=await import("../../src/server/modules/runtime/adapters/runtime/env");await closeRuntime();}finally{await cleanupDatabases();}});
