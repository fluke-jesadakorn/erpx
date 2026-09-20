"use client";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
export function WorkReturn({base}: {base: string}) {
 const query = useSearchParams(), value = query.get("return");
 if (!value) return null;
 const back = new URLSearchParams(value);
 for (const key of [...back.keys()]) if (!["view","q","queue","type","responsibility"].includes(key)) back.delete(key);
 return <Link className="back-link" href={`${base}/work?${back}`}>← Back to Work</Link>;
}
