import 'server-only';
import {sourceRecordPresentation} from '@erpx/feature-records/components/source-record-presentation';
import {pageError} from '@erpx/server-work_projection/public/page-error';
import {configuredProductHandoff} from '@erpx/feature-runtime/lib/product-hosts';
import {FeaturePage} from '@erpx/feature-shared/components/features/feature-page';
import {InlineNotice} from '@erpx/ui/components/foundation';
import {StatusBadge} from '@erpx/ui/components/primitives';
export async function ProjectTaskSourceRecordBackdrop({holdingSlug,id}:{holdingSlug:string;id:string}){
 const record=await sourceRecordPresentation(holdingSlug,'staff_task',id).catch(error=>pageError(error,holdingSlug));
 if(!record)return null;
 return <FeaturePage className="ui-page-content" data-source="live" data-module="projects"><header className="ui-section-header"><div><p className="eyebrow">Included Project task</p><h1>{record.title}</h1></div><StatusBadge appearance="standard" status={record.status}/></header><InlineNotice tone="info">Open the task details to continue this existing Project work and review its evidence.</InlineNotice><a data-product-switch href={configuredProductHandoff('apps')??'/'}>Choose a product in Apps</a></FeaturePage>;
}
