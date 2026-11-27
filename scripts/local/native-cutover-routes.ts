function validId(segment: string): boolean {
  const id = decodeURIComponent(segment);
  return !!id && id !== "." && id !== ".." && !/[/\\\x00-\x1f\x7f]/.test(id);
}

export type HoldingNativeWorkspace = { branch?: string; department?: string };

function canonicalQualityAction(url: string, prefix: string): string {
  const match = /^quality\/([^/?]+)(?:\/([^/?]+))?(\?.*)?$/.exec(url.slice(prefix.length));
  if (!match) return url;
  try {
    if (match[1] !== "specifications" && decodeURIComponent(match[1]!) === "specifications")
      return `${prefix}quality/specifications${match[2] ? `/${match[2]}` : ""}${match[3] ?? ""}`;
    if (!match[2]?.includes("%")) return url;
    const action = decodeURIComponent(match[2]);
    if (action === "cancellations" || action === "acknowledge")
      return `${prefix}quality/${match[1]}/${action}${match[3] ?? ""}`;
  } catch {
    return url;
  }
  return url;
}

export function normalizeHoldingNativePath(url: string, holdingSlug: string): { url: string; workspace?: HoldingNativeWorkspace } {
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!url.startsWith(prefix)) return { url };
  const scoped = /^(?:branches\/([a-z0-9_-]{1,128})\/(?:departments\/([a-z0-9_-]{1,128})\/)?|departments\/([a-z0-9_-]{1,128})\/)(.+)$/.exec(url.slice(prefix.length));
  if (!scoped) return { url: canonicalQualityAction(url, prefix) };
  const [, branch, nestedDepartment, department, path] = scoped;
  return { url: canonicalQualityAction(prefix + path, prefix), workspace: { branch, department: nestedDepartment ?? department } };
}

/** Native Holding operations and paths reserved for private SSR. */
export function holdingNativeCutoverPath(url: string, holdingSlug: string, method: string | undefined): "records" | "record-action" | "routine-templates" | "routine-schedules" | "routine-command" | "routine-editor-private" | "quality-read" | "quality-command" | undefined {
  const [path] = url.split("?", 1);
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!path?.startsWith(prefix)) return;
  const [owner, segment, ...tail] = path.slice(prefix.length).split("/");
  if (owner === "quality" && method === "POST" && segment === "specifications" && tail.length <= 1) {
    try {
      if (tail.length === 0 || validId(tail[0]!)) return "quality-command";
    } catch {
      return;
    }
  }
  if (owner === "quality" && method === "POST" && segment && tail.length === 1 && tail[0] === "acknowledge") {
    try {
      if (validId(segment)) return "quality-command";
    } catch {
      return;
    }
  }
  if (owner === "quality" && (method === "GET" || method === "HEAD")) {
    if (segment === undefined) return "quality-read";
    try {
      if (!validId(segment)) return;
      if (tail.length === 1 && tail[0] === "cancellations") return "quality-read";
      if (tail.length === 0 && !["alerts", "read-plan", "specifications"].includes(decodeURIComponent(segment))) return "quality-read";
    } catch {
      return;
    }
  }
  if (owner === "routine-template-editor-context" && segment === undefined) return "routine-editor-private";
  if (owner === "routine-templates" || owner === "routine-schedules") {
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
    if (tail.length === 1 || tail.length === 2 && ["references", "transition", "mentions", "suggestions"].includes(tail[1]!)) return "record-action";
    if (tail.length === 3 && tail[1] === "suggestions" && validId(tail[2]!)) return "record-action";
  } catch {
    return;
  }
}
