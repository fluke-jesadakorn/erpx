"use client";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import Link from "@erpx/platform-workplace/components/workspace-link";
import { useSearchParams } from "@erpx/platform-shared-ui/lib/destination-search";
export function WorkReturn({base}: {base: string}) {
 const query = useSearchParams(), value = query.get("return");
 if (!value) return null;
 const back = new URLSearchParams(value);
 for (const key of [...back.keys()]) if (!["view","q","queue","type","responsibility","tracking","offset"].includes(key)) back.delete(key);
 return <Link className="back-link" href={canonicalUiHref(`${base}/work?${back}`)}>← Back to My work</Link>;
}
