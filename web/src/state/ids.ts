const UUID_BYTES = 16;
const VERSION_BYTE = 6;
const VARIANT_BYTE = 8;
const VERSION_MASK = 0x0f;
const VERSION_FOUR = 0x40;
const VARIANT_MASK = 0x3f;
const VARIANT_RFC = 0x80;
const HEX_RADIX = 16;
const HEX_WIDTH = 2;
const GROUP_ENDS = [8, 12, 16, 20, 32];

export function hexOf(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(HEX_RADIX).padStart(HEX_WIDTH, "0")).join("");
}

export function newResumeId(): string {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  const bytes = crypto.getRandomValues(new Uint8Array(UUID_BYTES));
  bytes[VERSION_BYTE] = ((bytes[VERSION_BYTE] ?? 0) & VERSION_MASK) | VERSION_FOUR;
  bytes[VARIANT_BYTE] = ((bytes[VARIANT_BYTE] ?? 0) & VARIANT_MASK) | VARIANT_RFC;
  const hex = hexOf(bytes);
  return GROUP_ENDS.map((end, at) => hex.slice(GROUP_ENDS[at - 1] ?? 0, end)).join("-");
}
