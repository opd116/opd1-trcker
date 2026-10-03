// R163 (I2): a failed `img://` request is asked again. Rust answers 503 while a valorant-api download fails, and
// WebView2 never re-requests an `<img>` whose `src` did not change, so without this a tier icon, agent head, map or
// card art stays blank until the app restarts. Bounded (3 retries on a 2s, 5s, 15s backoff); each retry adds `?r=n`
// to the URL (Rust's `imgproto::parse_path` reads the path only, so the query changes nothing there but the browser
// treats it as a new URL). No layout, no text: the image simply appears when it finally loads.
import type { Attachment } from "svelte/attachments";

/** Wait before retry 1, 2 and 3. */
export const IMG_RETRY_MS = [2_000, 5_000, 15_000];

const RETRY_PARAM = /[?&]r=\d+$/;
/** The URL without a `?r=n` this module added. */
const baseOf = (url: string): string => url.replace(RETRY_PARAM, "");
const withRetry = (base: string, n: number): string => `${base}${base.includes("?") ? "&" : "?"}r=${n}`;

/**
 * `<img {@attach retryImg} src=…>`: on `error`, sets `src` to the same URL plus `?r=n` after the next backoff. A
 * `load` starts the count over; when Svelte gives the element another `src`, the pending retry is dropped and the
 * new URL starts from the first delay.
 */
export const retryImg: Attachment<HTMLImageElement> = (img) => {
  let base: string | null = null;
  let tries = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const onerror = () => {
    const current = baseOf(img.getAttribute("src") ?? "");
    if (current !== base) {
      base = current;
      tries = 0;
    }
    if (tries >= IMG_RETRY_MS.length) return;
    const n = ++tries;
    clearTimeout(timer);
    timer = setTimeout(() => {
      // Another src arrived while waiting: that one is Svelte's, leave it alone.
      if (baseOf(img.getAttribute("src") ?? "") !== base) return;
      img.src = withRetry(base!, n);
    }, IMG_RETRY_MS[n - 1]);
  };
  const onload = () => {
    tries = 0;
  };

  img.addEventListener("error", onerror);
  img.addEventListener("load", onload);
  return () => {
    clearTimeout(timer);
    img.removeEventListener("error", onerror);
    img.removeEventListener("load", onload);
  };
};

/**
 * `<div {@attach retryBg(url)}>`: a `background-image` that retries like `retryImg`. The image is preloaded with an
 * `Image`; the background is set once it has loaded (a failed `background-image` has no `error` event to listen to).
 * `url` changing re-runs the attachment, whose cleanup cancels the old load and clears the old background.
 */
export function retryBg(url: string, make: () => HTMLImageElement = () => new Image()): Attachment<HTMLElement> {
  return (node) => {
    let tries = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let img: HTMLImageElement | undefined;

    const attempt = () => {
      const src = tries === 0 ? url : withRetry(url, tries);
      const probe = make();
      img = probe;
      probe.onload = () => {
        node.style.backgroundImage = `url("${src}")`;
      };
      probe.onerror = () => {
        if (tries >= IMG_RETRY_MS.length) return;
        timer = setTimeout(() => {
          tries++;
          attempt();
        }, IMG_RETRY_MS[tries]);
      };
      probe.src = src;
    };
    attempt();

    return () => {
      clearTimeout(timer);
      if (img) img.onload = img.onerror = null;
      node.style.backgroundImage = "";
    };
  };
}
