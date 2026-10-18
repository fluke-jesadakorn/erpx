import {participantSql} from "../../workflow/public/approvals";
import {sourceAccessSql} from "../../collaboration/public/social-source";

/** SQL counterpart of the original task reader's participant and social-source
 * policy, for consumers that must apply source policy before their own LIMIT. */
function participantQuery(id:string,company:string,viewer:string){
 const user=`${viewer}.user_id`;
 const sql=`EXISTS(SELECT 1 FROM staff_tasks project_task_record LEFT JOIN staff_projects parent_project ON parent_project.id=project_task_record.project_id
 LEFT JOIN LATERAL (SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=project_task_record.id AND sd.company_id=project_task_record.company_id AND sd.status='active' AND sd.starts_at<=${viewer}.now_ms AND sd.expires_at>${viewer}.now_ms ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) delegation ON TRUE
 WHERE project_task_record.id=${id} AND project_task_record.company_id=${company} AND (
 iam_same_person(project_task_record.creator_user_id,${user},project_task_record.company_id) OR iam_same_person(project_task_record.assignee_user_id,${user},project_task_record.company_id)
 OR iam_same_person(delegation.delegate_user_id,${user},project_task_record.company_id) OR iam_same_person(parent_project.owner_user_id,${user},project_task_record.company_id)
 OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=parent_project.id AND m.user_id=${user})
 OR EXISTS(SELECT 1 FROM erpx_json_each(project_task_record.start_approvers_json) WHERE value=${user})
 OR EXISTS(SELECT 1 FROM erpx_json_each(project_task_record.completion_approvers_json) WHERE value=${user})
 OR ${participantSql('task','project_task_record',user)})
`;
 return sql;
}

/** Original native Task participant policy, without repeating the conversation
 * source gate. Consumers must compose its named current post-rights predicate. */
export function taskParticipantReadPolicy(id:string,company:string,viewer:string){return `${participantQuery(id,company,viewer)})`;}
export function projectTaskRecordPolicy(id:string,company:string,viewer:string){
 const user=`${viewer}.user_id`;
 return `${participantQuery(id,company,viewer)} AND NOT EXISTS(SELECT 1 FROM social_task_origins origin LEFT JOIN social_posts project_task_post ON project_task_post.id=origin.post_id WHERE origin.task_id=project_task_record.id AND (project_task_post.id IS NULL OR project_task_post.state<>'published' OR NOT (${sourceAccessSql('project_task_post',`${viewer}.assignment_id`,{userId:user,now:`${viewer}.now_ms`})}))))`;
}
