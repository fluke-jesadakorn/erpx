/** Work's existing Social transport. Administration actions remain outside it. */
export function workSocialBetaPathAllowed(path:readonly string[],method:string){
 if(path.some(value=>!value||value==='.'||value==='..'||value.length>160||/[\/\\\u0000-\u001f]/.test(value)||/%(?:2f|5c)/i.test(value)))return false;
 if(method==='GET'||method==='HEAD')return path.length===1&&['home','meta','suggestions'].includes(path[0]);
 if(method!=='POST')return false;
 if(path[0]==='drafts')return path.length===1||path.length===2||path.length===3&&path[2]==='publish';
 if(['comment-uploads','post-edit-uploads'].includes(path[0]))return path.length===1||path.length===2;
 return path[0]==='agent-replies'&&path.length===2;
}
