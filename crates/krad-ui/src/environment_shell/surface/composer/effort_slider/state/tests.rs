use super::*;

const LEVELS: [Effort; 3] = [Effort::Low, Effort::High, Effort::Max];

fn chat() -> TerminalId {
    TerminalId(7)
}

#[test]
fn keys_step_close_and_open_the_help() {
    let mut state = PopoverState::new(chat());
    assert_eq!(
        state.press("right", &LEVELS, Some(Effort::Low)),
        PopoverMove::Choose(Effort::High)
    );
    assert_eq!(
        state.press("end", &LEVELS, Some(Effort::Max)),
        PopoverMove::Ignore
    );
    assert_eq!(
        state.press("home", &LEVELS, Some(Effort::Max)),
        PopoverMove::Choose(Effort::Low)
    );
    assert_eq!(state.press("x", &LEVELS, None), PopoverMove::Ignore);
    assert_eq!(state.press("?", &LEVELS, None), PopoverMove::Redraw);
    assert!(state.help);
    assert_eq!(state.press("?", &LEVELS, None), PopoverMove::Redraw);
    assert!(!state.help);
    assert_eq!(state.press("escape", &LEVELS, None), PopoverMove::Close);
}

#[test]
fn dragging_snaps_to_the_level_under_the_pointer_only_while_held() {
    let mut state = PopoverState::new(chat());
    assert_eq!(
        state.drag_over(Effort::Max, Some(Effort::Low), true),
        PopoverMove::Ignore
    );
    assert_eq!(
        state.grab(Effort::High, Some(Effort::Low)),
        PopoverMove::Choose(Effort::High)
    );
    assert_eq!(
        state.drag_over(Effort::High, Some(Effort::High), true),
        PopoverMove::Ignore
    );
    assert_eq!(
        state.drag_over(Effort::Max, Some(Effort::High), true),
        PopoverMove::Choose(Effort::Max)
    );
    assert_eq!(
        state.drag_over(Effort::Low, Some(Effort::Max), false),
        PopoverMove::Ignore
    );
    state.release();
    assert_eq!(
        state.drag_over(Effort::Low, Some(Effort::Max), true),
        PopoverMove::Ignore
    );
}
