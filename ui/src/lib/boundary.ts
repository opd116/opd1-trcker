// The error boundaries' retry (App.svelte, GlanceShell.svelte): a crashed part tries again on the next update of the
// data it shows, the likeliest cause being one bad payload.
import { untrack } from "svelte";
import type { Attachment } from "svelte/attachments";
import type { Readable } from "svelte/store";

/** An attachment for a boundary's failed snippet: `reset` on the next update of `source`. */
export const retryOnNextUpdate =
  (reset: () => void, source: Readable<unknown>): Attachment =>
  () => {
    /** The subscription's own first call is the state that crashed; only a later one is an update. */
    let first = true;
    return source.subscribe(() => {
      if (first) first = false;
      else reset();
    });
  };

/** An attachment for a boundary's failed snippet: `reset` once `read()` (reactive state) differs from its value now. */
export function retryOnChange(reset: () => void, read: () => unknown): Attachment {
  // Read untracked here: a tracked read would make Svelte re-create the attachment, with the new value, on a change.
  const at = untrack(read);
  return () => {
    if (read() !== at) reset();
  };
}
