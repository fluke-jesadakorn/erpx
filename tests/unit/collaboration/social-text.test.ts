import { describe,it,expect } from "vitest";
import { activeToken,editMentions,hashtags,type Mention } from "../../../src/server/modules/collaboration/public/rules/social-text";
const m:Mention={kind:"person",targetId:"a",label:"Ann",start:3,end:7};
describe("social inline text",()=>{
 it("recognizes a token at the caret, not email addresses",()=>{
  expect(activeToken("Hi @ann later",7)).toEqual({trigger:"@",query:"ann",start:3,end:7});
  expect(activeToken("mail@example",12)).toBeNull();
  expect(activeToken("@",1)?.query).toBe("");
 });
 it("preserves identity across prefix edits and drops edited mentions",()=>{
  expect(editMentions("Hi @Ann","Hello @Ann",[m])[0]).toEqual({...m,start:6,end:10});
  expect(editMentions("Hi @Ann","Hi @Ana",[m])).toEqual([]);
  expect(editMentions("Hi @Ann","Hi ",[m])).toEqual([]);
  expect(editMentions("Hi @Ann","Hi @Anna",[m])).toEqual([]);
  expect(editMentions("Hi @Ann","Hi @Ann!",[m])).toEqual([m]);
 });
 it("keeps duplicate display names distinct and handles pasted emoji",()=>{
  const second={...m,targetId:"b",start:8,end:12};
  expect(editMentions("Hi @Ann @Ann","😀 Hi @Ann @Ann",[m,second]).map(x=>[x.targetId,x.start])).toEqual([["a",6],["b",11]]);
 });
 it("recognizes Thai/English tags, normalizes case, ignores URLs and oversized tags",()=>{
  expect(hashtags("#ทีมงาน #Release #ทีมงาน https://a/#fragment").map(t=>t.tag)).toEqual(["ทีมงาน","release","ทีมงาน"]);
  expect(hashtags("#"+"a".repeat(41))).toEqual([]);
  expect(hashtags("#cafe\u0301")[0].tag).toBe("café");
 });
});
