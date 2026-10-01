// R163 (I2): an `img://` request that failed (Rust answers 503 while a download fails) is asked again, a bounded
// number of times, with `?r=n` added so nothing caches the miss. The attachments are written against the few DOM
// members they use, so these tests run in vitest's node environment with small fakes.
import { afterEach, describe, expect, it, vi } from "vitest";
import { IMG_RETRY_MS, retryBg, retryImg } from "./imgRetry";

afterEach(() => vi.useRealTimers());

/** The bits of an `<img>` the attachment touches. */
function fakeImg(src: string) {
  const handlers = new Map<string, () => void>();
  const img = {
    attrs: { src },
    src,
    getAttribute: (n: string) => (n === "src" ? img.attrs.src : null),
    addEventListener: (ev: string, cb: () => void) => handlers.set(ev, cb),
    removeEventListener: (ev: string) => handlers.delete(ev),
  };
  // Assigning `src` is what a retry does; mirror it into the attribute like a browser.
  Object.defineProperty(img, "src", {
    get: () => img.attrs.src,
    set: (v: string) => {
      img.attrs.src = v;
    },
  });
  return { img: img as unknown as HTMLImageElement, handlers, fire: (ev: string) => handlers.get(ev)?.(), srcNow: () => img.attrs.src };
}

describe("retryImg", () => {
  it("the backoff is 2s, 5s, 15s", () => {
    expect(IMG_RETRY_MS).toEqual([2_000, 5_000, 15_000]);
  });

  it("an error sets the same URL plus ?r=n after each delay, three times, then gives up", async () => {
    vi.useFakeTimers();
    const f = fakeImg("img://localhost/agent/a1");
    retryImg(f.img);
    f.fire("error");
    await vi.advanceTimersByTimeAsync(1_999);
    expect(f.srcNow()).toBe("img://localhost/agent/a1");
    await vi.advanceTimersByTimeAsync(1);
    expect(f.srcNow()).toBe("img://localhost/agent/a1?r=1");
    f.fire("error");
    await vi.advanceTimersByTimeAsync(5_000);
    expect(f.srcNow()).toBe("img://localhost/agent/a1?r=2");
    f.fire("error");
    await vi.advanceTimersByTimeAsync(15_000);
    expect(f.srcNow()).toBe("img://localhost/agent/a1?r=3");
    f.fire("error"); // out of retries
    await vi.advanceTimersByTimeAsync(60_000);
    expect(f.srcNow()).toBe("img://localhost/agent/a1?r=3");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("a URL that already has a query gets &r=n", async () => {
    vi.useFakeTimers();
    const f = fakeImg("http://x/a?b=1");
    retryImg(f.img);
    f.fire("error");
    await vi.advanceTimersByTimeAsync(2_000);
    expect(f.srcNow()).toBe("http://x/a?b=1&r=1");
  });

  it("a load resets the count, so a later failure starts again at 2s", async () => {
    vi.useFakeTimers();
    const f = fakeImg("img://localhost/tier/21");
    retryImg(f.img);
    f.fire("error");
    await vi.advanceTimersByTimeAsync(2_000);
    f.fire("load");
    f.fire("error");
    await vi.advanceTimersByTimeAsync(2_000);
    expect(f.srcNow()).toBe("img://localhost/tier/21?r=1");
  });

  it("when Svelte gives the element another src, the old retry is dropped and the new one starts from 2s", async () => {
    vi.useFakeTimers();
    const f = fakeImg("img://localhost/agent/a1");
    retryImg(f.img);
    f.fire("error");
    await vi.advanceTimersByTimeAsync(1_000);
    f.img.src = "img://localhost/agent/a2";
    await vi.advanceTimersByTimeAsync(1_000); // the pending retry for a1 must not overwrite a2
    expect(f.srcNow()).toBe("img://localhost/agent/a2");
    f.fire("error");
    await vi.advanceTimersByTimeAsync(2_000);
    expect(f.srcNow()).toBe("img://localhost/agent/a2?r=1");
  });

  it("the cleanup removes the listeners and cancels a pending retry", async () => {
    vi.useFakeTimers();
    const f = fakeImg("img://localhost/agent/a1");
    const off = retryImg(f.img) as () => void;
    f.fire("error");
    off();
    expect(f.handlers.size).toBe(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(f.srcNow()).toBe("img://localhost/agent/a1");
  });
});

/** An `Image` whose load the test decides. */
function fakeImage() {
  const made: { src: string; onload: (() => void) | null; onerror: (() => void) | null }[] = [];
  const make = () => {
    const i = { src: "", onload: null as (() => void) | null, onerror: null as (() => void) | null };
    made.push(i);
    return i as unknown as HTMLImageElement;
  };
  return { make, made };
}

describe("retryBg", () => {
  const node = () => ({ style: { backgroundImage: "" } }) as unknown as HTMLElement;

  it("sets the background only once the image has loaded", () => {
    const n = node();
    const f = fakeImage();
    retryBg("img://localhost/cardLarge/c1", f.make)(n);
    expect(f.made[0]!.src).toBe("img://localhost/cardLarge/c1");
    expect(n.style.backgroundImage).toBe("");
    f.made[0]!.onload!();
    expect(n.style.backgroundImage).toBe('url("img://localhost/cardLarge/c1")');
  });

  it("a failed preload is retried with ?r=n on the 2s, 5s, 15s backoff, and the retry that loads is the one shown", async () => {
    vi.useFakeTimers();
    const n = node();
    const f = fakeImage();
    retryBg("img://localhost/cardLarge/c1", f.make)(n);
    f.made[0]!.onerror!();
    await vi.advanceTimersByTimeAsync(2_000);
    expect(f.made[1]!.src).toBe("img://localhost/cardLarge/c1?r=1");
    f.made[1]!.onerror!();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(f.made[2]!.src).toBe("img://localhost/cardLarge/c1?r=2");
    f.made[2]!.onload!();
    expect(n.style.backgroundImage).toBe('url("img://localhost/cardLarge/c1?r=2")');
  });

  it("gives up after three retries", async () => {
    vi.useFakeTimers();
    const f = fakeImage();
    retryBg("u", f.make)(node());
    for (const wait of IMG_RETRY_MS) {
      f.made.at(-1)!.onerror!();
      await vi.advanceTimersByTimeAsync(wait);
    }
    f.made.at(-1)!.onerror!();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(f.made).toHaveLength(4);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("the cleanup cancels a pending retry, ignores a late load and clears the background", async () => {
    vi.useFakeTimers();
    const n = node();
    const f = fakeImage();
    const off = retryBg("u", f.make)(n) as () => void;
    f.made[0]!.onerror!();
    off();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(f.made).toHaveLength(1);
    expect(f.made[0]!.onload).toBeNull();
    expect(n.style.backgroundImage).toBe("");
  });
});
