'use client';
import {Button,InlineNotice} from '@erpx/ui/components/foundation';
import {RecordLink} from '@erpx/feature-records/components/record-link';
import type {useRoutineCommand} from '@erpx/feature-operations/components/use-routine-command';
/** Also render within the active drawer so recovery remains reachable. */
export function RoutineCommandFeedback({command}:{command:ReturnType<typeof useRoutineCommand>}){return <>{command.retryAttempt&&<InlineNotice tone="warning"><p>The submitted routine action has an uncertain result. Its original request is retained separately from newer edits. Closing this editor does not cancel that request.</p><Button appearance="standard" clientManaged disabled={command.busy||!command.retryAllowed} onClick={()=>void command.retry()}>Retry submitted routine action</Button></InlineNotice>}{command.notice&&<InlineNotice tone="info">{command.notice}</InlineNotice>}{command.saved&&<div className="ui-form-actions"><RecordLink href={command.saved.href}>Open saved routine work</RecordLink><Button appearance="standard" onClick={command.clearSaved}>Review retained draft</Button></div>}</>;}
