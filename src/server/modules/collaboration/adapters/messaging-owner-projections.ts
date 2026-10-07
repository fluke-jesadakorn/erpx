export const messagingTeamMemberPredicate='AND EXISTS(SELECT 1 FROM messaging_team_members tm WHERE tm.team_id=? AND tm.user_id=a.user_id)';
