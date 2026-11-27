/** Holding routes whose corresponding Next entry points have been retired. */
export function holdingNativeCutoverPath(url: string, holdingSlug: string, method: string | undefined): "records" | "record-action" | "routine-templates" | undefined {
  const [path] = url.split("?", 1);
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!path?.startsWith(prefix)) return;
  const [owner, segment, ...tail] = path.slice(prefix.length).split("/");
  if (owner === "routine-templates") {
    if (tail.length) return;
    if (method !== "GET" && method !== "HEAD") return;
    if (segment === undefined) return owner;
    try {
      const id = decodeURIComponent(segment);
      if (id.length > 0 && id !== "." && id !== ".." && !/[/\\\x00]/.test(id)) return owner;
    } catch {
      return;
    }
  }
  if (owner !== "records" || !segment) return;
  try {
    const module = decodeURIComponent(segment);
    if (!/^[a-z0-9_-]+$/.test(module) || module === "search" || module === "native-search") return;
    if (tail.length === 0) return owner;
    const id = decodeURIComponent(tail[0]!);
    if (!id || id === "." || id === ".." || /[/\\\x00-\x1f\x7f]/.test(id)) return;
    if (tail.length === 1 || tail.length === 2 && ["references", "transition"].includes(tail[1]!)) return "record-action";
  } catch {
    return;
  }
}
