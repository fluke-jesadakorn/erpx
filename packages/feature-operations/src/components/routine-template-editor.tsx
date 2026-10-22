'use client';
import {useState} from 'react';
import {Button,Field,Select,TextArea,TextInput,type UiAppearance} from '@erpx/ui/components/foundation';
import {StaffPicker,Approvers} from '@erpx/feature-work_projection/components/staff-work-controls';
import type {StaffPickerDisplayOptions} from '@erpx/feature-work_projection/lib/staff-picker-display';
import type {TemplateRow} from '@erpx/server-operations/public/routines';
import type {RoutinePurchases} from '@erpx/server-operations/public/rules/routine-purchase-controls';
import {routineSourcePurchase} from '@erpx/feature-operations/lib/routine-purchase-controls';
import {useRoutineCommand} from '@erpx/feature-operations/components/use-routine-command';
import {RoutineCommandFeedback} from '@erpx/feature-operations/components/routine-command-feedback';
import {RoutineEditor} from '@erpx/feature-operations/components/routine-editor';
type Draft={title:string;description:string;assigneeUserId:string;projectId:string|null;startApprovers:string[];completionApprovers:string[];published:boolean};
export function RoutineTemplateEditor({base,options,projects,userId,template,clone=false,purchases,canWrite=true,appearance='standard'}:{base:string;options:StaffPickerDisplayOptions;projects:{id:string;title:string}[];userId:string;template?:TemplateRow;clone?:boolean;canWrite?:boolean;purchases?:RoutinePurchases;appearance?:UiAppearance}){
 const initial=():Draft=>({title:`${clone?'Copy of ':''}${template?.title??''}`,description:template?.description??'',assigneeUserId:template?.assignee_user_id??userId,projectId:template?.project_id??null,startApprovers:template?JSON.parse(template.start_approvers_json):[],completionApprovers:template?JSON.parse(template.completion_approvers_json):[''],published:template?!!template.published:true});
 const [draft,setDraft]=useState(initial),[saved,setSaved]=useState<{id:string;version:number;input:Draft}|null>(null),[phase,setPhase]=useState(()=>template?(template.start_approvers_json!=='[]'?template.completion_approvers_json!=='[]'?'both':'start':'completion'):'completion');
 const purchase=routineSourcePurchase(purchases,draft.projectId,template?.project_id),command=useRoutineCommand(base,purchase,purchases?.context??base,canWrite),active=!purchase||purchase.newWork,title=clone?'Clone routine template':template?'Edit routine template':'New routine template';
 const reset=()=>{const next=saved?.input??initial();setDraft(next);setPhase(next.startApprovers.length?next.completionApprovers.length?'both':'start':'completion');};
 const update=<Key extends keyof Draft>(key:Key,value:Draft[Key])=>setDraft(current=>({...current,[key]:value}));
 return <><RoutineCommandFeedback command={command}/><RoutineEditor title={title} appearance={appearance} busy={command.busy} error={command.error} onReset={reset} recovery={<RoutineCommandFeedback command={command}/>}>
  <form data-routine-draft data-ui-appearance={appearance} className="ui-form-grid" onSubmit={event=>{event.preventDefault();if(!active||!canWrite)return;const input:Draft={...draft,startApprovers:phase==='start'||phase==='both'?draft.startApprovers:[],completionApprovers:phase==='completion'||phase==='both'?draft.completionApprovers:[]},id=saved?.id??(clone?undefined:template?.id),path=id?`routine-templates/${id}`:clone&&template?`routine-templates/${template.id}/clone`:'routine-templates';void command.send(path,{...input,expectedVersion:saved?.version??template?.version??1},event.currentTarget,false,'routine-templates',result=>setSaved({id:result.id,version:result.version,input}));}}>
   <Field appearance={appearance}>Title<TextInput appearance={appearance} name="title" minLength={2} maxLength={200} required value={draft.title} onChange={event=>update('title',event.target.value)}/></Field>
   <Field appearance={appearance}>Project<Select appearance={appearance} name="projectId" value={draft.projectId??''} onChange={event=>update('projectId',event.target.value||null)}><option value="">Standalone work</option>{projects.map(project=><option key={project.id} value={project.id}>{project.title}</option>)}</Select></Field>
   <Field appearance={appearance}>Description<TextArea appearance={appearance} name="description" maxLength={10000} value={draft.description} onChange={event=>update('description',event.target.value)}/></Field>
   <StaffPicker appearance={appearance} label="Accountable assignee" value={draft.assigneeUserId} onChange={value=>update('assigneeUserId',value)} options={options}/>
   <Field appearance={appearance}>Review<Select appearance={appearance} value={phase} onChange={event=>{const value=event.target.value;setPhase(value);if((value==='start'||value==='both')&&!draft.startApprovers.length)update('startApprovers',['']);if((value==='completion'||value==='both')&&!draft.completionApprovers.length)update('completionApprovers',['']);}}><option value="completion">After completion</option><option value="start">Before work starts</option><option value="both">Before work and after completion</option></Select></Field>
   {(phase==='start'||phase==='both')&&<Approvers appearance={appearance} label="Start approval" values={draft.startApprovers} onChange={values=>update('startApprovers',values)} options={options} exclude={[draft.assigneeUserId,clone?userId:template?.creator_user_id??userId]}/>}
   {(phase==='completion'||phase==='both')&&<Approvers appearance={appearance} label="Completion approval" values={draft.completionApprovers} onChange={values=>update('completionApprovers',values)} options={options} exclude={[draft.assigneeUserId]}/>}
   <Field appearance={appearance} orientation="inline"><TextInput appearance={appearance} name="published" type="checkbox" checked={draft.published} onChange={event=>update('published',event.target.checked)}/>Publish for company staff</Field>
   {!active&&<p className="ui-muted">New or amended routine configuration requires an active package for its actual source.</p>}
   <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={command.busy||!active||!canWrite}>{clone&&!saved?'Create my copy':'Save routine template'}</Button>
  </form>
 </RoutineEditor></>;
}
