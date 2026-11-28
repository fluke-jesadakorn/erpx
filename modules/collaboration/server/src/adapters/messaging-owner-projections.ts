import {messagingTeamMemberGuard} from './read-tail-sql';
export const messagingTeamMemberPredicate=`AND ${messagingTeamMemberGuard('tm','?','a.user_id')}`;
