use serde::{Deserialize, Serialize};

pub const SIDEBAR_DEFAULT_W: f32 = 240.0;
pub const SIDEBAR_MIN_W: f32 = 200.0;
pub const SIDEBAR_MAX_W: f32 = 520.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "f32", into = "f32")]
pub struct SidebarWidth(f32);

impl SidebarWidth {
    #[must_use]
    pub const fn new(width: f32) -> Self {
        Self(width.clamp(SIDEBAR_MIN_W, SIDEBAR_MAX_W))
    }

    #[must_use]
    pub const fn as_f32(self) -> f32 {
        self.0
    }
}

impl Default for SidebarWidth {
    fn default() -> Self {
        Self(SIDEBAR_DEFAULT_W)
    }
}

impl From<f32> for SidebarWidth {
    fn from(width: f32) -> Self {
        Self::new(width)
    }
}

impl From<SidebarWidth> for f32 {
    fn from(width: SidebarWidth) -> Self {
        width.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_width_stays_within_its_limits_even_from_disk() -> serde_json::Result<()> {
        assert!((SidebarWidth::new(10.0).as_f32() - SIDEBAR_MIN_W).abs() < f32::EPSILON);
        assert!((SidebarWidth::new(9_999.0).as_f32() - SIDEBAR_MAX_W).abs() < f32::EPSILON);
        let read: SidebarWidth = serde_json::from_str("5000")?;
        assert!((read.as_f32() - SIDEBAR_MAX_W).abs() < f32::EPSILON);
        assert_eq!(serde_json::to_string(&SidebarWidth::new(300.0))?, "300.0");
        Ok(())
    }
}
