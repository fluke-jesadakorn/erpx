export function frontVisiblePath(holding:string,company:string,locationAndView:string[]=[]){return "/"+[holding,company,...locationAndView].map(encodeURIComponent).join("/");}
export function frontVisibleLocation(holding:string,company:string,location:string){return frontVisiblePath(holding,company,location.split("/").filter(Boolean))+"/";}
