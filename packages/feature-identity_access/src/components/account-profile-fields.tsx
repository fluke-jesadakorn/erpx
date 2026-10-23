import { Button, Field, InlineNotice, Select, TextInput } from '@erpx/ui/components/foundation';

// Presentation stays below the stable client controller. UI-only edits do not
// replace the Account draft, cancellation or optimistic-version witnesses.
type Draft = {name:string;language:'en'|'th';appearance:'system'|'light'|'dark'};
type Props = {th:boolean;draft:Draft;email:string;hydrated:boolean;pending:boolean;invalidated:boolean;dirty:boolean;error:string;saved:boolean;onEdit(patch:Partial<Draft>):void;onLoadSaved():void};
export function AccountProfileFields({th,draft,email,hydrated,pending,invalidated,dirty,error,saved,onEdit,onLoadSaved}:Props) {
  return <>
    <Field appearance="standard" label={th ? "ชื่อที่แสดง" : "Display name"} required>
      <TextInput appearance="standard" autoComplete="name" required maxLength={160} value={draft.name} disabled={!hydrated || pending || invalidated} onChange={event => onEdit({ name: event.target.value })} />
    </Field>
    <Field appearance="standard" label={th ? "อีเมล" : "Email"}><TextInput appearance="standard" type="email" autoComplete="email" readOnly value={email} /></Field>
    <Field appearance="standard" label={th ? "ภาษา" : "Language"}>
      <Select appearance="standard" value={draft.language} disabled={!hydrated || pending || invalidated} onChange={event => onEdit({ language: event.target.value as "en" | "th" })}>
        <option value="en">English</option><option value="th">ไทย</option>
      </Select>
    </Field>
    <Field appearance="standard" label={th ? "รูปแบบสี" : "Theme"}>
      <Select appearance="standard" value={draft.appearance} disabled={!hydrated || pending || invalidated} onChange={event => onEdit({ appearance: event.target.value as "system" | "light" | "dark" })}>
        <option value="system">{th ? "ตามอุปกรณ์" : "System"}</option><option value="light">{th ? "สว่าง" : "Light"}</option><option value="dark">{th ? "มืด" : "Dark"}</option>
      </Select>
    </Field>
    {error && <InlineNotice tone="danger" role="alert">{error}{" "}<Button appearance="standard" variant="plain" type="button" disabled={pending || invalidated} onClick={onLoadSaved}>{th ? "โหลดค่าที่บันทึกไว้" : "Load saved settings"}</Button></InlineNotice>}
    {saved && <InlineNotice tone="success">{th ? "บันทึกการตั้งค่าบัญชีแล้ว" : "Account settings saved."}</InlineNotice>}
    {invalidated && <InlineNotice tone="warning">{th ? "บัญชีหรือเซสชันเปลี่ยน โปรดโหลดหน้าใหม่ก่อนทำต่อ" : "Your account or session changed. Reload before continuing."}</InlineNotice>}
    <div><Button appearance="standard" type="submit" clientManaged pending={pending} disabled={invalidated || !dirty}>{th ? "บันทึกการตั้งค่า" : "Save account settings"}</Button></div>
  </>;
}
