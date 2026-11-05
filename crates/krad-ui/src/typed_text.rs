#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
pub(crate) use macos::typed_text;
#[cfg(not(target_os = "macos"))]
pub(crate) use other::typed_text;
