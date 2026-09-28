import { participantSql } from "./approvals";

/** Same task-participant categories used by task reads, with an SQL user expression. */
export function handoverTaskAccessSql(task: string, user: string) {
  return `(iam_same_person(${task}.creator_user_id,${user},${task}.company_id)
    OR iam_same_person(${task}.assignee_user_id,${user},${task}.company_id)
    OR EXISTS(SELECT 1 FROM staff_task_delegations hd WHERE hd.task_id=${task}.id
      AND hd.company_id=${task}.company_id AND hd.status='active'
      AND hd.starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
      AND hd.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
      AND iam_same_person(hd.delegate_user_id,${user},${task}.company_id))
    OR EXISTS(SELECT 1 FROM staff_projects hp WHERE hp.id=${task}.project_id
      AND hp.company_id=${task}.company_id AND iam_same_person(hp.owner_user_id,${user},${task}.company_id))
    OR EXISTS(SELECT 1 FROM staff_projects hpm JOIN staff_project_members hm ON hm.project_id=hpm.id
      WHERE hpm.id=${task}.project_id AND hpm.company_id=${task}.company_id AND hm.user_id=${user})
    OR EXISTS(SELECT 1 FROM erpx_json_each(${task}.start_approvers_json) WHERE value=${user})
    OR EXISTS(SELECT 1 FROM erpx_json_each(${task}.completion_approvers_json) WHERE value=${user})
    OR ${participantSql("task",task,user)})`;
}

/** Handover text is hidden as soon as any linked source is no longer readable. */
export function handoverSourcesReadableSql(post: string, user: string) {
  return `(${post}.purpose<>'handover' OR NOT EXISTS(
    SELECT 1 FROM social_post_sources hs WHERE hs.post_id=${post}.id AND (
      (hs.source_kind='staff_task' AND NOT EXISTS(
        SELECT 1 FROM staff_tasks ht WHERE ht.id=hs.source_id AND ht.company_id=${post}.company_id
          AND NOT EXISTS(SELECT 1 FROM social_task_origins ho WHERE ho.task_id=ht.id)
          AND ${handoverTaskAccessSql("ht",user)}))
      OR (hs.source_kind='staff_project' AND NOT EXISTS(
        SELECT 1 FROM staff_projects hp WHERE hp.id=hs.source_id AND hp.company_id=${post}.company_id
          AND (iam_same_person(hp.owner_user_id,${user},hp.company_id)
            OR EXISTS(SELECT 1 FROM staff_project_members hm WHERE hm.project_id=hp.id AND hm.user_id=${user}))))
    )))`;
}
