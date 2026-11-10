use super::request::{parse_value, spawned_agent, CliError, Options};
use crate::protocol::{Request, VariantsRequest};
use krad_domain::{
    ModelId, Variant, VariantRun, VariantSpec, WinnerOutcome, MAX_VARIANTS, MIN_VARIANTS,
};
use std::time::SystemTime;

const RUN: &str = "run";
const STATUS: &str = "status";
const PICK: &str = "pick";
const FLAG_VARIANT: &str = "--variant";
const FLAG_MODEL: &str = "--model";
const FLAG_INSTRUCTION: &str = "--instruction";
const FLAG_TEST: &str = "--test";
const FLAG_APPLY: &str = "--apply";
const FLAG_YES: &str = "--yes";
const RUN_ID: &str = "run id";
const VARIANT_ID: &str = "variant number";
const WORD_SEPARATOR: &str = " ";
const FLAG_PREFIX: &str = "--";
const NO_RUNS: &str = "No variant runs";
const DETAIL_SEPARATOR: &str = "  ";
const SECONDS_SUFFIX: &str = "s";

struct Draft {
    agent: String,
    model: Option<String>,
    instruction: Option<String>,
}

pub(super) fn variants_request(options: &Options) -> Result<Request, CliError> {
    let (action, rest) = options
        .positional
        .split_first()
        .ok_or(CliError::VariantsNeedAction)?;
    match action.as_str() {
        RUN => run_request(rest, options.group.clone()),
        STATUS => Ok(Request::Variants {
            run: rest.first().map(|id| parse_value(RUN_ID, id)).transpose()?,
        }),
        PICK => pick_request(rest),
        _ => Err(CliError::VariantsNeedAction),
    }
}

fn run_request(words: &[String], group: Option<String>) -> Result<Request, CliError> {
    let mut drafts: Vec<Draft> = Vec::new();
    let mut test = None;
    let mut prompt: Vec<&str> = Vec::new();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        let mut value = || {
            words
                .next()
                .cloned()
                .ok_or_else(|| CliError::FlagNeedsValue(word.clone()))
        };
        match word.as_str() {
            FLAG_VARIANT => drafts.push(Draft {
                agent: value()?,
                model: None,
                instruction: None,
            }),
            FLAG_MODEL => {
                let model = value()?;
                last(&mut drafts, word)?.model = Some(model);
            }
            FLAG_INSTRUCTION => {
                let instruction = value()?;
                last(&mut drafts, word)?.instruction = Some(instruction);
            }
            FLAG_TEST => test = Some(value()?),
            _ => prompt.push(word),
        }
    }
    let prompt = prompt.join(WORD_SEPARATOR);
    if prompt.trim().is_empty() {
        return Err(CliError::VariantsNeedTask);
    }
    if !(MIN_VARIANTS..=MAX_VARIANTS).contains(&drafts.len()) {
        return Err(CliError::VariantCount);
    }
    let variants = drafts
        .into_iter()
        .map(spec)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Request::RunVariants(VariantsRequest {
        prompt,
        variants,
        test,
        group,
    }))
}

fn last<'a>(drafts: &'a mut [Draft], flag: &str) -> Result<&'a mut Draft, CliError> {
    drafts
        .last_mut()
        .ok_or_else(|| CliError::FlagBeforeVariant(flag.to_owned()))
}

fn spec(draft: Draft) -> Result<VariantSpec, CliError> {
    let harness = spawned_agent(Some(&draft.agent))?;
    let model = draft
        .model
        .map(|text| {
            ModelId::parse(&text).ok_or(CliError::InvalidValue {
                what: FLAG_MODEL.to_owned(),
                text,
            })
        })
        .transpose()?;
    Ok(VariantSpec::new(
        harness,
        model,
        draft.instruction.as_deref(),
    ))
}

fn pick_request(words: &[String]) -> Result<Request, CliError> {
    let flags = |flag: &str| words.iter().any(|word| word == flag);
    let ids: Vec<&String> = words
        .iter()
        .filter(|word| !word.starts_with(FLAG_PREFIX))
        .collect();
    let [run, variant] = ids.as_slice() else {
        return Err(CliError::PickNeedsVariant);
    };
    if !flags(FLAG_YES) {
        return Err(CliError::PickNeedsConfirmation);
    }
    let outcome = if flags(FLAG_APPLY) {
        WinnerOutcome::ApplyOntoBase
    } else {
        WinnerOutcome::KeepWorktree
    };
    Ok(Request::PickVariant {
        run: parse_value(RUN_ID, run)?,
        variant: parse_value(VARIANT_ID, variant)?,
        outcome,
        confirmed: true,
    })
}

pub(super) fn render_runs(runs: &[VariantRun], now: SystemTime) -> String {
    if runs.is_empty() {
        return NO_RUNS.to_owned();
    }
    runs.iter()
        .map(|run| render_run(run, now))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_run(run: &VariantRun, now: SystemTime) -> String {
    let totals = run.totals(now);
    let head = format!(
        "Run {}  [{}]  {}{DETAIL_SEPARATOR}{} spent",
        run.id,
        run.phase().label(),
        run.prompt.lines().next().unwrap_or_default(),
        totals.spent.cost
    );
    std::iter::once(head)
        .chain(
            run.variants
                .iter()
                .map(|variant| render_variant(variant, now)),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_variant(variant: &Variant, now: SystemTime) -> String {
    let label = variant.spec.label(variant.spec.harness.display_name());
    let elapsed = variant
        .elapsed(now)
        .map_or_default(|took| format!("{DETAIL_SEPARATOR}{}{SECONDS_SUFFIX}", took.as_secs()));
    let diff = variant.diff.as_ref().map_or_default(|diff| {
        format!(
            "{DETAIL_SEPARATOR}{} files +{} -{}",
            diff.file_count(),
            diff.added,
            diff.removed
        )
    });
    format!(
        "  {}  {label}  [{}]{DETAIL_SEPARATOR}{} · {} tokens{elapsed}{diff}",
        variant.id,
        variant.status.label(),
        variant.spent.cost,
        variant.spent.tokens.total()
    )
}
