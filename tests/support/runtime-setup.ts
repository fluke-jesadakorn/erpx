import { beforeAll,afterAll } from 'vitest';
import { setupDatabases,cleanupDatabases } from "./runtime-env";
import { closeRuntime } from "../../src/server/modules/runtime/adapters/runtime/env";
beforeAll(setupDatabases);
afterAll(async()=>{try{await closeRuntime();}finally{await cleanupDatabases();}});
