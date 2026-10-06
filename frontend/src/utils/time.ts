export function formatMinutesFromNow(dateTime: Date): string {
  const secondsUntil = (dateTime.getTime() - Date.now()) / 1000;
  const minutes = Math.round(secondsUntil / 60);
  if (minutes <= 0) return "now";
  if (minutes === 1) return "1 min";
  return `${minutes} min`;
}

export function formatClockTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
