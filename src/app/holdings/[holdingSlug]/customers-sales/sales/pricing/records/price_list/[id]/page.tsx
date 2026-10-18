import Screen from '@/app/holdings/[holdingSlug]/customers-sales/sales/pricing/_screen';
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>}){
 const p=await params,section=defaultRecordSection("price_list");if(!(recordSections.price_list??[defaultRecordSection('price_list')]).includes(section))notFound();
 return <Screen params={Promise.resolve({holdingSlug:p.holdingSlug})} searchParams={Promise.resolve({})}/>;
}
