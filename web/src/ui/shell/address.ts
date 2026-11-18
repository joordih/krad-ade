import type { SessionPlace } from "../../state/sessions";

const SESSION_FRAGMENT = "#s=";
const PAIR_FRAGMENT = "#pair=";
const PLACE_SEPARATOR = "|";

export function placeFromHash(hash: string): SessionPlace | undefined {
  if (!hash.startsWith(SESSION_FRAGMENT)) return undefined;
  const [runtime, environment, session] = decodeURIComponent(hash.slice(SESSION_FRAGMENT.length)).split(PLACE_SEPARATOR);
  const id = Number(session);
  return runtime && environment && session && Number.isFinite(id) ? { runtime, environment, session: id } : undefined;
}

export function hashOf(place: SessionPlace): string {
  return `${SESSION_FRAGMENT}${encodeURIComponent([place.runtime, place.environment, place.session].join(PLACE_SEPARATOR))}`;
}

export function pairingCodeFromHash(hash: string): string | undefined {
  return hash.startsWith(PAIR_FRAGMENT) ? hash.slice(PAIR_FRAGMENT.length) : undefined;
}
