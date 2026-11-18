import type { CSSProperties } from "react";
import type { ThemedToken } from "shiki/core";
import type { DiagnosticSeverity } from "../../protocol/DiagnosticSeverity";
import { gutterDigits } from "./treeRows";

const FONT_STYLE_ITALIC = 1;
const FONT_STYLE_BOLD = 2;
const BOLD_WEIGHT = 700;
const LINE_ID_PREFIX = "file-line-";

export function lineId(line: number): string {
  return `${LINE_ID_PREFIX}${line + 1}`;
}

function tokenStyle(token: ThemedToken): CSSProperties {
  const style: CSSProperties = {};
  if (token.color) style.color = token.color;
  const font = token.fontStyle ?? 0;
  if (font & FONT_STYLE_ITALIC) style.fontStyle = "italic";
  if (font & FONT_STYLE_BOLD) style.fontWeight = BOLD_WEIGHT;
  return style;
}

export function CodeLines({
  lines,
  tokens,
  markers,
}: {
  lines: readonly string[];
  tokens: ThemedToken[][] | undefined;
  markers: ReadonlyMap<number, DiagnosticSeverity>;
}) {
  const styled = tokens && tokens.length === lines.length ? tokens : undefined;
  const gutter = { "--krad-web-gutter": gutterDigits(Math.max(lines.length, 1)) } as CSSProperties;
  return (
    <div className="code-scroll">
      <div className="code-lines" style={gutter}>
        {lines.map((line, index) => {
          const severity = markers.get(index);
          const row = styled?.[index];
          return (
            <div key={index} id={lineId(index)} className={severity ? `code-line has-${severity}` : "code-line"}>
              <span className="code-line-number">{index + 1}</span>
              <span className="code-text">
                {row
                  ? row.map((token, at) => (
                      <span key={at} style={tokenStyle(token)}>
                        {token.content}
                      </span>
                    ))
                  : line}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
