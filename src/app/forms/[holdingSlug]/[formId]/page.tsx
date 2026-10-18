import { marketingPresentationQueries } from "@/server/modules/marketing/public/presentation-queries";
import {notFound} from 'next/navigation';
import {ApiError} from "@/server/modules/runtime/public/http";
import { holdingBindingBySlug } from "@/server/modules/runtime/public/runtime";
import {PublicMarketingForm} from "@/components/marketing/public-marketing-form";

import Image from 'next/image';

import { PageContent } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;formId:string}>}){
 const {holdingSlug,formId}=await params,binding=await holdingBindingBySlug(holdingSlug).catch(error=>{if(error instanceof Error&&!(error instanceof ApiError)&&error.message==='HOLDING_NOT_FOUND')notFound();throw error;}),queries=await marketingPresentationQueries(binding.id);
 const form=await queries.loadPublicMarketingForm(formId).catch(error=>{if(error instanceof ApiError&&error.status>=400&&error.status<500)notFound();throw error;});
 const preview=JSON.parse(form.preview_json) as {summary?:string;headline?:string},presentation=form.presentation;
 return <PageContent as="main" className="ui-page-content" style={{maxWidth:680,margin:'3rem auto',padding:'1rem'}}><h1>{form.name}</h1><p>{preview.headline??preview.summary??'Contact our team.'}</p>{presentation.content&&<section aria-label="Approved form content">{presentation.content.headline&&<h2>{presentation.content.headline}</h2>}{presentation.content.summary&&<p>{presentation.content.summary}</p>}{presentation.media&&<Image src={`/api/public/holdings/${encodeURIComponent(holdingSlug)}/media-publications/${encodeURIComponent(presentation.media.id)}/content`} alt={presentation.media.altText} width={640} height={400} unoptimized style={{width:'100%',height:'auto',objectFit:'contain'}}/>}</section>}<PublicMarketingForm holdingSlug={holdingSlug} formId={form.id} formVersion={form.version}/></PageContent>;
}
