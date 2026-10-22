import { startWorkspaceGateway } from "./workspace-gateway-runtime";
const root=process.env.ERPX_REPOSITORY_ROOT,node=process.env.ERPX_NODE_BIN;
if(!root||!node)throw new Error("PINNED_GATEWAY_RUNTIME_REQUIRED");
const gateway=await startWorkspaceGateway({root,node,port:3100,prewarm:(process.env.ERPX_DEV_PREWARM??"").split(",").filter(Boolean)});
process.on("SIGTERM",()=>{void gateway.close();});process.on("SIGINT",()=>{void gateway.close();});
console.log("ERPX development gateway: http://127.0.0.1:3100");
