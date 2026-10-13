"use client";
import { useEffect,useState } from "react";
import Image from "next/image";
import Link from "next/link";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { prepareDocumentPages } from "@/lib/files/document-pages";
import type { WorkReview } from "@/lib/work_projection/work-review";
export function ReviewEvidence({file}:{file:WorkReview["evidence"][number]}){
 const [open,setOpen]=useState(false);
 return <details onToggle={e=>setOpen(e.currentTarget.open)}><summary>{file.name}</summary>{file.contentHref?<><a href={file.contentHref} target="_blank" rel="noreferrer">Open / download original</a>{open&&<EvidencePages file={file}/>}</>:<Link href={file.href}>Open evidence in source record</Link>}</details>;
}
function EvidencePages({file}:{file:WorkReview["evidence"][number]}){
 const [pages,setPages]=useState<{url:string;page:number;total:number}[]>([]),[error,setError]=useState("");
 useEffect(()=>{
  if(file.mediaType!=="application/pdf"||!file.contentHref||Number(file.bytes)>25*1024*1024)return;
  let cancelled=false;const urls:string[]=[],controller=new AbortController();
  void (async()=>{try{
   const response=await workFetch(file.contentHref!,{signal:controller.signal,cache:"no-store"});if(!response.ok)throw Error("The file is unavailable or your evidence access has changed. Open the source record to check.");
   const blob=await response.blob();if(blob.size>25*1024*1024)throw Error("Use the original download for this large document.");
   await prepareDocumentPages(new File([blob],file.name,{type:"application/pdf"}),async(page,total,image)=>{if(cancelled)return;const url=URL.createObjectURL(image);urls.push(url);setPages(previous=>[...previous,{url,page,total}]);});
  }catch(error){if(!cancelled)setError(error instanceof Error?error.message:"Unable to preview this document.");}})();
  return()=>{cancelled=true;controller.abort();urls.forEach(url=>URL.revokeObjectURL(url));};
 },[file]);
 if(file.mediaType?.startsWith("image/")&&file.previewHref)return <Image unoptimized width={720} height={960} style={{width:"100%",height:"auto"}} src={file.previewHref} alt={file.name}/>;
 if(file.mediaType!=="application/pdf")return null;
 if(Number(file.bytes)>25*1024*1024)return <p>Use the original download for this large document.</p>;
 return <section aria-label={`Pages of ${file.name}`}>{!pages.length&&!error&&<p role="status">Preparing document pages…</p>}{pages.map(p=><figure key={p.page}><Image unoptimized width={720} height={960} style={{width:"100%",height:"auto"}} src={p.url} alt={`${file.name}, page ${p.page} of ${p.total}`}/><figcaption>Page {p.page} of {p.total}</figcaption></figure>)}{error&&<p role="alert">{error}</p>}</section>;
}
