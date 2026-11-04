# Theme library

Color themes converted from popular VS Code themes into the app's theme format. Every color comes
from the published theme JSON of the extension (Open VSX package `extension/themes/*.json`, or the
`microsoft/vscode` repository for Solarized). Colors with alpha are blended over the theme's editor
background. Each license was checked in the LICENSE file of the theme's repository.

## Included

| File | Theme | Author | License | Source |
|---|---|---|---|---|
| `one-dark-pro.json` | One Dark Pro | Binaryify | MIT | https://github.com/Binaryify/OneDark-Pro |
| `dracula.json` | Dracula | Dracula Theme | MIT | https://github.com/dracula/visual-studio-code |
| `tokyo-night.json` | Tokyo Night | Enkia | MIT | https://github.com/tokyo-night/tokyo-night-vscode-theme |
| `tokyo-night-light.json` | Tokyo Night Light | Enkia | MIT | https://github.com/tokyo-night/tokyo-night-vscode-theme |
| `catppuccin-latte.json` | Catppuccin Latte | Catppuccin | MIT | https://github.com/catppuccin/vscode |
| `catppuccin-frappe.json` | Catppuccin Frappé | Catppuccin | MIT | https://github.com/catppuccin/vscode |
| `catppuccin-macchiato.json` | Catppuccin Macchiato | Catppuccin | MIT | https://github.com/catppuccin/vscode |
| `catppuccin-mocha.json` | Catppuccin Mocha | Catppuccin | MIT | https://github.com/catppuccin/vscode |
| `nord.json` | Nord | Sven Greb (Nord Theme) | MIT | https://github.com/nordtheme/visual-studio-code |
| `gruvbox-dark-medium.json` | Gruvbox Dark Medium | JD (jdinhify) | MIT | https://github.com/jdinhify/vscode-theme-gruvbox |
| `gruvbox-light-medium.json` | Gruvbox Light Medium | JD (jdinhify) | MIT | https://github.com/jdinhify/vscode-theme-gruvbox |
| `solarized-dark.json` | Solarized Dark | Microsoft, after Ethan Schoonover's Solarized | MIT | https://github.com/microsoft/vscode/tree/main/extensions/theme-solarized-dark |
| `solarized-light.json` | Solarized Light | Microsoft, after Ethan Schoonover's Solarized | MIT | https://github.com/microsoft/vscode/tree/main/extensions/theme-solarized-light |
| `github-light-default.json` | GitHub Light Default | GitHub (Primer) | MIT | https://github.com/primer/github-vscode-theme |
| `github-dark-default.json` | GitHub Dark Default | GitHub (Primer) | MIT | https://github.com/primer/github-vscode-theme |
| `ayu-light.json` | Ayu Light | Ike Kurghinyan (ayu-theme) | MIT | https://github.com/ayu-theme/vscode-ayu |
| `ayu-mirage.json` | Ayu Mirage | Ike Kurghinyan (ayu-theme) | MIT | https://github.com/ayu-theme/vscode-ayu |
| `ayu-dark.json` | Ayu Dark | Ike Kurghinyan (ayu-theme) | MIT | https://github.com/ayu-theme/vscode-ayu |
| `night-owl.json` | Night Owl | Sarah Drasner | MIT | https://github.com/sdras/night-owl-vscode-theme |
| `night-owl-light.json` | Night Owl Light | Sarah Drasner | MIT | https://github.com/sdras/night-owl-vscode-theme |

Every included theme ships its own terminal ANSI colors; none were derived.

## Conversion notes

- Accent: when `focusBorder` is a low-contrast border color (contrast under 2:1 against the
  sidebar background), the accent is taken from `button.background` (Tokyo Night, Tokyo Night
  Light, Nord, Night Owl) or `activityBarBadge.background` (One Dark Pro, Gruvbox Dark Medium,
  Gruvbox Light Medium).
- On accent: when `button.foreground` contrasts less than 3:1 with the accent, or is missing,
  black or white is picked by contrast (One Dark Pro, Gruvbox Light Medium, Solarized Dark,
  Solarized Light, Night Owl Light).
- Punctuation uses the editor foreground where the theme has no `punctuation` scope (One Dark Pro,
  Dracula, Solarized, GitHub, Ayu, Night Owl).
- Dracula defines no cursor color; the terminal cursor is the editor foreground.
- Solarized defines neither `descriptionForeground` nor line number colors; muted text is its
  comment color.
- GitHub defines no editor selection color; the terminal selection is
  `list.activeSelectionBackground`.
- Tokyo Night Light declares `"type": "dark"` in its theme file, but its extension registers it as
  a light theme (`uiTheme: vs`), so it is marked light. Dracula, GitHub and Solarized take their
  appearance from the extension's `uiTheme`, as their theme files have no `type`.

## Skipped

| Theme | Reason |
|---|---|
| Material Theme | The repository (`material-theme/vsc-material-theme`) now redirects to `vira-soft/vira-assets`, the proprietary Vira Theme, which has no open license. No current redistributable source. |
| Monokai Pro | Commercial. Its repository (`Monokai/monokai-pro-vscode`, now `monokai-pro/vscode`) is an issue tracker with no license file. |
