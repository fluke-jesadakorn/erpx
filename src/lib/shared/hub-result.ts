import { sourcePath,workspacePathParts } from "@/lib/work_projection/destination-registry";
import { recordDrawerHref, recordTarget } from "@/lib/records/record-drawer";

/** Preserve the active workspace and its filters when a command opens its result. */
export function hubResultHref(currentHref: string, base: string, destination: string) {
  const current = new URL(currentHref, "https://erpx.invalid");
  const parts=workspacePathParts(current.pathname);
  if(!parts||parts.base!==base||!['sales','customers'].includes(sourcePath(parts.path)))return null;
  for (const key of ["new", "edit", "request"]) current.searchParams.delete(key);
  const target = recordTarget(destination, base);
  return target ? recordDrawerHref(current.href, target) : `${current.pathname}${current.search}`;
}
