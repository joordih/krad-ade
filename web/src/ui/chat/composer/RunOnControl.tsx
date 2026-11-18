import { Icon } from "../../primitives/Icon";
import { MenuItem, MenuSection, MenuSurface, useMenu } from "../../primitives/Menu";
import { ControlPill } from "./ControlPill";
import { choiceDetail, sameRunOn, type RunOn, type RunOnChoice } from "./runOn";

const RUN_ON_TITLE = "Run on";
const RUN_ON_TOOLTIP = "Where this session runs: here, or as a cloud session on one of your cloud runners";

export function RunOnControl({
  choices,
  chosen,
  wide,
  onChoose,
}: {
  choices: readonly RunOnChoice[];
  chosen: RunOn;
  wide: boolean;
  onChoose: (runOn: RunOn) => void;
}) {
  const menu = useMenu();
  const label = choices.find((choice) => sameRunOn(choice.runOn, chosen))?.label ?? choices[0]?.label ?? "";
  return (
    <div className="control">
      <ControlPill title={RUN_ON_TOOLTIP} open={menu.open} onClick={menu.show}>
        <Icon name={chosen.kind === "here" ? "home-01" : "cloud"} size="xs" tone="muted" />
        <span className="control-label">{label}</span>
        <Icon name="arrow-down-01" size="xs" tone="subtle" />
      </ControlPill>
      {menu.open && (
        <MenuSurface title={RUN_ON_TITLE} wide={wide} onClose={menu.close} placement="above-start">
          {(dismiss) => (
            <>
              {wide && <MenuSection>{RUN_ON_TITLE}</MenuSection>}
              {choices.map((choice) => (
                <MenuItem
                  key={choice.runOn.kind === "here" ? choice.runOn.kind : choice.runOn.runner}
                  label={choice.label}
                  detail={choiceDetail(choice)}
                  checked={sameRunOn(choice.runOn, chosen)}
                  disabled={!choice.open}
                  onPick={() => {
                    onChoose(choice.runOn);
                    dismiss();
                  }}
                />
              ))}
            </>
          )}
        </MenuSurface>
      )}
    </div>
  );
}
