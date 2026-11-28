import {IamConsole} from '@erpx/identity-access-iam-ui/components/sections/groups';
import {iamPresentationQueries} from '@erpx/identity-access-server/public/presentation-context-queries';
import {requireManageErpxContext} from '@erpx/identity-access-server/public/authorization';
export default async function Page(){await requireManageErpxContext();const {queries,actor}=await iamPresentationQueries(null);return <IamConsole data={await queries.iamOverview(actor,false)} section="groups" base="/holdings/erpx/erpx-management" api="/api/v1/platform/iam"/>;}
