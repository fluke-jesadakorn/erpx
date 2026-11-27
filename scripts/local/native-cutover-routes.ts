/** Holding routes whose corresponding Next entry points have been retired. */
export function holdingNativeCutoverPath(url: string, holdingSlug: string, method: string | undefined): "records" | "routine-templates" | undefined {
  const [path] = url.split("?", 1);
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!path?.startsWith(prefix)) return;
  const [owner, segment, ...tail] = path.slice(prefix.length).split("/");
  if (tail.length) return;
  if (owner === "routine-templates") {
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
    if (/^[a-z0-9_-]+$/.test(module) && module !== "search" && module !== "native-search") return owner;
  } catch {
    return;
  }
}
