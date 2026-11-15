pub fn run_runner_verb() -> Option<i32> {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    let (verb, rest) = args.split_first()?;
    hosted_verb(verb, rest)
}

#[cfg(all(unix, feature = "runner-runtime"))]
fn hosted_verb(verb: &str, rest: &[String]) -> Option<i32> {
    krad_web_listener::run_web_access_verb(verb, rest).or_else(|| {
        krad_runner::run_runner_verb_with(verb, rest, vec![krad_web_listener::web_transport()])
    })
}

#[cfg(not(all(unix, feature = "runner-runtime")))]
fn hosted_verb(verb: &str, rest: &[String]) -> Option<i32> {
    krad_runner::run_runner_verb(verb, rest)
}
