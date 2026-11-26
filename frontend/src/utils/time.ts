import { useMemo } from "react";
import { getT, makeT, useLanguage, type TFn } from "@/i18n";
import { useSetting, type DateFormat, type TimeFormat } from "@/settings";

export function formatMinutesFromNow(dateTime: Date): string {
  const t = getT();
  const secondsUntil = (dateTime.getTime() - Date.now()) / 1000;
  const minutes = Math.round(secondsUntil / 60);
  if (minutes <= 0) return t("time.now");
  return t("time.minutes", { count: minutes });
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

const MONTH_SHORT_KEYS = [
  "date.month.jan",
  "date.month.feb",
  "date.month.mar",
  "date.month.apr",
  "date.month.may",
  "date.month.jun",
  "date.month.jul",
  "date.month.aug",
  "date.month.sep",
  "date.month.oct",
  "date.month.nov",
  "date.month.dec",
] as const;

/** Formats the local calendar date of `date`. `t` gives the month names. */
export function formatDate(date: Date, format: DateFormat, t: TFn): string {
  const day = date.getDate();
  const month = date.getMonth() + 1;
  const monthName = (): string => t(MONTH_SHORT_KEYS[month - 1]!);
  const d = pad2(day);
  const m = pad2(month);
  const y = String(date.getFullYear());
  switch (format) {
    case "dd-mm-yyyy":
      return `${d}-${m}-${y}`;
    case "dd.mm.yyyy.":
      return `${d}.${m}.${y}.`;
    case "dd/mm/yyyy":
      return `${d}/${m}/${y}`;
    case "mm/dd/yyyy":
      return `${m}/${d}/${y}`;
    case "yyyy-mm-dd":
      return `${y}-${m}-${d}`;
    case "d/m/yyyy":
      return `${day}/${month}/${y}`;
    case "d-m-yyyy":
      return `${day}-${month}-${y}`;
    case "d.m.yyyy.":
      return `${day}.${month}.${y}.`;
    case "m/d/yyyy":
      return `${month}/${day}/${y}`;
    case "d mmm yyyy":
      return `${day} ${monthName()} ${y}`;
    case "d. mmm yyyy.":
      return `${day}. ${monthName()} ${y}.`;
    case "mmm d, yyyy":
      return `${monthName()} ${day}, ${y}`;
  }
}

/** Formats the local wall-clock time of `date`. */
export function formatTime(date: Date, format: TimeFormat): string {
  const hours = date.getHours();
  const minutes = pad2(date.getMinutes());
  switch (format) {
    case "HH:mm":
      return `${pad2(hours)}:${minutes}`;
    case "h:mm a":
      return `${hours % 12 || 12}:${minutes} ${hours < 12 ? "AM" : "PM"}`;
  }
}

export type DateTimeFormatter = {
  time: (date: Date) => string;
  dateTime: (date: Date) => string;
};

/** Formatters that follow the date and time format settings. */
export function useDateTimeFormatter(): DateTimeFormatter {
  const dateFormat = useSetting("dateFormat");
  const timeFormat = useSetting("timeFormat");
  const language = useLanguage();
  return useMemo(() => {
    const t = makeT(language);
    return {
      time: (date) => formatTime(date, timeFormat),
      dateTime: (date) => `${formatDate(date, dateFormat, t)} ${formatTime(date, timeFormat)}`,
    };
  }, [dateFormat, timeFormat, language]);
}
