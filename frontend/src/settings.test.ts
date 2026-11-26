import { describe, test } from "node:test";
import assert from "node:assert/strict";
import "./test-storage";
import { syncSettingsSchema } from "./settings";

function fullPayload(): Record<string, unknown> {
  return {
    mapStyle: "flat",
    mapThemeMode: "manual",
    wakeLockEnabled: false,
    uiThemeMode: "manual",
    uiThemeManual: "dark",
    showGbfsStations: true,
    showBuses: true,
    showTrams: false,
    language: "hr",
  };
}

void describe("syncSettingsSchema", () => {
  void test("accepts language values", () => {
    const parsed = syncSettingsSchema.safeParse(fullPayload());
    assert.equal(parsed.success, true);
    assert.ok(parsed.success);
    assert.equal(parsed.data.language, "hr");
  });
  void test("old peer payload without language defaults to auto", () => {
    const payload = fullPayload();
    delete payload.language;
    const parsed = syncSettingsSchema.safeParse(payload);
    assert.equal(parsed.success, true);
    assert.ok(parsed.success);
    assert.equal(parsed.data.language, "auto");
  });
  void test("bogus language falls back to auto without breaking other fields", () => {
    const parsed = syncSettingsSchema.safeParse({ ...fullPayload(), language: "de" });
    assert.equal(parsed.success, true);
    assert.ok(parsed.success);
    assert.equal(parsed.data.language, "auto");
    assert.equal(parsed.data.mapStyle, "flat");
  });
  void test("missing or bogus date and time formats fall back to defaults", () => {
    const payload = { ...fullPayload(), dateFormat: "yyyy/dd/mm" };
    const parsed = syncSettingsSchema.safeParse(payload);
    assert.equal(parsed.success, true);
    assert.ok(parsed.success);
    assert.equal(parsed.data.dateFormat, "dd/mm/yyyy");
    assert.equal(parsed.data.timeFormat, "HH:mm");
    assert.equal(parsed.data.language, "hr");
  });
});
