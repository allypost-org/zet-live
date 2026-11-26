import { test } from "node:test";
import assert from "node:assert/strict";
import "@/test-storage";
import { settingsStore } from "@/settings";

void test("OAuth popup failures use the active language and preserve server messages", async (t) => {
  const previousLanguage = settingsStore.getState().language;
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  let blocked = false;
  const popup = { closed: false, close: () => undefined };
  let timeout = () => {};
  let pollClosed = () => {};
  let onMessage: (event: { origin: string; data: unknown }) => void = () => {};

  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: { querySelector: () => null },
  });
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      location: { origin: "https://zet.test" },
      screen: { width: 1200, height: 900 },
      open: () => (blocked ? null : popup),
      addEventListener: (_type: string, listener: typeof onMessage) => {
        onMessage = listener;
      },
      removeEventListener: () => undefined,
    },
  });
  t.mock.method(globalThis, "setTimeout", (callback: () => void) => {
    timeout = callback;
    return 1;
  });
  t.mock.method(globalThis, "setInterval", (callback: () => void) => {
    pollClosed = callback;
    return 2;
  });
  t.mock.method(globalThis, "clearTimeout", () => undefined);
  t.mock.method(globalThis, "clearInterval", () => undefined);

  try {
    const { openOAuthPopup } = await import("@/utils/oauth-popup");
    // VITE_API_URL from a local .env can point to another origin.
    const { backendOrigin } = await import("@/capabilities-store");
    await t.test("blocked popup explains how to recover in Croatian", async () => {
      settingsStore.setState({ language: "hr" });
      blocked = true;
      await assert.rejects(openOAuthPopup("google"), {
        message: "Skočni prozor je blokiran. Dopusti skočne prozore za ovu stranicu.",
      });
      blocked = false;
    });
    await t.test("timeout follows a language change while the popup is open", async () => {
      blocked = false;
      settingsStore.setState({ language: "en" });
      const result = openOAuthPopup("google");
      settingsStore.setState({ language: "hr" });
      timeout();
      assert.equal((await result).error, "Vrijeme za prijavu je isteklo. Pokušaj ponovno.");
    });
    await t.test("closing the popup reports the local error in Croatian", async () => {
      settingsStore.setState({ language: "hr" });
      const result = openOAuthPopup("google");
      popup.closed = true;
      pollClosed();
      assert.equal((await result).error, "Prozor za prijavu je zatvoren.");
      popup.closed = false;
    });
    await t.test("server error text is unchanged", async () => {
      popup.closed = false;
      const result = openOAuthPopup("google");
      onMessage({
        origin: backendOrigin(),
        data: { type: "zet-auth-callback", ok: false, error: "Provider unavailable" },
      });
      assert.equal((await result).error, "Provider unavailable");
    });
  } finally {
    t.mock.restoreAll();
    settingsStore.setState({ language: previousLanguage });
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else Reflect.deleteProperty(globalThis, "window");
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else Reflect.deleteProperty(globalThis, "document");
  }
});
