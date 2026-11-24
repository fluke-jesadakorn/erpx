/** Collaboration read-tail fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const socialTaskOriginPostExists=`EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)`;
export const messagingRoomMembersAgg=`(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_room_members m WHERE m.room_id=r.id)`;
export const socialFollowPostExists=`EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)`;
export const messagingTeamMemberExists=`EXISTS(SELECT 1 FROM messaging_team_members m WHERE m.team_id=t.id AND m.user_id=?)`;
export const legacyNativeUnion=`(SELECT * FROM legacy_rows UNION ALL SELECT * FROM native_rows)`;
