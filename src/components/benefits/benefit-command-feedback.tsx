"use client";
import {RecordLink} from '@/components/records/record-link';
import {Button,InlineNotice} from '@/components/ui/foundation';
import {benefitRetryAllowed,type BenefitPresentation} from '@/lib/benefits/presentation';
import type {useBenefitCommand} from './use-benefit-command';
/** Render inside the active drawer as well: its backdrop makes page actions inert. */
export function BenefitCommandFeedback({command,view}:{command:ReturnType<typeof useBenefitCommand>;view:BenefitPresentation}){
 return <>
  {command.retryAttempt&&<InlineNotice tone="warning"><p>The submitted action has an uncertain result. Its original request is retained separately from newer draft edits. Closing this editor does not cancel that request.</p><Button appearance="standard" clientManaged disabled={command.busy||!benefitRetryAllowed(view,command.retryAttempt)} onClick={()=>void command.retry()}>Retry submitted Benefits action</Button></InlineNotice>}
  {command.notice&&<InlineNotice tone="info">{command.notice}</InlineNotice>}
  {command.savedDraft&&<div className="ui-form-actions"><RecordLink href={command.savedDraft.href}>Open saved Benefits record</RecordLink><Button appearance="standard" onClick={command.clearSaved}>Review retained draft</Button></div>}
 </>;
}
