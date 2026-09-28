import {notFound} from 'next/navigation';
import {holdingBindingBySlug,holdingResources} from '@/server/runtime';
import {PublicMarketingForm} from '@/components/modules/public-marketing-form';
import {assertProductionModuleEnabled} from '@/server/hospitality-rollout';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;formId:string}>}){
 const {holdingSlug,formId}=await params,binding=await holdingBindingBySlug(holdingSlug).catch(()=>notFound()),{write}=await holdingResources(binding.id);
 const form=await write.prepare("SELECT id,company_id,name,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'").bind(formId).first<{id:string;company_id:string;name:string;preview_json:string;configuration_json:string}>();
 if(!form||JSON.parse(form.configuration_json).consentRequired!==true)notFound();
 await assertProductionModuleEnabled(write,form.company_id,'marketing').catch(()=>notFound());
 const preview=JSON.parse(form.preview_json) as {summary?:string;headline?:string};
 return <main className="content-stack" style={{maxWidth:680,margin:'3rem auto',padding:'1rem'}}><h1>{form.name}</h1><p>{preview.headline??preview.summary??'Contact our team.'}</p><PublicMarketingForm holdingSlug={holdingSlug} formId={form.id}/></main>;
}
