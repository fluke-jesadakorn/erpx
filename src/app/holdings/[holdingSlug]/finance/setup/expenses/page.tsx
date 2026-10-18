import Screen from '@/app/holdings/[holdingSlug]/_presenters/finance/expense-setup/screen';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){return (await Screen({params}))??null;}
