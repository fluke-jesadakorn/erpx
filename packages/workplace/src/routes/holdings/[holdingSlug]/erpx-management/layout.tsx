import {notFound} from 'next/navigation';
import {requireManageErpxContext} from '@erpx/server-identity_access/public/authorization';
export default async function ManagementLayout({children,params}:{children:React.ReactNode;params:Promise<{holdingSlug:string}>}) {
 if((await params).holdingSlug!=='erpx')notFound();
 await requireManageErpxContext();
 return children;
}
