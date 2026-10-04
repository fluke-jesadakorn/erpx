"use client";

import { useState, type FormEvent } from "react";
import { Button, Field, Select, TextArea, TextInput } from "./foundation";
import { Dialog } from "./dialog";

export function UiContractDemo() {
  const [open, setOpen] = useState(false);
  const [confirmation, setConfirmation] = useState(false);
  const [notes, setNotes] = useState("");
  const [result, setResult] = useState("");
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const submitter = (event.nativeEvent as SubmitEvent).submitter;
    const values = new FormData(event.currentTarget, submitter);
    setResult(
      `${values.get("intent")} · ${values.get("title")} · ${values.get("team")} · ${values.get("notes")}`,
    );
  }
  return (
    <section className="grid gap-4" aria-label="Interaction contracts">
      <h2>Forms and dialogs</h2>
      <form className="grid gap-4" onSubmit={submit}>
        <Field appearance="standard" label="Contract title">
          <TextInput appearance="standard" name="title" defaultValue="ภาษาไทย" required />
        </Field>
        <Field appearance="standard" label="Contract team">
          <Select appearance="standard" name="team" defaultValue="finance">
            <option value="finance">Finance</option>
            <option value="people">People</option>
          </Select>
        </Field>
        <Field appearance="standard" label="Contract notes">
          <TextArea appearance="standard" name="notes" defaultValue="ส่งต่อข้อมูล" />
        </Field>
        <div className="flex flex-wrap gap-2">
          <Button appearance="standard" type="submit" name="intent" value="submit">
            Submit contract form
          </Button>
          <Button appearance="standard" type="button" variant="secondary" onClick={() => setResult("Action only")}>
            Non-submit action
          </Button>
        </div>
      </form>
      <p role="status" data-testid="ui-form-result">
        {result}
      </p>
      <Button appearance="standard" variant="secondary" onClick={() => setOpen(true)}>
        Open draft dialog
      </Button>
      <Dialog appearance="standard"
        open={open}
        title="Draft dialog"
        onClose={() => (notes ? setConfirmation(true) : setOpen(false))}
      >
        <Field appearance="standard" label="Dialog draft">
          <TextInput appearance="standard"
            data-autofocus
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
          />
        </Field>
        <Dialog appearance="standard" open={confirmation} title="Discard changes?" onClose={() => setConfirmation(false)}>
          <p>Your draft has unsaved changes.</p>
          <div className="flex gap-2">
            <Button appearance="standard" variant="secondary" onClick={() => setConfirmation(false)}>
              Keep editing
            </Button>
            <Button appearance="standard"
              variant="danger"
              onClick={() => {
                setConfirmation(false);
                setNotes("");
                setOpen(false);
              }}
            >
              Discard draft
            </Button>
          </div>
        </Dialog>
      </Dialog>
    </section>
  );
}
