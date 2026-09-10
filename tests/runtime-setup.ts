import { beforeAll,afterAll } from 'vitest';
import { setupDatabases,cleanupDatabases } from './runtime-env';
beforeAll(setupDatabases);
afterAll(cleanupDatabases);
