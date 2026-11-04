use gpui_kit::Context;

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn reload_workspace(&mut self, cx: &mut Context<Self>) {
        self.reload_file_tree(cx);
        self.refresh_git(cx);
        self.forget_file_index();
        self.forget_missing_problems(cx);
    }
}
