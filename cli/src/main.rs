use std::env;
use std::panic;

use crate::commands::listbookmarks;
use crate::commands::listeditions;
use crate::commands::listreading;
use crate::commands::{
  getfeed, getgoals, getstreak, getuser, getuserbook, insertjournal, listjournal, login, search,
  setuserbook, switchedition, syncedition, update, updatejournal,
};
use crate::config::CONFIG;
use crate::utils::{UserError, VERSION, write_logfile};

mod commands;
mod storygraph;

mod config;
mod isbn;
mod utils;

use argh::FromArgs;
use itertools::Itertools;

/// The CLI for NickelStorygraph.
#[derive(FromArgs, PartialEq, Debug)]
struct Arguments {
  #[argh(subcommand)]
  command: Option<Commands>,

  /// print version number
  #[argh(switch)]
  version: bool,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum Commands {
  GetFeed(getfeed::GetFeed),
  GetGoals(getgoals::GetGoals),
  GetStreak(getstreak::GetStreak),
  GetUser(getuser::GetUser),
  GetUserBook(getuserbook::GetUserBook),
  InsertJournal(insertjournal::InsertJournal),
  ListBookmarks(listbookmarks::ListBookmarks),
  ListEditions(listeditions::ListEditions),
  ListJournal(listjournal::ListJournal),
  ListReading(listreading::ListReading),
  Login(login::Login),
  Search(search::Search),
  SetUserBook(setuserbook::SetUserBook),
  SwitchEdition(switchedition::SwitchEdition),
  SyncEdition(syncedition::SyncEdition),
  Update(update::Update),
  UpdateJournal(updatejournal::UpdateJournal),
}

fn main() {
  if env::var("RUST_BACKTRACE").is_err() {
    panic::set_hook(Box::new(|info| {
      let msg = info.payload_as_str().unwrap_or("An unknown error occurred");
      eprintln!("{}", msg);
      debug_log!("{}", msg);
      write_logfile();
    }));
  }

  let args: Arguments = argh::from_env();

  if args.version {
    println!("{}", *VERSION);
    return;
  }

  let Some(command) = args.command else {
    eprintln!("A subcommand must be present. Run with --help for more information.");
    std::process::exit(2);
  };

  let res = match command {
    Commands::GetFeed(args) => getfeed::run(args),
    Commands::GetGoals(args) => getgoals::run(args),
    Commands::GetStreak(args) => getstreak::run(args),
    Commands::GetUser(args) => getuser::run(args),
    Commands::GetUserBook(args) => getuserbook::run(args),
    Commands::InsertJournal(args) => insertjournal::run(args),
    Commands::ListBookmarks(args) => listbookmarks::run(args),
    Commands::ListEditions(args) => listeditions::run(args),
    Commands::ListJournal(args) => listjournal::run(args),
    Commands::ListReading(args) => listreading::run(args),
    Commands::Login(args) => login::run(args),
    Commands::Search(args) => search::run(args),
    Commands::SetUserBook(args) => setuserbook::run(args),
    Commands::SwitchEdition(args) => switchedition::run(args),
    Commands::SyncEdition(args) => syncedition::run(args),
    Commands::Update(args) => update::run(args),
    Commands::UpdateJournal(args) => updatejournal::run(args),
  };

  if let Err(e) = res {
    report_error(e);
    std::process::exit(1);
  }

  if CONFIG.debug {
    write_logfile();
  }
}

/// Prints the error to stderr (the channel the hook reads for user-facing
/// messages) and writes the session log. `UserError`s are already phrased for
/// the user and are printed verbatim; everything else gets the generic
/// "unexpected error" wrapper.
fn report_error(e: anyhow::Error) {
  if let Some(u) = e.downcast_ref::<UserError>() {
    eprintln!("{}", u.0);
  } else {
    eprintln!(
      "Encountered an unexpected error. Please report this.<br><br>{:#}",
      e.chain().join("<br>> ")
    );
  }

  debug_log!("{e:#}");
  write_logfile();
}
