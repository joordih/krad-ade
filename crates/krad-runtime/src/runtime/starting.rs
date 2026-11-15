use krad_domain::UsageBudget;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;
use std::thread;

use crate::adapters::AcpAdapters;
use crate::app_events::AppEvents;
use crate::attention::Attention;
use crate::buffers::Buffers;
use crate::bus::{open, Bus, BusAccess};
use crate::chats::{Chats, EditorBuffers};
use crate::cloud::CloudSessions;
use crate::cloud_runs::CloudRuns;
use crate::code_hosts::CodeHostWatch;
use crate::environments::Environments;
use crate::file_asks::Uploads;
use crate::gateway::Gateway;
use crate::git::Git;
use crate::handle::RuntimeHandle;
use crate::harness_health::Health;
use crate::hosts::Hosts;
use crate::input::Input;
use crate::jobs::Jobs;
use crate::kraddies::{Kraddies, KraddiesSetup};
use crate::launches::{LaunchedViews, Launches};
use crate::models::Models;
use crate::opencode::{OpencodeDirectory, OpencodeServers};
use crate::reviews::Reviews;
use crate::runners::Runners;
use crate::settings::AppSettings;
use crate::subscribers::Subscribers;
use crate::terminals::Terminals;
use crate::typing::Typist;
use crate::usage::Usage;
use crate::workers::Workers;
use krad_protocol::Seq;
use krad_store::{ChatImages, Secrets};

use super::session_events::SessionsSeen;
use super::{Runtime, RuntimeSetup};

const THREAD_NAME: &str = "krad-runtime";
const INPUT_CAPACITY: usize = 1024;

struct Owned {
    events: AppEvents,
    usage: Usage,
    environments: Environments,
    hosts: Hosts,
    opencode: OpencodeDirectory,
    settings: AppSettings,
    views: LaunchedViews,
    kraddies: KraddiesSetup,
    secrets: Secrets,
    chat_images: Option<ChatImages>,
}

impl Runtime {
    /// # Errors
    ///
    /// Fails when the runtime's thread cannot be spawned.
    #[tracing::instrument(skip_all)]
    pub fn start(setup: RuntimeSetup) -> io::Result<RuntimeHandle> {
        let (inbox, input) = sync_channel(INPUT_CAPACITY);
        let stopped = Arc::new(AtomicBool::new(false));
        let access = setup.bus.and_then(|bus| {
            open(bus, inbox.clone(), Arc::clone(&stopped))
                .inspect_err(|error| tracing::warn!(%error, "the agent bus could not start"))
                .ok()
                .map(Arc::new)
        });
        let events = AppEvents::start(setup.events, &setup.secrets, &inbox);
        let usage = Usage::new(Arc::new(UsageBudget::default()));
        let environments = Environments::start(setup.environments);
        let settings = AppSettings::start(setup.settings, &inbox);
        let host_context = setup.host_tools.context(settings.current.host_timeouts);
        let (hosts, links) = Hosts::new(access.clone(), host_context);
        let opencode = OpencodeDirectory::default();
        let views = LaunchedViews::default();
        let owned = Owned {
            events,
            usage,
            environments,
            hosts,
            opencode: opencode.clone(),
            settings,
            views: views.clone(),
            kraddies: setup.kraddies,
            secrets: setup.secrets,
            chat_images: setup.chat_images,
        };
        let secrets = owned.secrets.clone();
        let shared_access = access.clone();
        let runtime_inbox = inbox.clone();
        let own_runner = setup.own_runner;
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                let mut runtime = Self::idle(shared_access, owned, &runtime_inbox, stopped);
                runtime.adopt_own_runner(own_runner);
                runtime.run(&input);
            })?;
        tracing::info!("runtime started");
        Ok(RuntimeHandle::new(
            inbox,
            access,
            (links, opencode, views, secrets),
        ))
    }

    fn idle(
        access: Option<Arc<BusAccess>>,
        owned: Owned,
        inbox: &SyncSender<Input>,
        stopped: Arc<AtomicBool>,
    ) -> Self {
        let sessions_seen = SessionsSeen::new(owned.environments.iter());
        Self {
            seq: Seq::default(),
            outbox: Vec::new(),
            subscribers: Subscribers::default(),
            bus: access.map(|access| Bus::new(access, inbox.clone())),
            events: owned.events,
            attention: Attention::default(),
            jobs: Jobs::new(inbox.clone()),
            health: Health::default(),
            usage: owned.usage,
            models: Models::default(),
            environments: owned.environments,
            terminals: Terminals::new(inbox.clone()),
            hosts: owned.hosts,
            chats: Chats::new(inbox.clone()),
            adapters: AcpAdapters::default(),
            buffers: EditorBuffers::default(),
            code_hosts: CodeHostWatch::default(),
            reviews: Reviews::default(),
            runners: Runners::default(),
            cloud: CloudSessions::default(),
            cloud_runs: CloudRuns::default(),
            gateway: Gateway::default(),
            opencode: OpencodeServers::new(inbox.clone(), owned.opencode),
            settings: owned.settings,
            launches: Launches::new(owned.views, inbox.clone()),
            workers: Workers::default(),
            sessions_seen,
            git: Git::default(),
            kraddies: Kraddies::start(owned.kraddies, inbox.clone()),
            typist: Typist::default(),
            secrets: owned.secrets,
            chat_images: owned.chat_images,
            uploads: Uploads::default(),
            editor: Buffers::default(),
            stopped,
        }
    }
}
