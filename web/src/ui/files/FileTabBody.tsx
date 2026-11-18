import { diagnosticsOf } from "../../state/files";
import type { AppState, AppStore, RuntimeKey } from "../../state/store";
import { paletteFor } from "./fileIcons";
import { FileViewer } from "./FileViewer";
import { useSheetDark } from "./useSheetDark";

export function FileTabBody({
  store,
  state,
  runtime,
  environment,
  root,
  path,
}: {
  store: AppStore;
  state: AppState;
  runtime: RuntimeKey;
  environment: string;
  root: string | null;
  path: string;
}) {
  const view = state.views[runtime];
  const dark = useSheetDark();
  return (
    <section className="file-tab-body">
      <FileViewer
        key={`${runtime}/${environment}/${path}`}
        store={store}
        runtime={runtime}
        environment={environment}
        root={root ?? ""}
        path={path}
        diagnostics={diagnosticsOf(view?.diagnostics ?? {}, environment, path)}
        palette={paletteFor(view?.snapshot?.settings.file_icon_colors, dark)}
        connected={state.status.status === "open"}
      />
    </section>
  );
}
