import { test } from "node:test";
import assert from "node:assert/strict";
import "@/test-storage";
import { makeT } from "@/i18n";
import { formatDate, formatTime } from "@/utils/time";

void test("12-hour time maps midnight and noon to 12", () => {
  assert.equal(formatTime(new Date(2026, 0, 1, 0, 5), "h:mm a"), "12:05 AM");
  assert.equal(formatTime(new Date(2026, 0, 1, 12, 5), "h:mm a"), "12:05 PM");
  assert.equal(formatTime(new Date(2026, 0, 1, 13, 45), "h:mm a"), "1:45 PM");
});

void test("dates pad single digits only in padded formats", () => {
  const date = new Date(2026, 0, 2, 3, 4);
  const t = makeT("en");
  assert.equal(formatTime(date, "HH:mm"), "03:04");
  assert.equal(formatDate(date, "dd-mm-yyyy", t), "02-01-2026");
  assert.equal(formatDate(date, "mm/dd/yyyy", t), "01/02/2026");
  assert.equal(formatDate(date, "d.m.yyyy.", t), "2.1.2026.");
});

void test("short month names follow the language", () => {
  const date = new Date(2026, 11, 5);
  assert.equal(formatDate(date, "mmm d, yyyy", makeT("en")), "Dec 5, 2026");
  assert.equal(formatDate(date, "d. mmm yyyy.", makeT("hr")), "5. pro 2026.");
});
