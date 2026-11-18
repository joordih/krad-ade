import { useEffect, useState } from "react";
import type { SessionPlace } from "../../state/sessions";
import { hashOf, placeFromHash } from "./address";

const MOVES = ["popstate", "hashchange"] as const;

function fromAddress(): SessionPlace | undefined {
  return placeFromHash(location.hash);
}

export function useSelected(): [SessionPlace | undefined, (selected: SessionPlace | undefined) => void] {
  const [selected, setSelected] = useState<SessionPlace | undefined>(fromAddress);
  useEffect(() => {
    const moved = () => setSelected(fromAddress());
    for (const move of MOVES) addEventListener(move, moved);
    return () => {
      for (const move of MOVES) removeEventListener(move, moved);
    };
  }, []);
  const select = (next: SessionPlace | undefined) => {
    if (next) {
      history.pushState(null, "", hashOf(next));
    } else if (fromAddress()) {
      history.back();
      return;
    }
    setSelected(next);
  };
  return [selected, select];
}
