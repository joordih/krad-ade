import type { AgentPreset } from "../../protocol/AgentPreset";
import type { Settings } from "../../protocol/Settings";
import { harnessChoices } from "../../state/harnesses";
import { NEW_CHAT_LABEL } from "../../state/commands/sessions";
import { MenuItem, MenuSection, MenuSeparator, MenuSurface } from "../primitives/Menu";

const NEW_TAB = "New tab";

export function NewTabMenu({
  settings,
  wide,
  onClose,
  onChat,
  onTerminal,
}: {
  settings: Settings | undefined;
  wide: boolean;
  onClose: () => void;
  onChat: () => void;
  onTerminal: (preset: AgentPreset) => void;
}) {
  return (
    <MenuSurface title={NEW_TAB} wide={wide} onClose={onClose} placement="under-tabs">
      {(dismiss) => {
        const pick = (run: () => void) => () => {
          dismiss();
          run();
        };
        return (
          <>
            <MenuItem icon="bubble-chat" label={NEW_CHAT_LABEL} onPick={pick(onChat)} />
            <MenuSeparator />
            <MenuSection>Terminal with</MenuSection>
            {harnessChoices(settings).map((choice) => (
              <MenuItem key={choice.preset} icon={choice.icon} label={choice.label} onPick={pick(() => onTerminal(choice.preset))} />
            ))}
          </>
        );
      }}
    </MenuSurface>
  );
}
