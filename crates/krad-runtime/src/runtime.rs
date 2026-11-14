use krad_protocol::{EventKind, Seq};
use krad_store::{ChatImages, Secrets};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::adapters::AcpAdapters;
use crate::app_events::AppEvents;
use crate::attention::Attention;
use crate::bus::Bus;
use crate::chats::{Chats, EditorBuffers};
use crate::cloud::CloudSessions;
use crate::cloud_runs::CloudRuns;
use crate::code_hosts::CodeHostWatch;
use crate::environments::Environments;
use crate::file_asks::Uploads;
use crate::gateway::Gateway;
use crate::git::Git;
use crate::harness_health::Health;
use crate::hosts::Hosts;
use crate::jobs::Jobs;
use crate::kraddies::Kraddies;
use crate::launches::Launches;
use crate::models::Models;
use crate::opencode::OpencodeServers;
use crate::reviews::Reviews;
use crate::runners::Runners;
use crate::settings::AppSettings;
use crate::subscribers::Subscribers;
use crate::terminals::Terminals;
use crate::typing::Typist;
use crate::usage::Usage;
use crate::workers::Workers;

mod acp_listing;
mod agent_commands;
mod archive_files;
mod batches;
mod bus_git;
mod chat_images;
mod chatting;
mod ci_fixing;
mod cloud_hosting;
mod cloud_nodes;
mod cloud_results;
mod clouding;
mod code_host_watch;
mod commands;
mod digesting;
mod file_asks;
mod finished_jobs;
mod following;
mod git_acts;
mod gitting;
mod harness_spots;
mod history_asks;
mod hooking;
mod host_asks;
mod hosting;
mod keystrokes;
mod kraddie_bus;
mod kraddie_chats;
mod kraddie_commands;
mod kraddie_following;
mod kraddie_rounds;
mod kraddie_runs;
mod kraddie_worktrees;
mod launching;
mod opening_beside;
mod parts;
mod removing;
mod replies;
mod reviewing;
mod runner_lines;
mod runner_logins;
mod runner_return;
mod runner_webhooks;
mod running;
mod session_asks;
mod session_changes;
mod session_events;
mod settling;
mod setup;
mod sign_in_launches;
mod signing_in;
mod starting;
mod terminal_targets;
mod ticking;
mod variant_bus;
mod variant_picks;
mod variant_progress;
mod variant_runs;
mod waiting;
mod worker_worktrees;
mod working;

pub(crate) use self::chat_images::ChatImageDone;
pub(crate) use self::ci_fixing::PendingFix;
pub(crate) use self::kraddie_following::LinesCounted;
pub(crate) use self::kraddie_runs::RunPlan;
pub(crate) use self::launching::AttachedLine;

pub(crate) use self::replies::{answer_to, refused};
pub use self::setup::RuntimeSetup;
pub(crate) use self::sign_in_launches::SignInEnvRead;

const UNSUPPORTED_COMMAND: &str = "this runtime does not know the command";
const NO_BUS: &str = "this runtime serves no agent bus";

#[derive(Debug)]
pub struct Runtime {
    seq: Seq,
    outbox: Vec<EventKind>,
    subscribers: Subscribers,
    bus: Option<Bus>,
    events: AppEvents,
    attention: Attention,
    jobs: Jobs,
    health: Health,
    usage: Usage,
    models: Models,
    environments: Environments,
    terminals: Terminals,
    hosts: Hosts,
    chats: Chats,
    adapters: AcpAdapters,
    buffers: EditorBuffers,
    code_hosts: CodeHostWatch,
    reviews: Reviews,
    runners: Runners,
    cloud: CloudSessions,
    cloud_runs: CloudRuns,
    gateway: Gateway,
    opencode: OpencodeServers,
    settings: AppSettings,
    launches: Launches,
    workers: Workers,
    sessions_seen: session_events::SessionsSeen,
    git: Git,
    kraddies: Kraddies,
    typist: Typist,
    secrets: Secrets,
    chat_images: Option<ChatImages>,
    uploads: Uploads,
    stopped: Arc<AtomicBool>,
}
