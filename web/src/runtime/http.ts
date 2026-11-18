import type { PairingPermissions } from "../protocol/PairingPermissions";
import { hexOf } from "../state/ids";

export const PAIRING_PATH = "/pairing";
export const UPLOAD_PATH = "/upload";
export const THEME_PATH = "/theme.css";
const INSTALL_KEY = "krad-web-install";
const INSTALL_PREFIX = "web-";
const INSTALL_BYTES = 12;

export class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

const JSON_HEADERS = { "Content-Type": "application/json" };

async function failed(response: Response): Promise<HttpError> {
  const body = (await response.json().catch(() => ({}))) as { error?: string };
  return new HttpError(response.status, body.error ?? response.statusText);
}

export function installId(storage: Pick<Storage, "getItem" | "setItem"> = localStorage): string {
  const known = storage.getItem(INSTALL_KEY);
  if (known) return known;
  const random = crypto.getRandomValues(new Uint8Array(INSTALL_BYTES));
  const made = INSTALL_PREFIX + hexOf(random);
  storage.setItem(INSTALL_KEY, made);
  return made;
}

export async function redeemPairingCode(code: string, device: string): Promise<PairingPermissions> {
  const response = await fetch(PAIRING_PATH, {
    method: "POST",
    headers: JSON_HEADERS,
    body: JSON.stringify({ code: code.trim().toUpperCase(), install: installId(), device }),
    credentials: "same-origin",
  });
  if (!response.ok) throw await failed(response);
  return (await response.json()) as PairingPermissions;
}

export async function forgetPairing(): Promise<void> {
  const response = await fetch(PAIRING_PATH, { method: "DELETE", credentials: "same-origin" });
  if (!response.ok) throw await failed(response);
}

export async function upload(environment: string, dir: string, file: File): Promise<string> {
  const query = new URLSearchParams({ environment, dir, name: file.name });
  const response = await fetch(`${UPLOAD_PATH}?${query.toString()}`, {
    method: "POST",
    body: file,
    credentials: "same-origin",
  });
  if (!response.ok) throw await failed(response);
  return ((await response.json()) as { path: string }).path;
}

export function reloadTheme(document: Document = window.document): void {
  const link = document.querySelector<HTMLLinkElement>(`link[href^="${THEME_PATH}"]`);
  if (!link) return;
  const reloaded = () => {
    for (const listener of themeReloads) listener();
  };
  link.addEventListener("load", reloaded, { once: true });
  link.href = `${THEME_PATH}?at=${Date.now()}`;
}

const themeReloads = new Set<() => void>();

export function onThemeReload(listener: () => void): () => void {
  themeReloads.add(listener);
  return () => themeReloads.delete(listener);
}
