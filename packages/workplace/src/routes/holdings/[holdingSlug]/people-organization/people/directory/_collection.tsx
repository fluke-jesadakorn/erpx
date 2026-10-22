import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {PeopleOrganizationPage} from '@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/people-organization-page';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {forbidden} from 'next/navigation';
export default async function PeopleCollection({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const actor=await requireWorkContext((await params).holdingSlug),query=await searchParams;
 if(!['people.read','people.manage','iam.read','organization.manage'].some(permission=>hasPermission(actor,permission)))forbidden();
 if(query.status==='archived'&&!hasPermission(actor,'people.manage'))forbidden();
 if(query.new&&!hasPermission(actor,'people.manage'))forbidden();
 const search=Object.fromEntries(Object.entries(query).filter((entry):entry is [string,string]=>typeof entry[1]==='string'));
 return <PeopleOrganizationPage actor={actor} search={search}/>;
}
