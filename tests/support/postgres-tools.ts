import { execFile } from 'node:child_process';
import { isAbsolute, join } from 'node:path';
import { promisify } from 'node:util';

const execute = promisify(execFile);

/** Explicit CI tool directory, or the operator's PATH; never infer a macOS install. */
export async function postgresBackupTools(environment: NodeJS.ProcessEnv = process.env) {
  const directory = environment.ERPX_TEST_PG_BIN?.trim();
  if (directory && !isAbsolute(directory)) throw new Error('ABSOLUTE_TEST_POSTGRES_TOOL_DIRECTORY_REQUIRED');
  const tool = async (name: 'pg_dump' | 'pg_restore') => {
    const binary = directory ? join(directory, name) : name;
    const { stdout } = await execute(binary, ['--version'], { timeout: 5_000, maxBuffer: 4_096 });
    if (!new RegExp(`^${name} \\(PostgreSQL\\) 18\\.[0-9]+(?:\\s|$)`).test(stdout))
      throw new Error('POSTGRES_18_BACKUP_TOOL_REQUIRED');
    return binary;
  };
  const [pgDump, pgRestore] = await Promise.all([tool('pg_dump'), tool('pg_restore')]);
  return { pgDump, pgRestore };
}
