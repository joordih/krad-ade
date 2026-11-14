pub fn run_runner_verb() -> Option<i32> {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    let (verb, rest) = args.split_first()?;
    krad_runner::run_runner_verb(verb, rest)
}
