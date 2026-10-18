"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {RecordLink as Link} from "@/components/records/record-link";
import {learningPurchased,learningResultPurchased,type LearningConsoleData,type LearningConsoleDetail} from "@/lib/learning/purchase-controls";
import {useLearningCommand} from "./use-learning-command";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { FormLayout } from "@/components/ui/foundation";
export function LearningConsole({ recordOnly=false, section, embedded=false,
  base,
  data,
  detail,
}: { recordOnly?:boolean; section?:"courses"|"assignments"; embedded?:boolean;
  base: string;
  data?: LearningConsoleData;
  detail?: LearningConsoleDetail;
}) {
  const source=data??detail!,a=detail?.assignment,purchases=source.purchases;
  const courseNew=source.canWrite&&learningPurchased(purchases,'learn.courses','newWork'),courseSettle=source.canWrite&&learningPurchased(purchases,'learn.courses','settleExisting'),assignNew=source.canWrite&&source.canReview&&learningPurchased(purchases,'learn.assignments','newWork')&&learningPurchased(purchases,'learn.courses','newWork')&&learningPurchased(purchases,'people.learner-reference','newWork'),assignmentSettle=source.canWrite&&learningPurchased(purchases,'learn.assignments','settleExisting'),own=!!a&&a.learner_id===source.actorId&&Number(a.learner_eligible)>0,resultSettle=assignmentSettle&&learningResultPurchased(purchases,a?.required_for);
  const [open,setOpen]=useState({course:false,assignment:false,result:false}),[generation,setGeneration]=useState({course:0,assignment:0,result:0}),[completionFiles,setCompletionFiles]=useState<AttachmentPanelChange|null>(null);
  function allowed(input:Record<string,unknown>){if(!source.canWrite)return false;if(input.entity==='course')return typeof input.id!=='string'?courseNew:source.canReview&&(input.action==='publish'?courseNew:courseSettle);if(typeof input.id!=='string')return assignNew;return input.action==='start'?own&&assignmentSettle:input.action==='complete'?own&&resultSettle:source.canReview&&(input.action==='waive'?resultSettle:assignmentSettle);}
  const {send,retry,retryAttempt,busy,error,notice,uncertain,savedDraft,clearSaved}=useLearningCommand(base,allowed,attempt=>{const input=attempt.input,review=input.entity==='course'?typeof input.id==='string':typeof input.id!=='string'||['waive','cancel'].includes(String(input.action));return source.canWrite&&learningPurchased(purchases,input.entity==='course'?'learn.courses':'learn.assignments','history')&&(review?source.canReview:input.entity==='course'||a?.learner_id===source.actorId);});
  const locked=busy||uncertain,disabled=busy||!!savedDraft;
  function discard(kind:keyof typeof open){setOpen(value=>({...value,[kind]:false}));setGeneration(value=>({...value,[kind]:value[kind]+1}));if(kind==='result')setCompletionFiles(null);clearSaved();}
  return (
    <div className="ui-stack" data-owner-embedded={embedded||undefined}>
      {notice&&<InlineNotice tone="info" role="status">{notice}</InlineNotice>}
      {savedDraft&&<p><Link href={canonicalUiHref(savedDraft.entity==='course'?`${base}/knowledge/learning/courses?detailKind=learning_course&detailId=${encodeURIComponent(savedDraft.id)}`:`${base}/learning/${savedDraft.id}`)}>Review the saved {savedDraft.entity}</Link> before submitting newer details.</p>}
      {retryAttempt&&source.canWrite&&<Button appearance="standard" variant="secondary" disabled={disabled} onClick={()=>void retry()}>Retry submitted Learning action</Button>}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {section!=="assignments" && data?.canWrite && (courseNew||open.course) && (
        (!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" open={open.course} onToggle={event=>{const expanded=event.currentTarget.open;setOpen(value=>({...value,course:expanded}));}} onDiscard={()=>discard("course")} error={error} dismissDisabled={locked}><summary>Draft course</summary><FormLayout key={generation.course}
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            void send({
              ...f,
              entity: "course",
              durationMinutes: Number(f.durationMinutes),
              requiredFor: f.requiredFor || null,
            },e.currentTarget);
          }}
        >
          <h2>Draft course</h2>
          <Field appearance="standard">
            Title
            <TextInput appearance="standard" name="title" required />
          </Field>
          <Field appearance="standard">
            Description
            <TextArea appearance="standard" name="description" required minLength={3} />
          </Field>
          <Field appearance="standard">
            Provider
            <TextInput appearance="standard" name="provider" required />
          </Field>
          <Field appearance="standard">
            Minutes
            <TextInput appearance="standard" name="durationMinutes" type="number" min="1" required />
          </Field>
          <Field appearance="standard">
            Eligibility code
            <TextInput appearance="standard" name="requiredFor" />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={disabled||!courseNew}>
            Draft course
          </Button>
        </FormLayout></FormDisclosure>)
      )}
      {section!=="assignments" && data?.canReview &&
        data.courses
          .filter((x) => ["draft","published"].includes(String(x.status))&&x.created_by!==source.actorId&&(x.status==="draft"?(courseNew||courseSettle):courseSettle))
          .map((x) => (
            <FormLayout
              className="ui-panel ui-form-grid"
              key={String(x.id)}
              onSubmit={(e) => {
                e.preventDefault();
                const f = Object.fromEntries(new FormData(e.currentTarget));
                void send({
                  ...f,
                  entity: "course",
                  id: x.id,
                  expectedVersion: Number(x.version),
                },e.currentTarget);
              }}
            >
              <h2>{x.status==='published'?'Retire':'Review'} · {String(x.title)}</h2>
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="action">
                  {x.status==='draft'&&courseNew&&<option value="publish">Publish</option>}
                  {x.status==='draft'&&courseSettle&&<option value="reject">Reject</option>}
                  {x.status==='published'&&courseSettle&&<option value="retire">Retire</option>}
                </Select>
              </Field>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" required minLength={3} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={disabled}>
                Record review
              </Button>
            </FormLayout>
          ))}
      {section!=="courses" && data?.canReview && (assignNew||open.assignment) && (
        (!recordOnly && <FormDisclosure appearance="standard" open={open.assignment} onToggle={event=>{const expanded=event.currentTarget.open;setOpen(value=>({...value,assignment:expanded}));}} onDiscard={()=>discard("assignment")} dismissDisabled={locked} error={error} drawerTitle="Assign course"><summary>Assign course</summary><FormLayout key={generation.assignment}
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget)),
              course = data.courses.find((x) => x.id === f.courseId);
            void send({ ...f, courseVersion: Number(course?.version) },e.currentTarget);
          }}
        >
          <h2>Assign course</h2>
          <Field appearance="standard">
            Course
            <Select appearance="standard" name="courseId">
              {data.courses
                .filter((x) => x.status === "published")
                .map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.title)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Learner
            <Select appearance="standard" name="learnerId">
              {data.people.map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Due
            <TextInput appearance="standard" name="dueDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={disabled||!assignNew}>
            Assign course
          </Button>
        </FormLayout></FormDisclosure>)
      )}
      {a && a.status === "assigned" && own&&assignmentSettle && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send({
              id: a.id,
              action: "start",
              expectedVersion: Number(a.version),
              reason: String(new FormData(e.currentTarget).get("reason")),
            },e.currentTarget);
          }}
        >
          <h2>Start course</h2>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={disabled}>
            Start
          </Button>
        </FormLayout>)
      )}
      {a && a.status === "in_progress" && own&&(resultSettle||open.result) && (
        (!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" open={open.result} onToggle={event=>{const expanded=event.currentTarget.open;setOpen(value=>({...value,result:expanded}));}} onDiscard={()=>discard("result")} error={error} dismissDisabled={locked}><summary>Record attendance and result</summary><FormLayout key={generation.result}
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            void send({
              ...f,
              id: a.id,
              action: "complete",
              expectedVersion: Number(a.version),
              scoreBps: Number(f.scoreBps),
              ...(f.certificateValidFrom?{certificateValidFrom:f.certificateValidFrom}:{}),
              certificateExpiresOn:f.certificateExpiresOn||null,
              attachmentSetIds:
                completionFiles?.setId && completionFiles.readyCount > 0 && completionFiles.bindable
                  ? [completionFiles.setId]
                  : [],
            },e.currentTarget);
          }}
        >
          <h2>Record attendance and result</h2>
          <Field appearance="standard">
            Attendance
            <Select appearance="standard" name="attendance">
              <option value="attended">Attended</option>
              <option value="absent">Absent</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Score bps
            <TextInput appearance="standard" name="scoreBps" type="number" min="0" max="10000" required />
          </Field>
          <Field appearance="standard">
            Evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} />
          </Field>
          <Field appearance="standard">Certificate valid from<TextInput appearance="standard" name="certificateValidFrom" type="date"/></Field>
          <Field appearance="standard">Certificate expiry, when applicable<TextInput appearance="standard" name="certificateExpiresOn" type="date"/></Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <AttachmentPanel appearance="standard"
            base={base}
            targetType="learning_assignment"
            targetId={String(a.id)}
            expectedTargetVersion={Number(a.version)}
            componentKind="action"
            componentRef="completion"
            purposeKey="certificate"
            title="Certificate or completion evidence"
            description="Attach the provider certificate or other completion document. Clean files are locked with this result."
            visibility="Only the learner and authorized Learning document managers can access these files."
            onChange={setCompletionFiles}
          />
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={disabled||!resultSettle}>
            Complete course
          </Button>
        </FormLayout></FormDisclosure>)
      )}
      {a && ["assigned", "in_progress"].includes(String(a.status)) && source.canReview&&assignmentSettle && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            void send({ ...f, id: a.id, expectedVersion: Number(a.version) },e.currentTarget);
          }}
        >
          <h2>Reviewer exception</h2>
          <Field appearance="standard">
            Action
            <Select appearance="standard" name="action">
              {resultSettle&&<option value="waive">Waive</option>}
              <option value="cancel">Cancel</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={disabled}>
            Record exception
          </Button>
        </FormLayout>)
      )}
    </div>
  );
}
