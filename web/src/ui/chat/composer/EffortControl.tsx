import type { CSSProperties } from "react";
import type { Effort } from "../../../protocol/Effort";
import { Icon } from "../../primitives/Icon";
import { MenuItem, MenuSection, MenuSurface, useMenu } from "../../primitives/Menu";
import { ControlPill } from "./ControlPill";
import { effortLabel, effortMeaning, effortTrigger, levelShare, meterStep } from "./controls";
import type { ModelFacts } from "./models";

const EFFORT_TITLE = "Reasoning effort";
const NO_EFFORT = "This model has no reasoning effort levels to choose from";
const ONE_EFFORT = "This model thinks at a single effort level";

function Meter({ at, steps }: { at: number | undefined; steps: number }) {
  return (
    <span className="effort-meter" aria-hidden="true">
      {Array.from({ length: steps }, (_, step) => (
        <span
          key={step}
          className={at !== undefined && step <= at ? "effort-bar on" : "effort-bar"}
          style={{ "--krad-web-share": levelShare(step, steps) } as CSSProperties}
        />
      ))}
    </span>
  );
}

export function EffortControl({ facts, wide, onEffort }: { facts: ModelFacts; wide: boolean; onEffort: (effort: Effort) => void }) {
  const menu = useMenu();
  const steps = facts.efforts.length;
  const trigger = effortTrigger(facts.offersEffort, steps);
  if (trigger === "hidden") return null;
  const label = effortLabel(facts.effort);
  if (trigger === "disabled") {
    return (
      <ControlPill title={steps === 0 ? NO_EFFORT : ONE_EFFORT} disabled>
        <Icon name="flash" size="xs" tone="disabled" />
        <span>{label}</span>
      </ControlPill>
    );
  }
  return (
    <div className="control">
      <ControlPill title={EFFORT_TITLE} open={menu.open} onClick={menu.show}>
        <Icon name="flash" size="xs" tone="muted" />
        <span className="control-label">{label}</span>
        <Meter at={meterStep(facts.efforts, facts.effort, facts.defaultEffort)} steps={steps} />
        <Icon name="arrow-down-01" size="xs" tone="subtle" />
      </ControlPill>
      {menu.open && (
        <MenuSurface title={EFFORT_TITLE} wide={wide} onClose={menu.close} placement="above-end">
          {(dismiss) => (
            <>
              {wide && <MenuSection>{EFFORT_TITLE}</MenuSection>}
              {facts.efforts.map((effort: Effort) => (
                <MenuItem
                  key={effort}
                  label={effortLabel(effort)}
                  detail={effortMeaning(effort)}
                  checked={(facts.effort ?? facts.defaultEffort) === effort}
                  onPick={() => {
                    onEffort(effort);
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
