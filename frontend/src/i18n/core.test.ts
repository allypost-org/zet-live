import { describe, test } from "node:test";
import assert from "node:assert/strict";
import { detectLanguage, pluralKey, substitute, translate } from "./core";

function eq(actual: unknown, expected: unknown) {
  assert.equal(actual, expected);
}

void describe("detectLanguage", () => {
  void test("picks hr from prefixed regional tag", () => {
    eq(detectLanguage(["hr-HR", "en"]), "hr");
  });
  void test("is case-insensitive", () => {
    eq(detectLanguage(["HR-hr"]), "hr");
  });
  void test("takes the first matching entry", () => {
    eq(detectLanguage(["en-US", "hr"]), "en");
  });
  void test("falls back to en", () => {
    eq(detectLanguage(["de-DE", "sl-SI"]), "en");
  });
  void test("empty list falls back to en", () => {
    eq(detectLanguage([]), "en");
  });
});

void describe("pluralKey", () => {
  void test("en: one only for 1", () => {
    eq(pluralKey(1, "en"), "one");
    eq(pluralKey(0, "en"), "other");
    eq(pluralKey(2, "en"), "other");
  });
  void test("hr: 1, 21 one (n%10 == 1 except 11); 2-4 few", () => {
    eq(pluralKey(1, "hr"), "one");
    eq(pluralKey(21, "hr"), "one");
    eq(pluralKey(2, "hr"), "few");
    eq(pluralKey(4, "hr"), "few");
  });
  void test("hr: 11-14 other, 22-24 few", () => {
    eq(pluralKey(12, "hr"), "other");
    eq(pluralKey(14, "hr"), "other");
    eq(pluralKey(22, "hr"), "few");
    eq(pluralKey(24, "hr"), "few");
  });
  void test("hr: 0 and 5+ other", () => {
    eq(pluralKey(0, "hr"), "other");
    eq(pluralKey(5, "hr"), "other");
  });
});

void describe("substitute", () => {
  void test("replaces named params in order", () => {
    eq(
      substitute("{count} polazaka, {stop}", { count: 5, stop: "Kvaternikov trg" }),
      "5 polazaka, Kvaternikov trg",
    );
  });
  void test("missing param leaves the marker", () => {
    eq(substitute("{name} line", undefined), "{name} line");
  });
  void test("renders numbers", () => {
    eq(substitute("{count} min", { count: 3 }), "3 min");
  });
});

void describe("translate", () => {
  void test("plain template", () => {
    eq(translate("Share", "en", undefined), "Share");
  });
  void test("plural object picks hr form and falls back to other when a form is absent", () => {
    const one = { one: "1 polazak", other: "{count} polazaka" };
    eq(translate(one, "hr", { count: 1 }), "1 polazak");
    eq(translate(one, "hr", { count: 3 }), "3 polazaka"); // missing "few" -> other, still substituted
    eq(
      translate({ one: "1 departure", few: "{count} polaska", other: "{count} polazaka" }, "hr", {
        count: 3,
      }),
      "3 polaska",
    );
  });
  void test("plural object picks en form and treats missing count as 0 (other)", () => {
    eq(
      translate({ one: "{count} departure", other: "{count} departures" }, "en", { count: 1 }),
      "1 departure",
    );
    eq(
      translate({ one: "{count} departure", other: "{count} departures" }, "en", undefined),
      "0 departures",
    );
  });
});
