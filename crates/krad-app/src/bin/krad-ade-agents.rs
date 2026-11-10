#[path = "../api_file.rs"]
mod api_file;

fn main() {
    std::process::exit(krad_agents::run_process(api_file::api_file().as_ref()));
}
