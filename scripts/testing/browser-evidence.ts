import { chmod, copyFile, lstat, mkdir, open, readdir, realpath } from "node:fs/promises";
import { basename, join, relative, resolve } from "node:path";

/** Route observations belong to the exact case evidence directory, outside its disposable auth files. */
export async function prepareUatCaseEvidence(root: string, run: string, output: string): Promise<string> {
  const destination = resolve(output), evidenceRoot = resolve(root, ".scratch/dev/test-suites/browser");
  const parts = relative(evidenceRoot, destination).split("/");
  if (!/^[a-f0-9]{12}$/.test(run) || parts.length !== 2 || parts.some(part => !part || part === "..") || basename(destination) !== run || await realpath(destination) !== destination)
    throw new Error("OWNED_UAT_CASE_EVIDENCE_DIRECTORY_REQUIRED");
  await chmod(destination, 0o700);
  const path = join(destination, "route-results.jsonl");
  // Exclusive creation refuses stale evidence and existing symlinks; this run
  // can append its observations without altering any previous case's evidence.
  const handle = await open(path, "wx", 0o600);
  await handle.close();
  return path;
}

/** Retain only the declared fictional Acme preparation evidence before removing its cohort. */
export async function retainBrowserFixtureEvidence(root: string, run: string, output: string): Promise<void> {
  const destination = resolve(output), evidenceRoot = resolve(root, ".scratch/dev/test-suites/browser");
  const parts = relative(evidenceRoot, destination).split("/");
  if (!/^[a-f0-9]{12}$/.test(run) || parts.length !== 2 || parts.some(part => !part || part === "..") || ![run, `template-${run}`].includes(basename(destination))) throw new Error("OWNED_BROWSER_EVIDENCE_DIRECTORY_REQUIRED");
  if (await realpath(destination) !== destination) throw new Error("OWNED_BROWSER_EVIDENCE_DIRECTORY_REQUIRED");
  const source = resolve(root, ".runtime/browser", run, "acme-organization");
  let exists;
  try { exists = await lstat(source); } catch (error) { if ((error as NodeJS.ErrnoException).code === "ENOENT") return; throw error; }
  if (!exists.isDirectory() || await realpath(source) !== source) throw new Error("DECLARED_BROWSER_FIXTURE_EVIDENCE_REQUIRED");
  async function copy(directory: string, target: string, path: string) {
    await mkdir(target, { recursive: true, mode: 0o700 });
    await chmod(target, 0o700);
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const name = path ? `${path}/${entry.name}` : entry.name;
      if (entry.isDirectory() && /^before-\d+$/.test(name)) await copy(join(directory, entry.name), join(target, entry.name), name);
      else if (entry.isFile() && /^(?:verification\.json|before-\d+\/(?:acme|plan)\.json)$/.test(name)) {
        await copyFile(join(directory, entry.name), join(target, entry.name));
        await chmod(join(target, entry.name), 0o600);
      } else throw new Error("DECLARED_BROWSER_FIXTURE_EVIDENCE_REQUIRED");
    }
  }
  await copy(source, join(destination, "acme-organization"), "");
}
