/** Keep a freshly checked replay valid across duplicate portal capture paths.
 * A new native submit always requires a new check, including after validation
 * prevents requestSubmit from emitting an event. */
export function verifiedFormSubmissions<Form extends object, NativeEvent extends object>() {
  const replays = new WeakSet<Form>(), verified = new WeakSet<NativeEvent>();
  return {
    accepts(form: Form, event: NativeEvent) {
      if (verified.has(event)) return true;
      if (!replays.delete(form)) return false;
      verified.add(event); return true;
    },
    replay(form: Form, submit: () => void) {
      replays.add(form);
      try { submit(); } finally { replays.delete(form); }
    },
  };
}
