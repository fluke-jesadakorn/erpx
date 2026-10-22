/** Render-only destinations; no identity, grant or work-authority snapshot. */
export function accountSelfNavigation(workspaceBase:string,userId:string,central=false){
 const companyProfile=workspaceBase==='/platform'?null:`${workspaceBase}/profiles/${encodeURIComponent(userId)}`;
 return {access:central?'/my-access':`${workspaceBase}/iam/my-access`,profile:central?'/my-profile':companyProfile,companyProfile:central?companyProfile:null};
}
