use krad_domain::AgentPreset;

const NO_PROGRAM: &str = "this harness has no program to name it on the bus";

#[must_use]
pub(crate) fn preset_by_display_name(name: &str) -> Option<AgentPreset> {
    AgentPreset::all()
        .into_iter()
        .find(|preset| preset.display_name().eq_ignore_ascii_case(name))
}

pub(crate) mod program {
    use super::NO_PROGRAM;
    use krad_domain::AgentPreset;
    use serde::{de, ser, Deserialize as _, Deserializer, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn serialize<S: Serializer>(
        preset: &AgentPreset,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let program = preset
            .program()
            .ok_or_else(|| <S::Error as ser::Error>::custom(NO_PROGRAM))?;
        serializer.serialize_str(program)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<AgentPreset, D::Error> {
        let program = String::deserialize(deserializer)?;
        program.parse().map_err(<D::Error as de::Error>::custom)
    }
}

pub(crate) mod display_name {
    use super::preset_by_display_name;
    use krad_domain::AgentPreset;
    use serde::{de, Deserialize as _, Deserializer, Serializer};

    const UNKNOWN_DISPLAY_NAME: &str = "unknown harness name";

    #[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn serialize<S: Serializer>(
        preset: &Option<AgentPreset>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match preset {
            Some(preset) => serializer.serialize_some(preset.display_name()),
            None => serializer.serialize_none(),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<AgentPreset>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|name| {
                preset_by_display_name(&name)
                    .ok_or_else(|| <D::Error as de::Error>::custom(UNKNOWN_DISPLAY_NAME))
            })
            .transpose()
    }
}
