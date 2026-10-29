use krad_domain::TerminalTheme;
use krad_session::{AnsiPalette, ANSI_COLORS};

use crate::theme::Palette;

pub(crate) struct TerminalColors {
    pub(crate) background: u32,
    pub(crate) foreground: u32,
    pub(crate) cursor: u32,
    pub(crate) selection: (u32, u32),
    pub(crate) ansi: AnsiPalette,
}

const MATCH_APP_SELECTION_ALPHA: u32 = 0x40;
const OPAQUE: u32 = 0xff;

struct Preset {
    background: u32,
    foreground: u32,
    cursor: u32,
    selection: u32,
    ansi: [u32; ANSI_COLORS],
}

const XCODE_LIGHT: Preset = Preset {
    background: 0x00ff_ffff,
    foreground: 0x0026_2626,
    cursor: 0x0000_0000,
    selection: 0x00b4_d8fd,
    ansi: [
        0x001d_1d1f,
        0x00c4_1a16,
        0x0024_8a3d,
        0x008a_6d00,
        0x0000_40dd,
        0x00aa_0d91,
        0x000e_7490,
        0x008e_8e93,
        0x006e_6e73,
        0x00d7_0015,
        0x001f_8a4c,
        0x00a0_7700,
        0x0000_71e3,
        0x00af_52de,
        0x0000_71a4,
        0x003a_3a3c,
    ],
};

const XCODE_DARK: Preset = Preset {
    background: 0x001f_1f24,
    foreground: 0x00df_dfe0,
    cursor: 0x00df_dfe0,
    selection: 0x0051_5b70,
    ansi: [
        0x001f_1f24,
        0x00ff_8170,
        0x0078_c2b3,
        0x00d9_c97c,
        0x004e_b0cc,
        0x00ff_7ab2,
        0x00b2_81eb,
        0x00df_dfe0,
        0x007f_8c98,
        0x00ff_8170,
        0x00ac_f2e4,
        0x00ff_a14f,
        0x006b_dfff,
        0x00ff_7ab2,
        0x00da_baff,
        0x00ff_ffff,
    ],
};

const SOLARIZED_ANSI: [u32; ANSI_COLORS] = [
    0x0007_3642,
    0x00dc_322f,
    0x0085_9900,
    0x00b5_8900,
    0x0026_8bd2,
    0x00d3_3682,
    0x002a_a198,
    0x00ee_e8d5,
    0x0000_2b36,
    0x00cb_4b16,
    0x0058_6e75,
    0x0065_7b83,
    0x0083_9496,
    0x006c_71c4,
    0x0093_a1a1,
    0x00fd_f6e3,
];

const SOLARIZED_LIGHT: Preset = Preset {
    background: 0x00fd_f6e3,
    foreground: 0x0065_7b83,
    cursor: 0x0058_6e75,
    selection: 0x00ee_e8d5,
    ansi: SOLARIZED_ANSI,
};

const SOLARIZED_DARK: Preset = Preset {
    background: 0x0000_2b36,
    foreground: 0x0083_9496,
    cursor: 0x0093_a1a1,
    selection: 0x0007_3642,
    ansi: SOLARIZED_ANSI,
};

const GITHUB_LIGHT: Preset = Preset {
    background: 0x00ff_ffff,
    foreground: 0x0024_292f,
    cursor: 0x0009_69da,
    selection: 0x00dd_f4ff,
    ansi: [
        0x0024_292f,
        0x00cf_222e,
        0x0011_6329,
        0x004d_2d00,
        0x0009_69da,
        0x0082_50df,
        0x001b_7c83,
        0x006e_7781,
        0x0057_606a,
        0x00a4_0e26,
        0x001a_7f37,
        0x0063_3c01,
        0x0021_8bff,
        0x00a4_75f9,
        0x0031_92aa,
        0x008c_959f,
    ],
};

const ONE_DARK: Preset = Preset {
    background: 0x0028_2c34,
    foreground: 0x00ab_b2bf,
    cursor: 0x0052_8bff,
    selection: 0x003e_4451,
    ansi: [
        0x0028_2c34,
        0x00e0_6c75,
        0x0098_c379,
        0x00e5_c07b,
        0x0061_afef,
        0x00c6_78dd,
        0x0056_b6c2,
        0x00ab_b2bf,
        0x005c_6370,
        0x00e0_6c75,
        0x0098_c379,
        0x00e5_c07b,
        0x0061_afef,
        0x00c6_78dd,
        0x0056_b6c2,
        0x00ff_ffff,
    ],
};

const DRACULA: Preset = Preset {
    background: 0x0028_2a36,
    foreground: 0x00f8_f8f2,
    cursor: 0x00f8_f8f2,
    selection: 0x0044_475a,
    ansi: [
        0x0021_222c,
        0x00ff_5555,
        0x0050_fa7b,
        0x00f1_fa8c,
        0x00bd_93f9,
        0x00ff_79c6,
        0x008b_e9fd,
        0x00f8_f8f2,
        0x0062_72a4,
        0x00ff_6e6e,
        0x0069_ff94,
        0x00ff_ffa5,
        0x00d6_acff,
        0x00ff_92df,
        0x00a4_ffff,
        0x00ff_ffff,
    ],
};

const NORD: Preset = Preset {
    background: 0x002e_3440,
    foreground: 0x00d8_dee9,
    cursor: 0x00d8_dee9,
    selection: 0x0043_4c5e,
    ansi: [
        0x003b_4252,
        0x00bf_616a,
        0x00a3_be8c,
        0x00eb_cb8b,
        0x0081_a1c1,
        0x00b4_8ead,
        0x0088_c0d0,
        0x00e5_e9f0,
        0x004c_566a,
        0x00bf_616a,
        0x00a3_be8c,
        0x00eb_cb8b,
        0x0081_a1c1,
        0x00b4_8ead,
        0x008f_bcbb,
        0x00ec_eff4,
    ],
};

const TOKYO_NIGHT: Preset = Preset {
    background: 0x001a_1b26,
    foreground: 0x00c0_caf5,
    cursor: 0x00c0_caf5,
    selection: 0x0033_467c,
    ansi: [
        0x0015_161e,
        0x00f7_768e,
        0x009e_ce6a,
        0x00e0_af68,
        0x007a_a2f7,
        0x00bb_9af7,
        0x007d_cfff,
        0x00a9_b1d6,
        0x0041_4868,
        0x00f7_768e,
        0x009e_ce6a,
        0x00e0_af68,
        0x007a_a2f7,
        0x00bb_9af7,
        0x007d_cfff,
        0x00c0_caf5,
    ],
};

const GRUVBOX_DARK: Preset = Preset {
    background: 0x0028_2828,
    foreground: 0x00eb_dbb2,
    cursor: 0x00eb_dbb2,
    selection: 0x0050_4945,
    ansi: [
        0x0028_2828,
        0x00cc_241d,
        0x0098_971a,
        0x00d7_9921,
        0x0045_8588,
        0x00b1_6286,
        0x0068_9d6a,
        0x00a8_9984,
        0x0092_8374,
        0x00fb_4934,
        0x00b8_bb26,
        0x00fa_bd2f,
        0x0083_a598,
        0x00d3_869b,
        0x008e_c07c,
        0x00eb_dbb2,
    ],
};

const fn preset(theme: TerminalTheme) -> Option<&'static Preset> {
    match theme {
        TerminalTheme::MatchApp => None,
        TerminalTheme::XcodeLight => Some(&XCODE_LIGHT),
        TerminalTheme::XcodeDark => Some(&XCODE_DARK),
        TerminalTheme::SolarizedLight => Some(&SOLARIZED_LIGHT),
        TerminalTheme::SolarizedDark => Some(&SOLARIZED_DARK),
        TerminalTheme::GitHubLight => Some(&GITHUB_LIGHT),
        TerminalTheme::OneDark => Some(&ONE_DARK),
        TerminalTheme::Dracula => Some(&DRACULA),
        TerminalTheme::Nord => Some(&NORD),
        TerminalTheme::TokyoNight => Some(&TOKYO_NIGHT),
        TerminalTheme::GruvboxDark => Some(&GRUVBOX_DARK),
    }
}

pub(crate) fn terminal_colors(theme: TerminalTheme, pal: &Palette) -> TerminalColors {
    match preset(theme) {
        Some(preset) => TerminalColors {
            background: preset.background,
            foreground: preset.foreground,
            cursor: preset.cursor,
            selection: (preset.selection, OPAQUE),
            ansi: AnsiPalette(preset.ansi),
        },
        None => TerminalColors {
            background: pal.bg_surface,
            foreground: pal.text_strong,
            cursor: pal.accent,
            selection: (pal.accent, MATCH_APP_SELECTION_ALPHA),
            ansi: AnsiPalette(pal.ansi.0),
        },
    }
}
