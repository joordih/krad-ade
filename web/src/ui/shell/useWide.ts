import { useEffect, useState } from "react";

const WIDE_QUERY = "(min-width: 900px)";

export function useMedia(query: string): boolean {
  const [matches, setMatches] = useState(() => matchMedia(query).matches);
  useEffect(() => {
    const media = matchMedia(query);
    const changed = () => setMatches(media.matches);
    changed();
    media.addEventListener("change", changed);
    return () => media.removeEventListener("change", changed);
  }, [query]);
  return matches;
}

export function useWide(): boolean {
  return useMedia(WIDE_QUERY);
}
