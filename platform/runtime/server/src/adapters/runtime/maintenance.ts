import { access } from "node:fs/promises";
import { join } from "node:path";

export async function maintenance(dataDir: string) {
  try { await access(join(dataDir,'maintenance')); return true; }
  catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false; throw error; }
}
