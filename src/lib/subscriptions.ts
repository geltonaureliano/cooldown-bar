type Unlisten = () => void;

/** Clean up even when Tauri resolves listen() after a StrictMode unmount. */
export function subscribe(
  registrations: Array<() => Promise<Unlisten>>,
  ready: () => Promise<void>,
  onError: (error: unknown) => void,
): Unlisten {
  let alive = true;
  const listeners: Unlisten[] = [];
  void Promise.all(registrations.map(async (register) => {
    const unlisten = await register();
    if (alive) listeners.push(unlisten);
    else unlisten();
  })).then(async () => { if (alive) await ready(); })
    .catch((error: unknown) => { if (alive) onError(error); });
  return () => {
    alive = false;
    listeners.splice(0).forEach((unlisten) => unlisten());
  };
}
