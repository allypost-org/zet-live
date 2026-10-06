export function boardingLocationLabel(index: number): string {
  return "ABCDEFGHIJKLMNOPQRSTUVWXYZ"[index] ?? String(index + 1);
}
