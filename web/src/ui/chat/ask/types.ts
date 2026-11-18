import type { AgentRef } from "../../../protocol/AgentRef";
import type { Command } from "../../../protocol/Command";

export type Send = (command: Command) => void;

export type AskProps = { chat: AgentRef; ask: number; send: Send; wide: boolean };
