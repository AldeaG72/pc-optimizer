// Window controls for the custom title bar. Outside Tauri (tests, previews) they do nothing.

const inTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const isTauri = inTauri;

async function current() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export async function minimize(): Promise<void> {
  if (inTauri()) await (await current()).minimize();
}

export async function toggleMaximize(): Promise<void> {
  if (inTauri()) await (await current()).toggleMaximize();
}

export async function close(): Promise<void> {
  if (inTauri()) await (await current()).close();
}
