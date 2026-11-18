import type { Command } from "../../protocol/Command";
import type { FileAsk } from "../../protocol/FileAsk";

export const MAX_READ_BYTES = 8 * 1024 * 1024;

export function fileCommand(environment: string, ask: FileAsk): Command {
  return { area: "file", environment, ask };
}

export function listFolderCommand(environment: string, dir: string): Command {
  return fileCommand(environment, { verb: "list", dir });
}

export function watchFolderCommand(environment: string, dir: string): Command {
  return fileCommand(environment, { verb: "watch", dir });
}

export function unwatchFolderCommand(environment: string, dir: string): Command {
  return fileCommand(environment, { verb: "unwatch", dir });
}

export function readFileCommand(environment: string, path: string): Command {
  return fileCommand(environment, {
    verb: "read",
    path,
    from: 0,
    limit: MAX_READ_BYTES,
    over: "truncate",
  });
}
