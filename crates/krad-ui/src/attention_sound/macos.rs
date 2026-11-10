use krad_domain::AttentionSound;
use objc2_app_kit::NSSound;
use objc2_foundation::NSString;

pub(crate) const MISSING_NOTE: Option<&str> = None;

const CHIME: &str = "Glass";
const PING: &str = "Ping";
const POP: &str = "Pop";
const BELL: &str = "Submarine";

const fn system_name(sound: AttentionSound) -> Option<&'static str> {
    match sound {
        AttentionSound::Silent => None,
        AttentionSound::Chime => Some(CHIME),
        AttentionSound::Ping => Some(PING),
        AttentionSound::Pop => Some(POP),
        AttentionSound::Bell => Some(BELL),
    }
}

pub(crate) fn play(sound: AttentionSound) {
    let Some(name) = system_name(sound) else {
        return;
    };
    let Some(system) = NSSound::soundNamed(&NSString::from_str(name)) else {
        tracing::debug!(?sound, "system sound not found");
        return;
    };
    if !system.play() {
        tracing::debug!(?sound, "system sound did not play");
    }
}
