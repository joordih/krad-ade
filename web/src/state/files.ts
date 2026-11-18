import type { CodeDiagnostic } from "../protocol/CodeDiagnostic";
import type { DirEntry } from "../protocol/DirEntry";
import type { FileDiagnostics } from "../protocol/FileDiagnostics";
import type { FolderChanged } from "../protocol/FolderChanged";

export type FolderListings = Record<string, DirEntry[]>;
export type FileDiagnosticMap = Record<string, CodeDiagnostic[]>;

const SEPARATOR = "\u0000";

export function folderKey(environment: string, dir: string): string {
  return `${environment}${SEPARATOR}${dir}`;
}

export function diagnosticsKey(environment: string, path: string): string {
  return `${environment}${SEPARATOR}${path}`;
}

export function withListing(listings: FolderListings, environment: string, dir: string, entries: DirEntry[]): FolderListings {
  return { ...listings, [folderKey(environment, dir)]: entries };
}

export function listingOf(listings: FolderListings, environment: string, dir: string): DirEntry[] | undefined {
  return listings[folderKey(environment, dir)];
}

export function applyFolderChanged(listings: FolderListings, event: FolderChanged): FolderListings {
  return withListing(listings, event.environment, event.dir, event.entries);
}

export function withFileDiagnostics(diagnostics: FileDiagnosticMap, event: FileDiagnostics): FileDiagnosticMap {
  return { ...diagnostics, [diagnosticsKey(event.environment, event.path)]: event.diagnostics };
}

export function diagnosticsOf(diagnostics: FileDiagnosticMap, environment: string, path: string): CodeDiagnostic[] {
  return diagnostics[diagnosticsKey(environment, path)] ?? [];
}
