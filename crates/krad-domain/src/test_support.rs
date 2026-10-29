const FLOAT_EPSILON: f32 = 1e-6;

pub(crate) fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < FLOAT_EPSILON,
        "{actual} differs from {expected}"
    );
}
