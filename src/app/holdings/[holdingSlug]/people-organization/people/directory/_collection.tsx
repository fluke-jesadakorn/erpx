import {requireWorkContext} from '@/server/modules/identity_access/public/authorization';
import {PeopleOrganizationPage} from '@/app/holdings/[holdingSlug]/_presenters/people/people-organization-page';
import {hasPermission} from '@/lib/identity_access/permissions';
import {forbidden} from 'next/navigation';
export default async function PeopleCollection({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const actor=await requireWorkContext((await params).holdingSlug),query=await searchParams;
 if(!['people.read','people.manage','iam.read','organization.manage'].some(permission=>hasPermission(actor,permission)))forbidden();
 if(query.status==='archived'&&!hasPermission(actor,'people.manage'))forbidden();
 if(query.new&&!hasPermission(actor,'people.manage'))forbidden();
 const search=Object.fromEntries(Object.entries(query).filter((entry):entry is [string,string]=>typeof entry[1]==='string'));
 return <PeopleOrganizationPage actor={actor} search={search}/>;
}
