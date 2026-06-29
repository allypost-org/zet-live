export function appRequestAnimationFrame(callback: () => void) {
  if (typeof window.requestAnimationFrame === "function") {
    return window.requestAnimationFrame(callback);
  }
  return setTimeout(callback, 16) as ReturnType<typeof window.requestAnimationFrame>;
}

export function appRequestIdleCallback(callback: () => void) {
  if (typeof window.requestIdleCallback === "function") {
    return window.requestIdleCallback(callback);
  }
  return appRequestAnimationFrame(callback) as ReturnType<typeof window.requestIdleCallback>;
}

export function cancelAnimationOrIdleCallback(id: number | null) {
  if (id === null) return;
  if (typeof window.cancelAnimationFrame === "function") {
    window.cancelAnimationFrame(id);
  }
  if (typeof window.cancelIdleCallback === "function") {
    window.cancelIdleCallback(id);
  }
  clearTimeout(id);
}

export function appQueueMicrotask(callback: () => void) {
  if (typeof window.queueMicrotask === "function") {
    window.queueMicrotask(callback);
  }
  setTimeout(callback, 0);
}
