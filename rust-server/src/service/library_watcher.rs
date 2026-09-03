use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::PgPool;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::models::db::library::{self, LibraryRow};
use crate::service::job::JobService;
use crate::utils::file_walk::is_hidden_path;
use crate::utils::glob::path_matches_exclusion;
use crate::utils::mime_types::is_supported_media_path;
use crate::utils::system_config::{get_merged, json_bool};

const WRITE_STABILITY_MS: u64 = 5000;

#[derive(Debug)]
enum WatcherCommand {
    SetEnabled(bool),
    WatchAll,
    Watch(Uuid),
    Unwatch(Uuid),
    UnwatchAll,
}

struct LibraryWatcherManager {
    pool: PgPool,
    jobs: JobService,
    watch_enabled: bool,
    watchers: HashMap<Uuid, LibraryWatchHandle>,
}

struct LibraryWatchHandle {
    _watcher: RecommendedWatcher,
    event_task: JoinHandle<()>,
}

impl Drop for LibraryWatchHandle {
    fn drop(&mut self) {
        self.event_task.abort();
    }
}

struct WatchContext {
    library_id: Uuid,
    exclusion_patterns: Vec<String>,
    jobs: JobService,
    debounce: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    ignore_events: Arc<std::sync::atomic::AtomicBool>,
}

static WATCHER_TX: std::sync::OnceLock<mpsc::UnboundedSender<WatcherCommand>> =
    std::sync::OnceLock::new();

fn init_command_loop(pool: PgPool, jobs: JobService) {
    if WATCHER_TX.get().is_some() {
        return;
    }

    let (tx, mut rx) = mpsc::unbounded_channel();
    let _ = WATCHER_TX.set(tx);

    let manager = Arc::new(Mutex::new(LibraryWatcherManager {
        pool,
        jobs,
        watch_enabled: false,
        watchers: HashMap::new(),
    }));

    tokio::spawn(async move {
        while let Some(command) = rx.recv().await {
            let mut guard = manager.lock().await;
            guard.handle(command).await;
        }
    });
}

pub async fn bootstrap(pool: &PgPool, jobs: JobService) {
    init_command_loop(pool.clone(), jobs);

    let config = match get_merged(pool).await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("library watcher: failed to load config: {err}");
            return;
        }
    };

    let enabled = json_bool(&config, &["library", "watch", "enabled"], false);
    send_command(WatcherCommand::SetEnabled(enabled));
    if enabled {
        send_command(WatcherCommand::WatchAll);
    }
}

pub async fn reload_watch_config(pool: &PgPool) {
    let config = match get_merged(pool).await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("library watcher: failed to reload config: {err}");
            return;
        }
    };

    let enabled = json_bool(&config, &["library", "watch", "enabled"], false);
    send_command(WatcherCommand::SetEnabled(enabled));
    if enabled {
        send_command(WatcherCommand::WatchAll);
    } else {
        send_command(WatcherCommand::UnwatchAll);
    }
}

pub fn request_watch(library_id: Uuid) {
    send_command(WatcherCommand::Watch(library_id));
}

pub fn request_unwatch(library_id: Uuid) {
    send_command(WatcherCommand::Unwatch(library_id));
}

pub fn shutdown() {
    send_command(WatcherCommand::UnwatchAll);
}

fn send_command(command: WatcherCommand) {
    if let Some(tx) = WATCHER_TX.get() {
        let _ = tx.send(command);
    }
}

impl LibraryWatcherManager {
    async fn handle(&mut self, command: WatcherCommand) {
        match command {
            WatcherCommand::SetEnabled(enabled) => {
                self.watch_enabled = enabled;
            }
            WatcherCommand::WatchAll => {
                if !self.watch_enabled {
                    return;
                }
                let libraries = match library::list_all(&self.pool).await {
                    Ok(value) => value,
                    Err(err) => {
                        eprintln!("library watcher: failed to list libraries: {err}");
                        return;
                    }
                };
                for library in libraries {
                    self.watch_library(&library).await;
                }
            }
            WatcherCommand::Watch(id) => {
                if !self.watch_enabled {
                    return;
                }
                match library::get_by_id(&self.pool, &id).await {
                    Ok(Some(library)) => self.watch_library(&library).await,
                    Ok(None) => self.unwatch_library(id),
                    Err(err) => {
                        eprintln!("library watcher: failed to load library {id}: {err}");
                    }
                }
            }
            WatcherCommand::Unwatch(id) => {
                self.unwatch_library(id);
            }
            WatcherCommand::UnwatchAll => {
                self.watchers.clear();
            }
        }
    }

    async fn watch_library(&mut self, library: &LibraryRow) {
        if library.import_paths.is_empty() {
            return;
        }

        self.unwatch_library(library.id);

        println!(
            "Starting to watch library {} with import path(s) {:?}",
            library.id, library.import_paths
        );

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let mut watcher = match RecommendedWatcher::new(
            move |result| {
                if let Ok(event) = result {
                    let _ = event_tx.send(event);
                }
            },
            Config::default(),
        ) {
            Ok(value) => value,
            Err(err) => {
                eprintln!(
                    "library watcher: failed to create watcher for {}: {err}",
                    library.id
                );
                return;
            }
        };

        for import_path in &library.import_paths {
            if let Err(err) = watcher.watch(Path::new(import_path), RecursiveMode::Recursive) {
                eprintln!(
                    "library watcher: failed to watch {import_path} for library {}: {err}",
                    library.id
                );
            }
        }

        let context = Arc::new(WatchContext {
            library_id: library.id,
            exclusion_patterns: library.exclusion_patterns.clone(),
            jobs: self.jobs.clone(),
            debounce: Arc::new(Mutex::new(HashMap::new())),
            ignore_events: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        });

        let ignore_events = context.ignore_events.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            ignore_events.store(false, std::sync::atomic::Ordering::Release);
        });

        let event_context = context.clone();
        let event_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                handle_watch_event(&event_context, event).await;
            }
        });

        self.watchers.insert(
            library.id,
            LibraryWatchHandle {
                _watcher: watcher,
                event_task,
            },
        );
    }

    fn unwatch_library(&mut self, library_id: Uuid) {
        if self.watchers.remove(&library_id).is_some() {
            println!("Stopped watching library {library_id}");
        }
    }
}

async fn handle_watch_event(context: &WatchContext, event: Event) {
    if context
        .ignore_events
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return;
    }

    let is_remove = matches!(event.kind, EventKind::Remove(_));
    let is_sync = matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_));

    if !is_remove && !is_sync {
        return;
    }

    for path in event.paths {
        let Some(path_str) = path.to_str().map(str::to_string) else {
            continue;
        };

        if !is_supported_media_path(&path_str) {
            continue;
        }

        if path_matches_exclusion(&path_str, &context.exclusion_patterns) {
            continue;
        }

        if is_hidden_path(Path::new(&path_str)) {
            continue;
        }

        if is_remove {
            if let Err(err) = context
                .jobs
                .queue_library_remove_asset(&context.library_id, &[path_str])
                .await
            {
                eprintln!(
                    "library watcher: failed to queue remove for {} in library {}: {err}",
                    path.display(),
                    context.library_id
                );
            }
            continue;
        }

        schedule_sync(context, path_str).await;
    }
}

async fn schedule_sync(context: &WatchContext, path: String) {
    let mut pending = context.debounce.lock().await;
    if let Some(handle) = pending.remove(&path) {
        handle.abort();
    }

    let jobs = context.jobs.clone();
    let library_id = context.library_id;
    let debounce = context.debounce.clone();
    let path_for_task = path.clone();

    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(WRITE_STABILITY_MS)).await;
        if let Err(err) = jobs
            .queue_library_sync_files(&library_id, &[path_for_task.clone()])
            .await
        {
            eprintln!(
                "library watcher: failed to queue sync for {path_for_task} in library {library_id}: {err}"
            );
        }
        debounce.lock().await.remove(&path_for_task);
    });

    pending.insert(path, handle);
}
