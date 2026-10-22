import {IamConsole} from '@erpx/feature-iam/components/sections/users';
import {iamPresentationQueries} from '@erpx/server-identity_access/public/presentation-context-queries';
import {requireManageErpxContext} from '@erpx/server-identity_access/public/authorization';
export default async function Page(){await requireManageErpxContext();const {queries,actor}=await iamPresentationQueries(null);return <IamConsole data={await queries.iamOverview(actor,false)} section="users" base="/holdings/erpx/erpx-management" api="/api/v1/platform/iam"/>;}
