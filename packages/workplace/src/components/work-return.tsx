"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import Link from "@erpx/workplace/components/workspace-link";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
export function WorkReturn({base}: {base: string}) {
 const query = useSearchParams(), value = query.get("return");
 if (!value) return null;
 const back = new URLSearchParams(value);
 for (const key of [...back.keys()]) if (!["view","q","queue","type","responsibility","tracking","offset"].includes(key)) back.delete(key);
 return <Link className="back-link" href={canonicalUiHref(`${base}/work?${back}`)}>← Back to My work</Link>;
}
