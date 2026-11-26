// settings.ts builds its Zustand store at module scope by reading localStorage.
// bun test does not provide localStorage, so tests that import that module first
// need this stub. Returning null makes loadSettings fall back to DEFAULTS.
if (!("localStorage" in globalThis)) {
  Object.defineProperty(globalThis, "localStorage", {
    value: { getItem: () => null, setItem: () => undefined },
    configurable: true,
  });
}

export {};
