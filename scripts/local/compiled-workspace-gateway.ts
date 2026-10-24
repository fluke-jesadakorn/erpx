import { createServer, request, type IncomingMessage, type ServerResponse } from 'node:http';
import { PassThrough, type Duplex } from 'node:stream';
import { hasPortListener } from './port-listeners';
import { appIds } from '@erpx/contracts/apps';

export type CompiledGatewayRouting = { schema: 'erpx-compiled-routing/v1'; hosts: Record<string,string>; ports: Record<string,number> };
/** External release orchestration owns processes. This transport never starts jobs or replays requests. */
export function validateCompiledRouting(value: CompiledGatewayRouting) {
  if (value?.schema !== 'erpx-compiled-routing/v1' || !value.hosts || !value.ports || JSON.stringify(Object.keys(value.ports).sort()) !== JSON.stringify([...appIds].sort())) throw new Error('EXACT_COMPILED_ROUTING_REQUIRED');
  const ports = new Set<number>(), routed = new Set<string>();
  for (const port of Object.values(value.ports)) {
    if (!Number.isInteger(port) || port < 3200 || port > 3299 || ports.has(port)) throw new Error('INVALID_COMPILED_APP_PORT');
    ports.add(port);
  }
  for (const [host,id] of Object.entries(value.hosts)) {
    if (!/^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?(?::[0-9]{1,5})?$/.test(host) || new URL('http://'+host).host !== host || !appIds.includes(id as typeof appIds[number])) throw new Error('INVALID_COMPILED_HOST');
    routed.add(id);
  }
  if (routed.size !== appIds.length) throw new Error('EXACT_COMPILED_ROUTING_REQUIRED');
  return { ...value, hosts: Object.freeze({...value.hosts}), ports: Object.freeze({...value.ports}) };
}
function unavailable(response: ServerResponse) {
  if (response.headersSent) {response.destroy();return;}
  response.writeHead(503, {'Cache-Control':'no-store','Content-Type':'text/plain; charset=utf-8'});response.end('Application unavailable.');
}
export async function startCompiledWorkspaceGateway(routing: CompiledGatewayRouting, port: number) {
  const config = validateCompiledRouting(routing);
  if (!Number.isInteger(port) || port < 1024 || port > 65535 || [3000,3001,8080,...Object.values(config.ports)].includes(port)) throw new Error('EXPLICIT_GATEWAY_PORT_REQUIRED');
  if(hasPortListener(port))throw new Error('GATEWAY_PORT_OCCUPIED');
  const sockets = new Set<Duplex>();
  const target = (req: IncomingMessage) => config.ports[config.hosts[req.headers.host??'']];
  const server = createServer((req,res) => {
    const selected = target(req);
    if (!selected) {res.writeHead(421,{'Cache-Control':'no-store'});res.end();return;}
    // Host/Origin, cookie headers, path/query and multipart/RSC/SSE bodies pass unchanged.
    const upstream=request({host:'127.0.0.1',port:selected,path:req.url,method:req.method,headers:req.headers},response=>{
      res.writeHead(response.statusCode??502,response.statusMessage,response.headers);response.pipe(res);
      response.once('error',()=>unavailable(res));
    });
    req.once('aborted',()=>upstream.destroy());upstream.once('error',()=>unavailable(res));
    res.once('close',()=>{if(!res.writableFinished)upstream.destroy();});req.pipe(upstream);
  });
  server.on('upgrade',(req,downstream,head)=>{
    const selected=target(req);
    if(!selected){downstream.end('HTTP/1.1 421 Misdirected Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n');return;}
    const waiting=new PassThrough({highWaterMark:64*1024});
    downstream.once('error',()=>downstream.destroy());downstream.once('end',()=>downstream.destroy());
    downstream.once('close',()=>waiting.destroy());waiting.once('error',()=>downstream.destroy());downstream.pipe(waiting);
    const upstream=request({host:'127.0.0.1',port:selected,path:req.url,method:req.method,headers:req.headers});
    downstream.once('close',()=>upstream.destroy());
    upstream.once('upgrade',(response,socket,upstreamHead)=>{
      if(downstream.destroyed){socket.destroy();return;}
      sockets.add(downstream);downstream.once('close',()=>{sockets.delete(downstream);socket.destroy();});socket.once('close',()=>downstream.destroy());
      downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n`+response.rawHeaders.reduce((text,value,index,values)=>index%2?text:text+value+': '+values[index+1]+'\r\n','')+'\r\n');
      if(upstreamHead.length)downstream.write(upstreamHead);if(head.length)socket.write(head);
      socket.pipe(downstream);waiting.pipe(socket);socket.once('error',()=>downstream.destroy());downstream.once('error',()=>socket.destroy());
    });
    upstream.once('response',response=>{downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\nConnection: close\r\n\r\n`);response.pipe(downstream);response.once('error',()=>downstream.destroy());});
    upstream.once('error',()=>downstream.destroy());upstream.end();
  });
  await new Promise<void>((done,reject)=>{server.once('error',reject);server.listen(port,'127.0.0.1',()=>{server.off('error',reject);done();});});
  return {server,close:async()=>{for(const socket of sockets)socket.end();await new Promise<void>((done,reject)=>server.close(error=>error?reject(error):done()));}};
}
