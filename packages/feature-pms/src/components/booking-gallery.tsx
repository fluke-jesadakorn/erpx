"use client";
import Image from 'next/image';
import { useState } from 'react';
import { Button } from '@erpx/ui/components/foundation';
import type { BookingImage } from "@erpx/server-pms/public/rules/pms-model";
import styles from '@erpx/feature-pms/components/booking.module.css';
export function BookingGallery({slug,images=[],compact=false}:{slug:string;images?:BookingImage[];compact?:boolean}){
 const [selected,setSelected]=useState(0);
 if(!images.length)return null;
 const image=images[Math.min(selected,images.length-1)];
 return <figure className={`${styles.gallery} ${compact?styles.galleryCompact:''}`}>
  <Image unoptimized src={`/api/public/holdings/${encodeURIComponent(slug)}/media-publications/${encodeURIComponent(image.publicationId)}/content`} alt={image.alt} width={1200} height={760} sizes="(max-width: 700px) 100vw, 70vw" />
  {images.length>1&&<figcaption>{images.map((image,index)=><Button appearance="standard" type="button" variant="secondary" key={image.publicationId} aria-label={image.alt} aria-pressed={selected===index} onClick={()=>setSelected(index)}>{index+1}</Button>)}</figcaption>}
 </figure>;
}
