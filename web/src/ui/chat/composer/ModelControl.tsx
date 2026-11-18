import type { AgentPreset } from "../../../protocol/AgentPreset";
import type { HarnessSource } from "../../../state/harnesses";
import { Icon } from "../../primitives/Icon";
import { useMenu } from "../../primitives/Menu";
import { ControlPill } from "./ControlPill";
import { ModelMenu } from "./ModelMenu";
import type { ModelChoice, ModelFacts, ModelMark } from "./models";

const MODEL_TOOLTIP = "Model";
const MISSING_TOOLTIP = "This model is no longer offered here; the default model answers until you pick another";

export function ModelControl({
  facts,
  harness,
  settings,
  wide,
  described,
  canMark,
  onModel,
  onMark,
}: {
  facts: ModelFacts;
  harness: AgentPreset;
  settings: HarnessSource | undefined;
  wide: boolean;
  described: boolean;
  canMark: boolean;
  onModel: (choice: ModelChoice) => void;
  onMark: (choice: ModelChoice, mark: ModelMark) => void;
}) {
  const menu = useMenu();
  return (
    <div className="control">
      <ControlPill title={facts.missing ? MISSING_TOOLTIP : MODEL_TOOLTIP} open={menu.open} onClick={menu.show}>
        {facts.missing && <Icon name="flag-02" size="xs" tone="warning" />}
        <span className="control-label">{facts.label}</span>
        <Icon name="arrow-down-01" size="xs" tone="subtle" />
      </ControlPill>
      {menu.open && (
        <ModelMenu
          harness={harness}
          settings={settings}
          choices={facts.choices}
          wide={wide}
          described={described}
          canMark={canMark}
          onPick={(choice) => {
            onModel(choice);
            menu.close();
          }}
          onMark={onMark}
          onClose={menu.close}
        />
      )}
    </div>
  );
}
