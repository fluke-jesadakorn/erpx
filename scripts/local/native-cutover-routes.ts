function validId(segment: string): boolean {
  const id = decodeURIComponent(segment);
  return !!id && id !== "." && id !== ".." && !/[/\\\x00-\x1f\x7f]/.test(id);
}

export type HoldingNativeWorkspace = { branch?: string; department?: string };

export function normalizeHoldingNativePath(url: string, holdingSlug: string): { url: string; workspace?: HoldingNativeWorkspace } {
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!url.startsWith(prefix)) return { url };
  const scoped = /^(?:branches\/([a-z0-9_-]{1,128})\/(?:departments\/([a-z0-9_-]{1,128})\/)?|departments\/([a-z0-9_-]{1,128})\/)(.+)$/.exec(url.slice(prefix.length));
  if (!scoped) return { url };
  const [, branch, nestedDepartment, department, path] = scoped;
  return { url: prefix + path, workspace: { branch, department: nestedDepartment ?? department } };
}

/** Holding routes whose corresponding Next entry points have been retired. */
export function holdingNativeCutoverPath(url: string, holdingSlug: string, method: string | undefined): "records" | "record-action" | "routine-templates" | "routine-command" | undefined {
  const [path] = url.split("?", 1);
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!path?.startsWith(prefix)) return;
  const [owner, segment, ...tail] = path.slice(prefix.length).split("/");
  if (owner === "routine-templates") {
    try {
      if (segment !== undefined && !validId(segment)) return;
      if (!tail.every(validId)) return;
      return method === "POST" ? "routine-command" : owner;
    } catch {
      return;
    }
  }
  if (owner !== "records" || !segment) return;
  try {
    const module = decodeURIComponent(segment);
    if (!/^[a-z0-9_-]+$/.test(module) || module === "search" || module === "native-search") return;
    if (tail.length === 0) return owner;
    if (!validId(tail[0]!)) return;
    if (tail.length === 1 || tail.length === 2 && ["references", "transition"].includes(tail[1]!)) return "record-action";
  } catch {
    return;
  }
}
