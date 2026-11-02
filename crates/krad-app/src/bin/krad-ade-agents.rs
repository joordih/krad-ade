fn main() {
    let mut words = std::env::args_os();
    let invoked = words.next().unwrap_or_default();
    let args: Vec<String> = words
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    std::process::exit(krad_agents::run_invoked(&invoked, &args));
}
