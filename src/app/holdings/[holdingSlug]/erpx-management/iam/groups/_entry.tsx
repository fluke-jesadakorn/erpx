import {IamConsole} from '@/components/iam/sections/groups';
import {iamPresentationQueries} from '@/server/modules/identity_access/public/presentation-context-queries';
import {requireManageErpxContext} from '@/server/modules/identity_access/public/authorization';
export default async function Page(){await requireManageErpxContext();const {queries,actor}=await iamPresentationQueries(null);return <IamConsole data={await queries.iamOverview(actor,false)} section="groups" base="/holdings/erpx/erpx-management" api="/api/v1/platform/iam"/>;}
