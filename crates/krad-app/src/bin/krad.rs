#[path = "../api_file.rs"]
mod api_file;
#[path = "../runner_verb.rs"]
mod runner_verb;

fn main() {
    let code = runner_verb::run_runner_verb()
        .unwrap_or_else(|| krad_agents::run_process(api_file::api_file().as_ref()));
    std::process::exit(code);
}
