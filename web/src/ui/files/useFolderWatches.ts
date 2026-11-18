import { useEffect, useRef } from "react";
import { unwatchFolderCommand, watchFolderCommand } from "../../state/commands/files";
import type { AppStore, RuntimeKey } from "../../state/store";
import { sendCommand, warn } from "../primitives/send";
import { openFolders } from "./treeRows";

export function useFolderWatches({
  store,
  runtime,
  environment,
  root,
  expanded,
  connected,
}: {
  store: AppStore;
  runtime: RuntimeKey;
  environment: string;
  root: string | undefined;
  expanded: ReadonlySet<string>;
  connected: boolean;
}): () => void {
  const watched = useRef<Set<string>>(new Set());
  const stale = useRef<Set<string>>(new Set());
  const unwatch = (dir: string) => sendCommand(store, runtime, unwatchFolderCommand(environment, dir));
  const loadAll = () => {
    if (!root) return;
    for (const dir of openFolders(root, expanded)) void store.loadFolder(runtime, environment, dir).catch(warn);
  };

  useEffect(() => {
    if (connected) return;
    for (const dir of watched.current) stale.current.add(dir);
    watched.current.clear();
  }, [connected]);

  useEffect(() => {
    if (!connected || !root) return;
    for (const dir of [...stale.current]) {
      stale.current.delete(dir);
      unwatch(dir);
    }
    const wanted = new Set(openFolders(root, expanded));
    for (const dir of wanted) {
      if (watched.current.has(dir)) continue;
      watched.current.add(dir);
      sendCommand(store, runtime, watchFolderCommand(environment, dir));
    }
    for (const dir of [...watched.current]) {
      if (wanted.has(dir)) continue;
      watched.current.delete(dir);
      unwatch(dir);
    }
  }, [connected, root, expanded, runtime, environment, store]);

  const reload = useRef(loadAll);
  reload.current = loadAll;
  useEffect(() => {
    if (connected && root) reload.current();
  }, [connected, root, runtime, environment, store]);

  useEffect(() => {
    const current = watched.current;
    return () => {
      for (const dir of current) unwatch(dir);
      current.clear();
    };
  }, [store, runtime, environment]);

  return loadAll;
}
