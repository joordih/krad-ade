import { useEffect, useRef, useState } from "react";
import type { CommandOffer } from "../../../protocol/CommandOffer";
import type { IndexEntry } from "../../../protocol/IndexEntry";
import { mentionsCommand } from "../../../state/commands/chat";
import type { AppStore, RuntimeKey } from "../../../state/store";
import { warn } from "../../primitives/send";
import { matchingSkills, suggestTrigger, type SuggestTrigger } from "./suggestions";

export type Suggestion = { kind: "entry"; entry: IndexEntry } | { kind: "skill"; skill: CommandOffer };

export type Suggested = { trigger: SuggestTrigger; rows: Suggestion[] | undefined; selected: number };

export function useSuggestions({
  store,
  runtime,
  environment,
  root,
  skills,
  text,
  cursor,
}: {
  store: AppStore;
  runtime: RuntimeKey;
  environment: string;
  root: string | null;
  skills: readonly CommandOffer[];
  text: string;
  cursor: number;
}) {
  const [dismissed, setDismissed] = useState<string>();
  const [entries, setEntries] = useState<{ query: string; found: IndexEntry[] }>();
  const [selected, setSelected] = useState(0);
  const asked = useRef<string>(undefined);
  const trigger = suggestTrigger(text, cursor);
  const key = trigger ? `${trigger.kind}/${trigger.start}` : undefined;
  const open = trigger && key !== dismissed && (trigger.kind === "skill" || root !== null) ? trigger : undefined;
  const query = open?.kind === "mention" ? open.query : undefined;

  useEffect(() => {
    if (query === undefined || root === null) return;
    asked.current = query;
    store
      .ask(runtime, mentionsCommand(environment, root, query))
      .then((answer) => {
        if (answer.kind === "mentions" && asked.current === query) setEntries({ query, found: answer.entries });
      })
      .catch(warn);
  }, [store, runtime, environment, root, query]);

  useEffect(() => setSelected(0), [key, open?.query]);

  const rows: Suggestion[] | undefined = !open
    ? undefined
    : open.kind === "skill"
      ? matchingSkills(skills, open.query).map((skill) => ({ kind: "skill", skill }))
      : entries?.query === open.query
        ? entries.found.map((entry) => ({ kind: "entry", entry }))
        : undefined;

  const suggested: Suggested | undefined = open ? { trigger: open, rows, selected } : undefined;
  const count = rows?.length ?? 0;
  return {
    suggested,
    move: (down: boolean) => {
      if (count > 0) setSelected((at) => (at + (down ? 1 : count - 1)) % count);
    },
    select: setSelected,
    dismiss: () => setDismissed(key),
  };
}
