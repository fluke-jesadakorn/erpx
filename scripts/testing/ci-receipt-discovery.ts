import { realpath, readdir } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";

const receiptName = /^(?:native|browser-\d+-\d+)\.json$/;
const archivedDirectory = /^(?:attempts?|archives?|baselines?|ci-failed(?:-|$))/i;

/** Discover current aggregate receipts without importing retained retry/archive evidence. */
export async function discoverCiReceiptFiles(root: string): Promise<string[]> {
  const rejectArchivedAncestors = (directory: string) => {
    let current = directory;
    while (true) {
      if (archivedDirectory.test(basename(current))) throw new Error("CI_ARCHIVED_RECEIPT_DIRECTORY_FORBIDDEN");
      const parent = dirname(current);
      if (parent === current) break;
      current = parent;
    }
  };

  const lexicalRoot = resolve(root);
  rejectArchivedAncestors(lexicalRoot);
  const canonicalRoot = await realpath(lexicalRoot);
  rejectArchivedAncestors(canonicalRoot);

  async function visit(directory: string): Promise<string[]> {
    const entries = await readdir(directory, { withFileTypes: true });
    const discovered = await Promise.all(entries.map(entry => {
      if (entry.isDirectory()) return archivedDirectory.test(entry.name) ? [] : visit(join(directory, entry.name));
      return entry.isFile() && receiptName.test(entry.name) ? [join(directory, entry.name)] : [];
    }));
    return discovered.flat();
  }

  return (await visit(lexicalRoot)).sort();
}
