"use client";
import { useState } from "react";
import { Button } from "@erpx/ui/components/button";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import { TextInput } from "@erpx/ui/components/input";
import { Menu, MenuContent, MenuItem, MenuTrigger, Popover, PopoverContent, PopoverTrigger, Combobox, LocalTabs, TabsList, TabsTrigger, TabsPanel, Tooltip, TooltipProvider } from "@erpx/ui/components/popups";

export function UiStandardDemo() {
  const [value, setValue] = useState("finance"), [action, setAction] = useState(""), [open, setOpen] = useState(false), [draft, setDraft] = useState("");
  return <section className="ui-panel ui-stack" aria-label="Standard popup contracts">
    <h2>Standard popup contracts</h2>
    <div className="flex flex-wrap gap-3">
      <Menu><MenuTrigger render={<Button appearance="standard" variant="secondary" />}>Record actions</MenuTrigger><MenuContent><MenuItem onClick={() => setAction("Reviewed")}>Review record</MenuItem><MenuItem disabled>Unavailable action</MenuItem></MenuContent></Menu>
      <Popover><PopoverTrigger render={<Button appearance="standard" variant="secondary" />}>Record information</PopoverTrigger><PopoverContent><p>รายละเอียดภาษาไทยที่ยาวและยังคงอ่านได้ครบถ้วน</p></PopoverContent></Popover>
      <TooltipProvider><Tooltip content="Additional record context"><Button appearance="standard" variant="ghost">Record hint</Button></Tooltip></TooltipProvider>
      <Button appearance="standard" variant="secondary" onClick={() => setOpen(true)}>Open standard drawer</Button>
    </div>
    <p role="status" data-testid="standard-menu-result">{action}</p>
    <LocalTabs defaultValue="summary"><TabsList aria-label="Record sections"><TabsTrigger value="summary">Summary</TabsTrigger><TabsTrigger value="history">History</TabsTrigger></TabsList><TabsPanel value="summary">Record summary</TabsPanel><TabsPanel value="history">Record history</TabsPanel></LocalTabs>
    <DraftFormDrawer open={open} title="Standard editor" dirty={!!draft} onClose={() => setOpen(false)} onDiscard={() => {setDraft("");setOpen(false);}}>
      <div className="ui-stack">
        <TextInput appearance="standard" aria-label="Standard draft" value={draft} onChange={event=>setDraft(event.target.value)} />
        <Combobox label="Standard department" options={[{value:"finance",label:"Finance · ฝ่ายการเงิน"},{value:"people",label:"People · ฝ่ายบุคคล"}]} value={value} onValueChange={setValue} name="department" />
        <p data-testid="standard-department-value">{value}</p>
      </div>
    </DraftFormDrawer>
  </section>;
}
