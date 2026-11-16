const PUSH_PATH = "/push";
const WORKER_PATH = "/sw.js";

export type PushState = "unsupported" | "blocked" | "off" | "on";

function supported(): boolean {
  return "serviceWorker" in navigator && "PushManager" in window && "Notification" in window;
}

function keyBytes(key: string): Uint8Array<ArrayBuffer> {
  const base64 = (key + "=".repeat((4 - (key.length % 4)) % 4)).replaceAll("-", "+").replaceAll("_", "/");
  const raw = atob(base64);
  const bytes = new Uint8Array(raw.length);
  for (let index = 0; index < raw.length; index += 1) bytes[index] = raw.charCodeAt(index);
  return bytes;
}

export async function registerWorker(onOpen: (url: string) => void): Promise<void> {
  if (!("serviceWorker" in navigator)) return;
  navigator.serviceWorker.addEventListener("message", (event: MessageEvent<{ type?: string; url?: string }>) => {
    if (event.data.type === "open" && event.data.url) onOpen(event.data.url);
  });
  await navigator.serviceWorker.register(WORKER_PATH);
}

export async function pushState(): Promise<PushState> {
  if (!supported()) return "unsupported";
  if (Notification.permission === "denied") return "blocked";
  const registration = await navigator.serviceWorker.getRegistration(WORKER_PATH);
  const subscription = await registration?.pushManager.getSubscription();
  return subscription && Notification.permission === "granted" ? "on" : "off";
}

export async function turnPushOn(): Promise<PushState> {
  if (!supported()) return "unsupported";
  if ((await Notification.requestPermission()) !== "granted") return "blocked";
  const answer = await fetch(PUSH_PATH, { credentials: "same-origin" });
  if (!answer.ok) throw new Error(`the runtime refused notifications (${answer.status})`);
  const { key } = (await answer.json()) as { key: string };
  const registration = await navigator.serviceWorker.ready;
  const subscription = await registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: keyBytes(key),
  });
  const kept = await fetch(PUSH_PATH, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(subscription.toJSON()),
  });
  if (!kept.ok) {
    const refusal = (await kept.json().catch(() => ({}))) as { error?: string };
    throw new Error(refusal.error ?? `the runtime did not keep the subscription (${kept.status})`);
  }
  return "on";
}

export async function turnPushOff(): Promise<PushState> {
  const registration = await navigator.serviceWorker.getRegistration(WORKER_PATH);
  const subscription = await registration?.pushManager.getSubscription();
  if (subscription) {
    await fetch(PUSH_PATH, {
      method: "DELETE",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ endpoint: subscription.endpoint }),
    });
    await subscription.unsubscribe();
  }
  return "off";
}
