import { describe, expect, it, vi } from "vitest";
import { createLatestRead } from "@erpx/feature-shared/lib/latest-read";
import {createPosSeedReadGate,isLivePosTab,posReadKey} from "@erpx/feature-pos/lib/pos-read-identity";

describe("POS background reads", () => {
  it("reuses the server read, including repeated mount effects, but reloads changed or revisited filters",()=>{
    const initial=posReadKey("/holdings/acme","branch:actor",{tab:"history",q:"receipt",cursor:"page2",sale:"sale1"});
    const gate=createPosSeedReadGate(initial);
    expect(gate.shouldRead(initial)).toBe(false);
    expect(gate.shouldRead(initial)).toBe(false);
    const changed=posReadKey("/holdings/acme","branch:actor",{tab:"history",q:"different",cursor:"page2",sale:"sale1"});
    expect(gate.shouldRead(changed)).toBe(true);
    expect(gate.shouldRead(initial)).toBe(true);
    expect(createPosSeedReadGate(null).shouldRead(initial)).toBe(true);
  });
  it("fences finance, workspace, actor, receipt and pagination, and polls only operational tabs",()=>{
    const seed={tab:"sale",q:"",sale:"sale1",cursor:""};
    const key=posReadKey("company-a","actor-a",seed);
    for(const [base,scope,filters] of [["company-b","actor-a",seed],["company-a","actor-b",seed],["company-a","actor-a",{...seed,finance:true}],["company-a","actor-a",{...seed,sale:"sale2"}],["company-a","actor-a",{...seed,cursor:"page2"}]] as const)
      expect(posReadKey(base,scope,filters)).not.toBe(key);
    expect(["sale","kitchen","shifts"].every(isLivePosTab)).toBe(true);
    expect(["history","reports","settings","catalog"].some(isLivePosTab)).toBe(false);
  });
  it("publishes a slow read even when polling requests the same data repeatedly", async () => {
    const reads = createLatestRead<string>();
    let finish!: (value: string) => void;
    const load = vi.fn(() => new Promise<string>(resolve => { finish = resolve; }));
    const publish = vi.fn();
    const initial = reads.read("sale", load, publish);
    for (let poll = 0; poll < 5; poll++) expect(reads.read("sale", load, publish)).toBe(initial);
    expect(load).toHaveBeenCalledTimes(1);
    finish("loaded");
    await initial;
    expect(publish).toHaveBeenCalledWith("loaded");
  });

  it("discards an obsolete tab response and lets the current tab finish", async () => {
    const reads = createLatestRead<string>();
    let finishOld!: (value: string) => void;
    const publishOld = vi.fn(), publishCurrent = vi.fn();
    const old = reads.read("sale", () => new Promise(resolve => { finishOld = resolve; }), publishOld);
    await reads.read("kitchen", async () => "current", publishCurrent);
    finishOld("obsolete");
    await old;
    expect(publishOld).not.toHaveBeenCalled();
    expect(publishCurrent).toHaveBeenCalledWith("current");
  });

  it("times out a stuck fetch and permits an explicit retry", async () => {
    const reads = createLatestRead<string>(15);
    const pending = reads.read("sale", signal => new Promise((_resolve, reject) => {
      signal.addEventListener("abort", () => reject(signal.reason), { once: true });
    }), vi.fn());
    await expect(pending).rejects.toMatchObject({ name: "TimeoutError" });
    const publish = vi.fn();
    await reads.read("sale", async () => "retry loaded", publish);
    expect(publish).toHaveBeenCalledWith("retry loaded");
  });

  it("does not reuse a pre-save poll for the same endpoint after a command commits", async () => {
    const reads = createLatestRead<string>();
    let finishPoll!: (value: string) => void;
    const publish = vi.fn();
    const poll = reads.read("receipt", () => new Promise(resolve => { finishPoll = resolve; }), publish);
    reads.cancel();
    await reads.read("receipt", async () => "committed receipt", publish);
    finishPoll("receipt before save");
    await poll;
    expect(publish.mock.calls).toEqual([["committed receipt"]]);
  });

  it("rejects an expired result when body decoding ignores the abort signal", async () => {
    const reads = createLatestRead<string>(15);
    let finish!: (value: string) => void;
    let deadline!: Promise<void>;
    const publish = vi.fn();
    const pending = reads.read("receipt", signal => {
      deadline = new Promise(resolve => signal.addEventListener("abort", () => resolve(), { once: true }));
      return new Promise(resolve => { finish = resolve; });
    }, publish);
    await deadline;
    finish("expired receipt");
    await expect(pending).rejects.toMatchObject({ name: "TimeoutError" });
    expect(publish).not.toHaveBeenCalled();
    await reads.read("receipt", async () => "fresh receipt", publish);
    expect(publish).toHaveBeenCalledWith("fresh receipt");
  });
});
