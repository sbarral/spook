mod parser;
mod server;

use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
use std::ffi::OsString;
use std::process::Command;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_EVENT_NAME: &str = "update";
const MIN_EVENT_PORT: u16 = 1024;
const MAX_EVENT_PORT: u16 = 65535;
const DEFAULT_EVENT_PORT: u16 = 2133;
const MIN_NOTIFY_PERIOD: u64 = 100;
const MAX_NOTIFY_PERIOD: u64 = 3600000;
const DEFAULT_NOTIFY_PERIOD: u64 = 1000;
const EVENT_PATH: &str = "/events";
const CMD_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION_MSG: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nCopyright (C) 2019 Serge Barral"
);

/// A Server-Sent Event.
#[derive(Clone, Debug)]
pub struct Event {
    name: String,
    port: u16,
}

/// Verbose information.
#[derive(Clone, Debug)]
pub struct VerboseInfo {
    event_out: Option<Event>,
    sub_cmd_repr: Option<String>,
}

/// Parse result.
#[derive(Clone, Default, Debug)]
pub struct ArgList {
    pub sub_cmd_args: Vec<OsString>,
    pub watched_files: Vec<OsString>,
    pub event_out: Option<Event>,
    pub notify_period: u64,
    pub verbose: bool,
    pub init: bool,
}

/// Crate result type.
type Result<T> = std::result::Result<T, String>;

/// Main.
fn main() -> Result<()> {
    let arg_list = parser::parse_args();
    // Set up the file watcher.
    let (tx, rx) = channel();
    let mut watcher = watcher(tx.clone(), Duration::from_millis(arg_list.notify_period)).unwrap();
    for file in &arg_list.watched_files {
        watcher
            .watch(file, RecursiveMode::Recursive)
            .map_err(|err| match err {
                notify::Error::PathNotFound => {
                    format!("{}: {}: file not found", CMD_NAME, file.to_string_lossy())
                }
                _ => format!("{}: {}: file error", CMD_NAME, file.to_string_lossy()),
            })?
    }

    // Launch a background thread to manage server-sent events subscribers.
    let event_tx_list = arg_list.event_out.clone().map(|event| {
        let event_tx_list = Arc::new(Mutex::new(Vec::new()));
        let event_tx_list_clone = event_tx_list.clone();
        thread::spawn(move || server::manage_connections(event_tx_list_clone, event));

        event_tx_list
    });

    // Verbose information.
    let verbose_info = VerboseInfo {
        event_out: arg_list.event_out.clone().filter(|_| arg_list.verbose),
        sub_cmd_repr: if arg_list.verbose && !arg_list.sub_cmd_args.is_empty() {
            Some(
                arg_list
                    .sub_cmd_args
                    .iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect::<Vec<String>>()
                    .join(" "),
            )
        } else {
            None
        },
    };

    // Form the command.
    let mut sub_cmd_iter = arg_list.sub_cmd_args.iter();
    let mut sub_cmd = sub_cmd_iter.next().map(|sub_cmd_name| {
        // Make a command with this name.
        let mut sub_cmd = Command::new(&sub_cmd_name);
        // Set the trailing arguments as the command argument.
        sub_cmd.args(sub_cmd_iter);
        // Done.
        sub_cmd
    });

    // Make a preemptive update if requested.
    if arg_list.init {
        // Run the sub-command.
        update(&mut sub_cmd, &event_tx_list, &verbose_info)?;
    }

    // Run the sub-command and/or send a signal whenever a file is modified.
    loop {
        match rx.recv().unwrap() {
            // Ignore rescan and notices.
            DebouncedEvent::NoticeRemove(_)
            | DebouncedEvent::NoticeWrite(_)
            | DebouncedEvent::Rescan => {}

            // Actual modifications.
            DebouncedEvent::Write(_) | DebouncedEvent::Chmod(_) | DebouncedEvent::Create(_) => {
                // Run the sub-command.
                update(&mut sub_cmd, &event_tx_list, &verbose_info)?
            }

            // Removal or replacement through renaming.
            DebouncedEvent::Remove(path) => {
                // Instead of modifying the file, some weird editors
                // (hello Gedit!) remove the current file and recreate it
                // by renaming the buffer.
                // To outsmart such editors, the watcher is set up to watch
                // again a file with the same name. If this succeeds, the
                // file is deemed changed.
                watcher
                    .watch(path.clone(), RecursiveMode::NonRecursive)
                    .map_err(|_| {
                        format!(
                            "[{}] {}: File was deleted",
                            CMD_NAME,
                            path.to_string_lossy()
                        )
                    })
                    .and_then(|_| update(&mut sub_cmd, &event_tx_list, &verbose_info))?
            }

            // Treat renamed files as a fatal error because it may
            // impact the sub-command.
            DebouncedEvent::Rename(path, _) => {
                return Err(format!(
                    "[{}] {}: File was renamed",
                    CMD_NAME,
                    path.to_string_lossy()
                ));
            }

            // Other errors.
            DebouncedEvent::Error(err, path) => {
                // Display the error and the path, if any.
                let err_msg = format!(
                    "[{}] {}{}",
                    CMD_NAME,
                    path.map(|p| {
                        let mut path_str = p.to_string_lossy().into_owned();
                        path_str.push_str(": ");
                        path_str
                    })
                    .unwrap_or_default(),
                    err
                );
                return Err(err_msg);
            }
        }
    }
}

/// Run sub-command and notify subscribers.
fn update(
    sub_cmd: &mut Option<std::process::Command>,
    event_tx_list: &Option<Arc<Mutex<Vec<Sender<()>>>>>,
    verbose_info: &VerboseInfo,
) -> Result<()> {
    // Verbose log for signal.
    if let Some(event_out) = &verbose_info.event_out {
        println!(
            "[{}] Triggering signal '{}' on port {}",
            CMD_NAME, event_out.name, event_out.port
        );
    }

    // Verbose log for command execition.
    if let Some(sub_cmd_repr) = &verbose_info.sub_cmd_repr {
        println!("[{}] Triggering command: {}", CMD_NAME, sub_cmd_repr);
    }

    // Run sub-command, return early on error.
    if let Some(cmd) = sub_cmd {
        cmd.status()
            .map_err(|_| format!("[{}] Failed to run command", CMD_NAME))?;
    }

    // Notify subscribers and forget disconnected subscribers.
    if let Some(event_tx_list) = event_tx_list {
        let tx_list = &mut *event_tx_list.lock().unwrap();
        *tx_list = tx_list.drain(..).filter(|tx| tx.send(()).is_ok()).collect();
    }

    Ok(())
}
