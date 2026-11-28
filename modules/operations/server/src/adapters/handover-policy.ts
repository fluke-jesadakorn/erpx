import { finalRootOwnerProjectionSources as collaborationProjectionSources } from "@erpx/collaboration-server/public/final-root-owner-projection-sources";
import {projectTaskOwnerSql,projectTaskMemberSql,projectHandoverSourceSql} from "@erpx/projects-server/public/project-read-projections";
import {participantSql} from '@erpx/workflow-server/public/approval-participant-projection';
import {approverJsonExistsSql} from './task-approver-access-source';
import {operationsSocialTaskOriginExists} from './social-task-origin-exists';
import {operationsProjectSourceExists} from './project-source-exists';
import {operationsActiveDelegationSql} from './task-delegation-source';
import {operationsStaffTaskExists} from './operations-staff-task-exists';

/** Same task-participant categories used by task reads, with an SQL user expression. */
export function handoverTaskAccessSql(task: string, user: string) {
  return `(iam_same_person(${task}.creator_user_id,${user},${task}.company_id)
    OR iam_same_person(${task}.assignee_user_id,${user},${task}.company_id)
    OR EXISTS(${operationsActiveDelegationSql('1','hd',`${task}.id`,`${task}.company_id`,'floor(extract(epoch FROM clock_timestamp())*1000)::bigint',`\n      AND iam_same_person(hd.delegate_user_id,${user},${task}.company_id)`,' ','\n      AND ')})
    OR ${projectTaskOwnerSql(task,user)}
    OR ${projectTaskMemberSql(task,user)}
    OR ${approverJsonExistsSql(task,'start',user)}
    OR ${approverJsonExistsSql(task,'completion',user)}
    OR ${participantSql("task",task,user)})`;
}

/** Handover text is hidden as soon as any linked source is no longer readable. */
export function handoverSourcesReadableSql(post: string, user: string) {
  return `(${post}.purpose<>'handover' OR NOT ${operationsProjectSourceExists(collaborationProjectionSources.social_post_sources,'hs',`hs.post_id=${post}.id AND (
      (hs.source_kind='staff_task' AND NOT ${operationsStaffTaskExists('ht','hs.source_id',`ht.company_id=${post}.company_id
          AND NOT ${operationsSocialTaskOriginExists('ho','ht.id')}
          AND ${handoverTaskAccessSql("ht", user)}`,'\n        ')})
      OR (hs.source_kind='staff_project' AND NOT EXISTS(${projectHandoverSourceSql(post, user)}))
    )`,' ','\n    ')})`;
}
