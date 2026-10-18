"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import Link from "@/components/workplace/workspace-link";
import { useSearchParams } from "@/lib/shared/destination-search";
export function WorkReturn({base}: {base: string}) {
 const query = useSearchParams(), value = query.get("return");
 if (!value) return null;
 const back = new URLSearchParams(value);
 for (const key of [...back.keys()]) if (!["view","q","queue","type","responsibility","tracking","offset"].includes(key)) back.delete(key);
 return <Link className="back-link" href={canonicalUiHref(`${base}/work?${back}`)}>← Back to My work</Link>;
}
