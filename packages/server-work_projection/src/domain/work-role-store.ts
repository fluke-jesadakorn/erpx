export type ActiveWorkSelection = { session_id:string;user_id:string;company_id:string;assignment_id:string|null;assignment_version:number|null;revision:number;expires_at:number };
export type WorkAuthority = { sessionId:string;revision:number;assignmentVersion:number };
export type SelectWorkRole={sessionId:string;userId:string;companyId:string;assignmentId:string;assignmentVersion:number;revision:number;expiresAt:number;restoring?:boolean;rememberPreference?:boolean};
