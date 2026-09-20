import { afterEach, expect, it, vi } from "vitest";
import { createRequestDraftOnce, createRequestUploader } from "../src/lib/request-draft-create";
afterEach(() => vi.unstubAllGlobals());
it("recovers the exact draft across a lost response and reload before uploading", async () => {
 const stored = new Map<string,string>();
 vi.stubGlobal("sessionStorage",{getItem:(k:string)=>stored.get(k)??null,setItem:(k:string,v:string)=>stored.set(k,v),removeItem:(k:string)=>stored.delete(k)});
 const fetcher = vi.fn().mockRejectedValueOnce(Error("Lost response")).mockResolvedValueOnce(Response.json({data:{id:"same"}})); vi.stubGlobal("fetch",fetcher);
 await expect(createRequestDraftOnce()("/holdings/acme","request:new",{title:"Keep my input"})).rejects.toThrow();
 const create = createRequestDraftOnce();
 expect(await create("/holdings/acme","request:new",{title:"Changed"})).toBe("same");
 expect(await create("/holdings/acme","request:new",{})).toBe("same");
 expect(fetcher).toHaveBeenCalledTimes(2);
 expect(fetcher.mock.calls[0][1]).toEqual(fetcher.mock.calls[1][1]);
});
it("retries an upload without another intent or completed file", async () => {
 const fetcher=vi.fn().mockResolvedValueOnce(Response.json({data:{uploadUrl:"/upload/one"}})).mockRejectedValueOnce(Error("Interrupted")).mockResolvedValueOnce(Response.json({data:{id:"one"}}));vi.stubGlobal("fetch",fetcher);
 const upload=createRequestUploader(),file=new File(["synthetic"],"file.png",{type:"image/png"});
 await expect(upload("/holdings/acme","draft",file)).rejects.toThrow();
 await upload("/holdings/acme","draft",file); await upload("/holdings/acme","draft",file);
 expect(fetcher).toHaveBeenCalledTimes(3);
 expect(fetcher.mock.calls[1][0]).toBe(fetcher.mock.calls[2][0]);
});
