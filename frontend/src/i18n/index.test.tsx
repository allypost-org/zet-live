import { test } from "node:test";
import assert from "node:assert/strict";
import "@/test-storage";
import { renderToStaticMarkup } from "react-dom/server";
import { makeNodeT, makeT, renderText } from "@/i18n";
import { settingsStore } from "@/settings";
import { useStore } from "@/store";
// Load Sonner before the document stub so its style installer skips DOM access.
import "sonner";

void test("rich-text translations render styled parts without key warnings", () => {
  const errors: unknown[][] = [];
  const originalError = console.error;
  console.error = (...args: unknown[]) => errors.push(args);
  try {
    const parts = makeNodeT("en")("auth.modal.transferPrompt", {
      name: <strong>Google</strong>,
    });
    const html = renderToStaticMarkup(<p>{parts}</p>);
    assert.equal(
      html,
      "<p>This <strong>Google</strong> account is already linked to another account. Transfer it to this account?</p>",
    );
    assert.deepEqual(errors, []);
  } finally {
    console.error = originalError;
  }
});

void test("stored stop fallback follows language changes and feed names stay unchanged", () => {
  const previousLanguage = settingsStore.getState().language;
  try {
    settingsStore.setState({ language: "en" });
    useStore.getState().selectStop(["missing"]);
    const name = useStore.getState().stopSelection?.name;
    assert.ok(name);
    assert.equal(renderText(name, makeT("en")), "Unknown stop");
    settingsStore.setState({ language: "hr" });
    assert.equal(renderText(name, makeT("hr")), "Nepoznata stanica");
    assert.equal(renderText("Kvaternikov trg", makeT("hr")), "Kvaternikov trg");
    assert.equal(renderText("stop.unknown", makeT("hr")), "stop.unknown");
  } finally {
    settingsStore.setState({ language: previousLanguage });
    useStore.getState().clearSelection();
  }
});

void test("trip notices change language without fetching the trip again", async (t) => {
  const previousLanguage = settingsStore.getState().language;
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: { querySelector: () => null },
  });
  const responses = [
    {
      body: { d: { stopIds: [], route: [], stopTimes: [], partial: true } },
      english: "No published schedule for this trip",
      croatian: "Nema objavljenog rasporeda za ovu vožnju",
    },
    {
      body: { error: "Trip not found", status: 404 },
      english: "Could not find full stop list.",
      croatian: "Nije moguće dohvatiti cijeli popis stanica.",
    },
  ];

  try {
    const { fetchFollowingRoute } = await import("@/hooks/use-stops");
    for (const response of responses) {
      t.mock.method(globalThis, "fetch", () => Promise.resolve(Response.json(response.body)));
      settingsStore.setState({ language: "en" });
      useStore.getState().selectVehicle("test-vehicle", "test-trip");
      await fetchFollowingRoute("test-trip");
      const notice = useStore.getState().vehicleSelection?.fetchError;
      assert.ok(notice);
      assert.ok(renderText(notice, makeT("en")).startsWith(response.english));
      settingsStore.setState({ language: "hr" });
      assert.ok(renderText(notice, makeT("hr")).startsWith(response.croatian));
      t.mock.restoreAll();
    }
  } finally {
    t.mock.restoreAll();
    settingsStore.setState({ language: previousLanguage });
    useStore.getState().clearSelection();
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else Reflect.deleteProperty(globalThis, "document");
  }
});
