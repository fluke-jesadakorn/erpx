import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";

/** Current navigation is checked even when its destination is historical. */
export function immutableDocument(name: string, protectedPaths: ReadonlySet<string>): boolean {
  return protectedPaths.has(name) || name.startsWith("docs/testing/evidence/") ||
    (name.startsWith("docs/archive/") && name !== "docs/archive/README.md");
}

export function documentationFiles(root: string): string[] {
  const documents: string[] = [];
  function walk(directory: string): void {
    if (!existsSync(directory)) return;
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && /\.mdx?$/.test(entry.name)) documents.push(path);
    }
  }
  for (const directory of ["docs", "ops", "Designed", "fixtures", ".agents", ".github", ".claude"])
    walk(resolve(root, directory));
  for (const entry of readdirSync(root, { withFileTypes: true }))
    if (entry.isFile() && /\.mdx?$/.test(entry.name)) documents.push(resolve(root, entry.name));
  return documents.sort();
}

/** Deleted working-tree files may remain in the index until the user stages them. */
export function trackedTemporaryFiles(root: string): string[] {
  return execFileSync("git", ["ls-files", "-z", "--", ".scratch", ".tmp"], { cwd: root, encoding: "utf8" })
    .split("\0").filter(name => name && existsSync(resolve(root, name))).sort();
}

const withoutCode = (text: string) => text.replace(/```[\s\S]*?```|~~~[\s\S]*?~~~/g, "");

export function checkDocumentationLinks(
  root: string,
  protectedPaths: ReadonlySet<string>,
  historicalSource: (document: string, file: string) => boolean = () => false,
): { documents: string[]; checked: number; errors: string[] } {
  const documents = documentationFiles(root), errors: string[] = [], anchorCache = new Map<string, Set<string>>();
  function anchors(path: string): Set<string> {
    const cached = anchorCache.get(path);
    if (cached) return cached;
    const text = withoutCode(readFileSync(path, "utf8")), ids = new Set<string>(), counts = new Map<string, number>();
    for (const heading of text.matchAll(/^#{1,6}\s+(.+)$/gm)) {
      const label = heading[1].replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/<[^>]*>/g, "");
      const slug = label.toLowerCase().replace(/[^\p{L}\p{N}\p{M}_\s-]/gu, "").replace(/\s/g, "-");
      const count = counts.get(slug) ?? 0;
      counts.set(slug, count + 1);
      ids.add(count ? `${slug}-${count}` : slug);
    }
    for (const attribute of text.matchAll(/(?:id|name)=["']([^"']+)["']/g)) ids.add(attribute[1]);
    anchorCache.set(path, ids);
    return ids;
  }

  let checked = 0;
  for (const path of documents) {
    const name = relative(root, path);
    if (immutableDocument(name, protectedPaths)) continue;
    const text = withoutCode(readFileSync(path, "utf8"));
    const targets = [
      ...text.matchAll(/!?\[[^\]]*\]\((<[^>]+>|[^\s)]+)(?:\s+["'][^"']*["'])?\)/g),
      ...text.matchAll(/^\s*\[[^\]]+\]:\s*(<[^>]+>|\S+)/gm),
    ];
    for (const target of targets) {
      const raw = target[1].replace(/^<|>$/g, "");
      if (/^[\w+.-]+:|^\/\//.test(raw)) continue;
      const [destination, fragment] = raw.split("#");
      const resolved = destination ? resolve(dirname(path), decodeURIComponent(destination)) : path;
      checked++;
      if (!existsSync(resolved)) {
        if (!historicalSource(name, relative(root, resolved))) errors.push(`${name}: missing local link ${raw}`);
      } else if (fragment && /\.mdx?$/.test(resolved) && !anchors(resolved).has(decodeURIComponent(fragment))) {
        errors.push(`${name}: missing anchor ${raw}`);
      }
    }
  }
  return { documents, checked, errors };
}
