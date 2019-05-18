use clap::{App, Arg};
use std::str::FromStr;

use crate::{
    ArgList, Event, CMD_NAME, DEFAULT_EVENT_NAME, DEFAULT_EVENT_PORT, DEFAULT_NOTIFY_PERIOD,
    MAX_EVENT_PORT, MAX_NOTIFY_PERIOD, MIN_EVENT_PORT, MIN_NOTIFY_PERIOD, VERSION_MSG,
};

/// Parses command line arguments.
pub fn parse_args() -> ArgList {
    let matches = App::new(CMD_NAME)
        .version(VERSION_MSG)
        .arg(
            Arg::with_name("VERBOSE")
                .short("v")
                .long("verbose")
                .help("Inform about event triggers on std output"),
        )
        .arg(
            Arg::with_name("INIT")
                .short("i")
                .long("init")
                .help("Preemptively trigger command/event immediately at launch"),
        )
        .arg(
            Arg::with_name("PERIOD")
                .long("period")
                .takes_value(true)
                .validator(|s| {
                    validate_minmax(
                        s,
                        MIN_NOTIFY_PERIOD,
                        MAX_NOTIFY_PERIOD,
                        format!(
                            "the file notification period should be a delay in the range {}-{}ms",
                            MIN_NOTIFY_PERIOD, MAX_NOTIFY_PERIOD
                        ),
                    )
                })
                .help(&format!(
                    "File watcher notification period [default: {}ms]",
                    DEFAULT_NOTIFY_PERIOD
                )),
        )
        .arg(
            Arg::with_name("SIGNAL")
                .short("s")
                .long("signal")
                .help("Send server events"),
        )
        .arg(
            Arg::with_name("PORT")
                .short("p")
                .long("port")
                .takes_value(true)
                .validator(|s| {
                    validate_minmax(
                        s,
                        MIN_EVENT_PORT,
                        MAX_EVENT_PORT,
                        format!(
                            "the port should be a number in the range {}-{}",
                            MIN_EVENT_PORT, MAX_EVENT_PORT
                        ),
                    )
                })
                .requires("SIGNAL")
                .help(&format!(
                    "TCP port for event broadcast [default: {}]",
                    DEFAULT_EVENT_PORT
                )),
        )
        .arg(
            Arg::with_name("NAME")
                .short("n")
                .long("name")
                .takes_value(true)
                .requires("SIGNAL")
                .help(&format!(
                    "Server event name [default: {}]",
                    DEFAULT_EVENT_NAME
                )),
        )
        .arg(
            Arg::with_name("FILES")
                .help("Files or directories to be watched")
                .multiple(true)
                .required(true),
        )
        .arg(
            Arg::with_name("CMD [ARGS]")
                .help("Command to be run")
                .required_unless("SIGNAL")
                .raw(true),
        )
        .get_matches();

    // Deserialize arguments.
    let mut arg_list = ArgList::default();

    // Verbose flag.
    arg_list.verbose = matches.is_present("VERBOSE");

    // Init flag.
    arg_list.init = matches.is_present("INIT");

    // Notification period.
    arg_list.notify_period = matches
        .value_of("PERIOD")
        .map(|s| s.parse::<u64>().unwrap()) // already checked by validator
        .unwrap_or(DEFAULT_NOTIFY_PERIOD);

    // Signal with optional name and port if specified, or default values.
    arg_list.event_out = if matches.is_present("SIGNAL") {
        Some(Event {
            name: matches
                .value_of("NAME")
                .unwrap_or(DEFAULT_EVENT_NAME)
                .to_owned(),
            port: matches
                .value_of("PORT")
                .map(|s| s.parse::<u16>().unwrap()) // already checked by validator
                .unwrap_or(DEFAULT_EVENT_PORT),
        })
    } else {
        None
    };

    // Files to be watched
    arg_list.watched_files = matches
        .values_of_os("FILES")
        .map(|iter| iter.map(|s| s.to_owned()).collect::<Vec<_>>())
        .unwrap_or_default();

    // Sub-command.
    arg_list.sub_cmd_args = matches
        .values_of_os("CMD [ARGS]")
        .map(|iter| iter.map(|s| s.to_owned()).collect::<Vec<_>>())
        .unwrap_or_default();

    // Success.
    arg_list
}

fn validate_minmax<T: FromStr + Ord>(
    arg: String,
    min: T,
    max: T,
    err_msg: String,
) -> Result<(), String> {
    if let Ok(port) = arg.parse::<T>() {
        if port >= min && port <= max {
            return Ok(());
        }
    }

    Err(err_msg)
}
