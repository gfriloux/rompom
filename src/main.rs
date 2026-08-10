mod collect;
mod conf;
mod emulationstation;
mod hash;
mod package;
mod queue;
mod rom;
mod state;
mod summary;
mod ui;
mod worker;

use std::{
  collections::HashMap,
  env, fs,
  io::{self, IsTerminal as _, Write as _},
  path::Path,
  sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
  },
  thread,
};

use glob::Pattern;
use internet_archive::metadata::Metadata;
use screenscraper::{ApiFailure, ScreenScraper};

use crate::collect::group_multi_disc;
use crate::conf::{Conf, Source};
use crate::queue::{Semaphore, TaskQueue};
use crate::rom::{FolderSource, IaSource, Rom, RomSource, RomSourceData, StepKind, StepStatus};
use crate::state::{write_with_rotation, SystemState};
use crate::ui::Ui;
use crate::worker::{lookup_failure, WorkerContext};

// ── Constants ──────────────────────────────────────────────────────────────

/// Extra main-pool workers beyond the SS-semaphore limit.
/// Keeps downloads and packaging running while SS slots are saturated.
const N_EXTRA_MAIN_WORKERS: usize = 8;
const N_BLOCKING_WORKERS: usize = 2;

/// How often the accumulated ROM state is written to disk mid-run.
const FLUSH_INTERVAL_MS: u64 = 30_000;
/// How often the flusher wakes to check whether the run is over. Short enough that it
/// does not hold up the shutdown by a visible amount.
const FLUSH_POLL_MS: u64 = 250;

/// Compiles a system's `filter` globs once, naming the offending pattern on failure.
///
/// They used to be recompiled inside the per-file filter closure, so an invalid glob in
/// the config panicked once per candidate file — and compiling the same pattern for every
/// file in an Internet Archive item is wasted work besides.
fn compile_patterns(patterns: &[String]) -> Result<Vec<Pattern>, String> {
  patterns
    .iter()
    .map(|p| Pattern::new(p).map_err(|e| format!("invalid filter pattern {:?}: {}", p, e)))
    .collect()
}

/// Resolves a system's `source` block to the list of ROM files to process.
///
/// Everything in here used to `unwrap()`. All of these failures are ordinary user-facing
/// mistakes — a mistyped Internet Archive item, an invalid glob in `rompom.yml`, a folder
/// that does not exist — and they happen after the terminal has entered raw mode, where a
/// panic message is painted over the interface and wiped by the next frame. The caller
/// drops the `Ui` before printing what comes back from here.
fn collect_sources(source: &Source, ui: &Ui) -> Result<Vec<RomSourceData>, String> {
  let mut sources: Vec<RomSourceData> = Vec::new();

  match source {
    Source::InternetArchive(ia_items) => {
      for item in ia_items {
        ui.fetching_metadata(&item.item);

        let patterns = compile_patterns(&item.filter)?;
        let metadata = Arc::new(Metadata::get(&item.item).map_err(|e| {
          format!(
            "could not fetch Internet Archive metadata for item {:?}: {}",
            item.item, e
          )
        })?);

        for file in &metadata.files {
          // A name that is not valid UTF-8 cannot travel any further: every downstream
          // structure is String-based. Skipping it beats failing the whole system.
          let Some(filename) = Path::new(&file.name).file_name().and_then(|n| n.to_str()) else {
            continue;
          };
          if !patterns.iter().any(|pat| pat.matches(filename)) {
            continue;
          }

          let rom_url = metadata
            .file_urls(&file.name)
            .map_err(|e| format!("no download URL for {:?}: {}", file.name, e))?
            .into_iter()
            .next()
            .unwrap_or_default();

          sources.push(RomSourceData {
            file_name: file.name.clone(),
            filename: filename.to_string(),
            source: RomSource::InternetArchive(IaSource {
              rom_url,
              crc32: file.crc32.clone(),
              md5: file.md5.clone(),
              sha1: file.sha1.clone(),
              size: file
                .size
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
              metadata: Arc::clone(&metadata),
            }),
            extra_discs: Vec::new(),
          });
        }
      }
    }

    Source::Folder(folder) => {
      ui.fetching_metadata(&folder.path);

      let patterns = compile_patterns(&folder.filter)?;
      let dir = Path::new(&folder.path);
      let entries = fs::read_dir(dir)
        .map_err(|e| format!("could not read source folder {:?}: {}", folder.path, e))?;

      for entry in entries {
        let entry =
          entry.map_err(|e| format!("could not read an entry of {:?}: {}", folder.path, e))?;
        let path = entry.path();
        if !path.is_file() {
          continue;
        }

        // Same reasoning as above, for both the base name and the full path.
        let (Some(filename), Some(full_path)) =
          (path.file_name().and_then(|n| n.to_str()), path.to_str())
        else {
          continue;
        };
        if !patterns.iter().any(|pat| pat.matches(filename)) {
          continue;
        }

        sources.push(RomSourceData {
          file_name: full_path.to_string(),
          filename: filename.to_string(),
          source: RomSource::Folder(FolderSource {
            local_path: path.clone(),
          }),
          extra_discs: Vec::new(),
        });
      }
    }
  }

  Ok(sources)
}

/// Something went wrong while running: no config directory, unreadable config, unknown
/// system, a system with no source, ScreenScraper refusing the credentials.
const EXIT_FAILURE: i32 = 1;
/// The command line itself is wrong — an unknown flag, a missing value. Kept distinct
/// from `EXIT_FAILURE` so a script can tell "I invoked it wrong" from "it tried and
/// could not".
const EXIT_USAGE: i32 = 2;

fn print_usage(program: &str, opts: &getopts::Options) {
  let brief = format!("Usage: {} -s SYSTEM", program);
  print!("{}", opts.usage(&brief));
}

/// The sample config at the repository root, embedded at compile time so the starter
/// file and the documented example can never drift apart.
const CONFIG_TEMPLATE: &str = include_str!("../rompom.yml");

/// Writes a starter `rompom.yml`, refusing to touch one that is already there.
///
/// `create_new` rather than `exists()` then `write`: this file holds the user's
/// ScreenScraper password, and the check and the write have to be the same operation for
/// the refusal to be worth anything.
fn init_config(path: &Path) -> Result<(), String> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)
      .map_err(|e| format!("could not create {}: {}", parent.display(), e))?;
  }

  match fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(path)
  {
    Ok(mut file) => file
      .write_all(CONFIG_TEMPLATE.as_bytes())
      .map_err(|e| format!("could not write {}: {}", path.display(), e)),
    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(format!(
      "{} already exists — rompom will not overwrite it",
      path.display()
    )),
    Err(e) => Err(format!("could not create {}: {}", path.display(), e)),
  }
}

/// Reads `--resume=yes|no` into an answer, `None` when the flag was not given.
fn parse_resume_flag(value: Option<&str>) -> Result<Option<bool>, String> {
  match value {
    None => Ok(None),
    Some(v) => match v.to_ascii_lowercase().as_str() {
      "yes" | "y" | "true" => Ok(Some(true)),
      "no" | "n" | "false" => Ok(Some(false)),
      other => Err(format!("--resume expects yes or no, got {:?}", other)),
    },
  }
}

/// Asks on the terminal whether to resume an interrupted run.
///
/// The read used to be `read_line(..).unwrap()` and the answer matched against
/// `"" | "y" | "yes"`. With stdin closed — a CI runner, a `< /dev/null` — `read_line`
/// returns `Ok(0)` and leaves the buffer empty, so nobody answering was read as the
/// default *yes*: the run silently resumed a `run.yml` nothing had validated.
fn ask_resume(done: usize, total: usize) -> bool {
  print!(
    "Found interrupted run ({}/{} done). Resume? [Y/n]: ",
    done, total
  );
  if io::stdout().flush().is_err() {
    return false;
  }

  let mut answer = String::new();
  match io::stdin().read_line(&mut answer) {
    Ok(0) => {
      println!();
      false
    }
    Ok(_) => matches!(answer.trim().to_lowercase().as_str(), "" | "y" | "yes"),
    Err(_) => false,
  }
}

/// Prints every system in the loaded config with its ScreenScraper id and where its ROMs
/// come from.
///
/// This is the answer to "why does rompom say my system is unknown": the name has to
/// match `rompom.yml` exactly, and a system with no `source` block cannot be run either
/// — so it is called out rather than merely listed.
fn list_systems(conf: &Conf) {
  if conf.systems.is_empty() {
    println!("No system declared in rompom.yml.");
    return;
  }

  let width = conf
    .systems
    .iter()
    .map(|s| s.name.len())
    .max()
    .unwrap_or(0)
    .max(6);

  println!("{:<width$}  {:>5}  SOURCE", "SYSTEM", "ID", width = width);
  for system in &conf.systems {
    let source = match &system.source {
      Some(Source::InternetArchive(items)) => {
        let names: Vec<&str> = items.iter().map(|i| i.item.as_str()).collect();
        format!("internet_archive: {}", names.join(", "))
      }
      Some(Source::Folder(folder)) => format!("folder: {}", folder.path),
      None => "(no source — cannot be run)".to_string(),
    };
    println!(
      "{:<width$}  {:>5}  {}",
      system.name,
      system.id,
      source,
      width = width
    );
  }
}

// ── main ──────────────────────────────────────────────────────────────────

fn main() {
  let args: Vec<String> = env::args().collect();
  let program = args[0].clone();
  let mut opts = getopts::Options::new();

  let confdir = match dirs::config_dir() {
    Some(x) => x,
    None => {
      eprintln!("rompom: could not find the user configuration directory");
      std::process::exit(EXIT_FAILURE);
    }
  };

  opts.optopt("s", "system", "System to search for", "SYSTEM");
  opts.optflag(
    "",
    "update-config",
    "interactively update rompom.yml with missing fields",
  );
  opts.optflag(
    "",
    "debug",
    "write <system>.debug.log with per-ROM pipeline decisions (useful to diagnose false updates)",
  );
  opts.optflag(
    "",
    "plain",
    "one line per finished ROM instead of the full-screen interface (implied when stdout is not a terminal)",
  );
  opts.optopt(
    "",
    "resume",
    "answer the resume prompt up front instead of being asked",
    "yes|no",
  );
  opts.optflag(
    "",
    "ascii",
    "use ASCII letters instead of Nerd Font icons for the media columns",
  );
  opts.optflag(
    "",
    "init",
    "write a starter rompom.yml, if there is not one already",
  );
  opts.optflag(
    "",
    "list-systems",
    "list the systems declared in rompom.yml and exit",
  );
  opts.optflag("h", "help", "print this help menu");
  opts.optflag("V", "version", "print the version and exit");

  // getopts already words this well ("Unrecognized option: 'systm'"). Panicking on it
  // buried that sentence under a backtrace and a thread name, for what is almost always
  // a typo.
  let matches = match opts.parse(&args[1..]) {
    Ok(m) => m,
    Err(f) => {
      eprintln!("rompom: {}", f);
      print_usage(&program, &opts);
      std::process::exit(EXIT_USAGE);
    }
  };

  if matches.opt_present("h") {
    print_usage(&program, &opts);
    return;
  }

  // Answered before anything is read from disk: the first thing asked of a binary in a
  // bug report is its version, and that has to work on a machine with no config at all.
  // `just version-check` is what guarantees this is the version that was released.
  if matches.opt_present("version") {
    println!("rompom {}", env!("CARGO_PKG_VERSION"));
    return;
  }

  // Before anything can render, and before the summary is built: both read the table
  // through `ui::media_icons()`.
  if matches.opt_present("ascii") {
    ui::use_ascii_icons();
  }

  // stdout not being a terminal is exactly the situation --plain describes, so it
  // selects itself: piping to a file or a CI log gives a readable transcript instead of
  // escape sequences wrapped around a frame nobody will look at.
  let plain = matches.opt_present("plain") || !io::stdout().is_terminal();
  if plain {
    ui::use_plain_output();
  }

  let resume_flag = match parse_resume_flag(matches.opt_str("resume").as_deref()) {
    Ok(value) => value,
    Err(message) => {
      eprintln!("rompom: {}", message);
      std::process::exit(EXIT_USAGE);
    }
  };

  if matches.opt_present("init") {
    let conf_path = confdir.join("rompom.yml");
    if let Err(e) = init_config(&conf_path) {
      eprintln!("rompom: {}", e);
      std::process::exit(EXIT_FAILURE);
    }
    println!(
      "Wrote {}.\nFill in the screenscraper.dev and screenscraper.user credentials, then \
       give a system a source block.",
      conf_path.display()
    );
    return;
  }

  if matches.opt_present("update-config") {
    let conf_path = format!("{}/rompom.yml", confdir.display());
    if let Err(e) = conf::Conf::update(&conf_path) {
      eprintln!("rompom: {}", e);
      std::process::exit(1);
    }
    return;
  }

  let conf = match Conf::load(&format!("{}/rompom.yml", confdir.display())) {
    Ok(c) => c,
    Err(e) => {
      eprintln!("rompom: {}", e);
      std::process::exit(1);
    }
  };

  if matches.opt_present("list-systems") {
    list_systems(&conf);
    return;
  }

  let system_name = match matches.opt_str("s") {
    Some(x) => x,
    None => {
      eprintln!("rompom: no system given");
      print_usage(&program, &opts);
      std::process::exit(EXIT_USAGE);
    }
  };

  let debug_log_path: Option<String> = if matches.opt_present("debug") {
    let path = format!("{}.debug.log", system_name);
    // Create / truncate the file so each run starts fresh.
    match fs::OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(true)
      .open(&path)
    {
      Ok(mut f) => {
        let ts = std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .map(|d| d.as_secs())
          .unwrap_or(0);
        let _ = writeln!(f, "# rompom debug log — {} — unix={}", system_name, ts);
        let _ = writeln!(f);
        Some(path)
      }
      Err(e) => {
        eprintln!("Warning: could not create debug log {}: {}", path, e);
        None
      }
    }
  } else {
    None
  };

  // Both of these used to print and `return`, which is exit 0. In CI a mistyped system
  // name was therefore a green build that had scraped nothing.
  let system = match conf.find_system(&system_name) {
    Some(s) => s,
    None => {
      eprintln!(
        "rompom: system {:?} is not in rompom.yml — run --list-systems to see what is",
        system_name
      );
      std::process::exit(EXIT_FAILURE);
    }
  };

  let source = match system.source.clone() {
    Some(s) => s,
    None => {
      eprintln!(
        "rompom: system {:?} has no source block in rompom.yml, so there is nothing to \
         collect — add an internet_archive or folder source",
        system_name
      );
      std::process::exit(EXIT_FAILURE);
    }
  };

  // ── Resume check ──────────────────────────────────────────────────────

  let run_path = format!("{}.run.yml", system_name);
  let resumed_state: Option<worker::RunState> = if Path::new(&run_path).exists() {
    match worker::load_run_state(&run_path) {
      Ok(s) => {
        let done = s
          .roms
          .iter()
          .filter(|r| r.step_statuses.iter().all(|st| st.is_complete()))
          .count();
        // Nobody to ask in plain mode. Not resuming is the safe default: everything
        // expensive is already skip-if-valid, so a fresh run re-checks rather than
        // re-does, while replaying a run.yml of unknown provenance is the surprising one.
        let resume = match resume_flag {
          Some(answer) => answer,
          None if plain => {
            println!(
              "Found interrupted run ({}/{} done) — starting fresh (pass --resume yes to \
               resume).",
              done,
              s.roms.len()
            );
            false
          }
          None => ask_resume(done, s.roms.len()),
        };
        if resume {
          Some(s)
        } else {
          fs::remove_file(&run_path).ok();
          None
        }
      }
      Err(e) => {
        eprintln!("Warning: could not load {}: {}", run_path, e);
        None
      }
    }
  } else {
    None
  };

  // ── Collection ────────────────────────────────────────────────────────
  //
  // Collect RomSourceData for all matching files first (total unknown),
  // then create bars and Rom structs once the total is known.
  //
  // See `collect_sources` below: every failure here is reported, not panicked, because
  // the terminal is already in raw mode by this point.

  // Loaded before Ui::new so the warning has a plain terminal to land on. A state file
  // that cannot be read means the whole run is about to redo work it already did, which
  // is worth knowing beforehand rather than in the summary.
  let state_path = format!("{}.state.yml", system_name);
  let (loaded_state, state_warning) = SystemState::load(&state_path);
  if let Some(warning) = state_warning {
    eprintln!("rompom: {}", warning);
  }

  // Must precede Ui::new: from here on the terminal is in raw mode and the default
  // panic output would be written over the interface.
  worker::install_panic_hook();

  let interrupted = Arc::new(AtomicBool::new(false));
  let queue = TaskQueue::new();
  let ui = Ui::new(Arc::clone(&interrupted), Arc::clone(&queue));
  let sources = match collect_sources(&source, &ui) {
    Ok(sources) => sources,
    Err(message) => {
      // Dropping the Ui leaves raw mode and restores the screen. Printing before this
      // would write over the interface and be wiped by the next frame.
      drop(ui);
      eprintln!("rompom: {}", message);
      std::process::exit(1);
    }
  };

  // ── Group multi-disc files ────────────────────────────────────────────

  let sources = group_multi_disc(sources);

  // ── RomSourceData → Arc<Mutex<Rom>> ──────────────────────────────────

  let total = sources.len();
  let roms: Vec<Arc<Mutex<Rom>>> = sources
    .into_iter()
    .enumerate()
    .map(|(i, source)| {
      let bar = ui.new_rom_bar(i + 1, total, &source.filename);
      if matches!(&source.source, RomSource::Folder(_)) {
        Rom::new_folder(source, bar)
      } else {
        Rom::new_ia(source, bar)
      }
    })
    .collect();

  // Apply run state from a previous interrupted run.
  if let Some(ref run_state) = resumed_state {
    for rom_arc in &roms {
      let mut rom = rom_arc.lock().unwrap();
      if let Some(entry) = run_state
        .roms
        .iter()
        .find(|r| r.filename == rom.source.filename)
      {
        worker::apply_run_state(&mut rom, entry);
        worker::restore_bar_for_resumed_rom(&rom);
      }
    }
  }

  let all_roms = Arc::new(roms);

  // ── Pipeline setup ────────────────────────────────────────────────────

  // Wrong credentials are the everyday case here, and this ran under the TUI: the panic
  // was painted over the interface and the terminal left in raw mode.
  let ss = match ScreenScraper::new(
    &conf.screenscraper.user.login,
    &conf.screenscraper.user.password,
    &conf.screenscraper.dev.login,
    &conf.screenscraper.dev.password,
  ) {
    Ok(ss) => ss,
    Err(e) => {
      drop(ui);
      // Never print `e`. On a transport failure its Display carries the request URL,
      // and the credentials travel in that URL's query string.
      let reason = lookup_failure(e.failure())
        .map(|failure| failure.to_string())
        .unwrap_or_else(|| "ScreenScraper does not know this account".to_string());
      let advice = match e.failure() {
        ApiFailure::Transport => "Check your network connection, then try again.",
        ApiFailure::ThreadLimit | ApiFailure::ServerBusy | ApiFailure::ApiClosed => {
          "ScreenScraper is busy — try again later."
        }
        ApiFailure::QuotaExceeded | ApiFailure::KoQuotaExceeded => {
          "Your ScreenScraper quota is spent for today."
        }
        _ => "Check the screenscraper.user and screenscraper.dev credentials in your config.",
      };
      eprintln!(
        "rompom: could not authenticate against ScreenScraper: {}\n{}",
        reason, advice
      );
      std::process::exit(1);
    }
  };

  let n_disc = ss.user_info.maxthreads as usize;
  let modal_tx = ui.modal_sender();
  let state = Arc::new(Mutex::new(loaded_state));
  let ss = Arc::new(ss);
  let system = Arc::new(system);
  let lang = Arc::new(conf.lang);

  // Count ROMs whose SaveState step still needs to run.
  let remaining_count = all_roms
    .iter()
    .filter(|rom_arc| {
      let rom = rom_arc.lock().unwrap();
      let last = rom.pipeline.len() - 1;
      !matches!(
        rom.pipeline[last].status,
        StepStatus::Done | StepStatus::Skipped | StepStatus::Failed(_)
      )
    })
    .count();

  // ── Ctrl-C handler ────────────────────────────────────────────────────

  {
    let queue = Arc::clone(&queue);
    let interrupted = Arc::clone(&interrupted);
    ctrlc::set_handler(move || {
      // Second Ctrl-C: hard exit.
      if interrupted.swap(true, Ordering::SeqCst) {
        std::process::exit(1);
      }
      eprintln!("\nInterrupted — waiting for active steps to finish...");
      queue.shutdown();
    })
    .expect("Error setting Ctrl-C handler");
  }

  // All ROMs already done (full resume with no pending work).
  if remaining_count == 0 {
    fs::remove_file(&run_path).ok();
    let summary = ui.summary();
    drop(ui);
    summary.print();
    return;
  }

  let ctx = Arc::new(WorkerContext {
    queue: Arc::clone(&queue),
    ss: Arc::clone(&ss),
    system: Arc::clone(&system),
    lang: Arc::clone(&lang),
    state: Arc::clone(&state),
    modal_tx,
    ss_sem: Semaphore::new(n_disc),
    modal_sem: Semaphore::new(1),
    remaining: Arc::new(AtomicUsize::new(remaining_count)),
    interrupted: Arc::clone(&interrupted),
    debug_log_path,
  });

  // Enqueue all steps that are Pending with wait_for == 0.
  // For a fresh run: always step 0 for each ROM.
  // For a resumed run: whatever steps are ready after applying saved statuses.
  for rom_arc in all_roms.iter() {
    let rom = rom_arc.lock().unwrap();
    let ready: Vec<usize> = rom
      .pipeline
      .iter()
      .enumerate()
      .filter(|(_, step)| step.status == StepStatus::Pending && step.wait_for_count() == 0)
      .map(|(i, _)| i)
      .collect();
    drop(rom);
    for idx in ready {
      queue.push(Arc::clone(rom_arc), idx);
    }
  }

  // ── Launch workers ────────────────────────────────────────────────────

  let n_main = n_disc + N_EXTRA_MAIN_WORKERS;
  let mut handles: Vec<thread::JoinHandle<()>> = Vec::with_capacity(n_main + N_BLOCKING_WORKERS);

  for _ in 0..n_main {
    let ctx = Arc::clone(&ctx);
    handles.push(thread::spawn(move || worker::worker_loop_main(ctx)));
  }
  for _ in 0..N_BLOCKING_WORKERS {
    let ctx = Arc::clone(&ctx);
    handles.push(thread::spawn(move || worker::worker_loop_blocking(ctx)));
  }

  // Periodic flush. The state used to be written once, after every worker had joined,
  // so anything short of a clean exit or a Ctrl-C — a kill -9, an OOM, a power cut —
  // threw away the whole run: every ROM came back as new, was re-downloaded, and had
  // its pkgver bumped a second time for identical content.
  //
  // Flushing mid-run is safe because `SaveState` inserts one whole `RomStateEntry` at a
  // time under the mutex. A snapshot is therefore always a set of finished ROMs, never
  // half of one.
  let flushing = Arc::new(AtomicBool::new(true));
  let flusher = {
    let state = Arc::clone(&state);
    let state_path = state_path.clone();
    let flushing = Arc::clone(&flushing);
    thread::spawn(move || {
      let mut since_flush = 0u64;
      while flushing.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_millis(FLUSH_POLL_MS));
        since_flush += FLUSH_POLL_MS;
        if since_flush < FLUSH_INTERVAL_MS {
          continue;
        }
        since_flush = 0;
        // Serialise under the lock, write without it — workers keep running.
        //
        // The lock is taken poison-tolerantly: a handler that panics while holding the
        // state poisons it for the instant it takes `execute_step` to call
        // `clear_poison()`. Landing in that window would kill this thread and silently
        // end the periodic flushing for the rest of the run.
        let yaml = match state.lock().unwrap_or_else(|e| e.into_inner()).to_yaml() {
          Ok(yaml) => yaml,
          Err(_) => continue,
        };
        write_with_rotation(&state_path, &yaml).ok();
      }
    })
  };

  for h in handles {
    h.join().unwrap();
  }

  // ── Post-join ─────────────────────────────────────────────────────────

  flushing.store(false, Ordering::Relaxed);
  flusher.join().ok();

  // Flush accumulated ROM state to disk (partial on interrupt, complete otherwise).
  if let Err(e) = state.lock().unwrap().save_with_rotation(&state_path) {
    eprintln!("Warning: could not save state: {}", e);
  }

  if interrupted.load(Ordering::SeqCst) {
    let run_state = worker::collect_run_state(&all_roms);
    match worker::save_run_state(&system_name, &run_state) {
      Ok(()) => eprintln!(
        "Run state saved to {}.run.yml — resume with: rompom -s {}",
        system_name, system_name
      ),
      Err(e) => eprintln!("Warning: could not save run state: {}", e),
    }
    drop(ui);
    return;
  }

  // Clean up leftover run file from a previous interrupted run.
  fs::remove_file(&run_path).ok();

  // ── Step telemetry ─────────────────────────────────────────────────

  let mut duration_buckets: HashMap<StepKind, Vec<std::time::Duration>> = HashMap::new();
  for rom_arc in all_roms.iter() {
    let rom = rom_arc.lock().unwrap();
    for step in &rom.pipeline {
      if let (Some(start), Some(end)) = (step.started_at, step.finished_at) {
        let dur = end.checked_duration_since(start).unwrap_or_default();
        duration_buckets
          .entry(step.kind.clone())
          .or_default()
          .push(dur);
      }
    }
  }

  let step_avg_durations: Vec<(&'static str, std::time::Duration)> = [
    (StepKind::ComputeHashes, "ComputeHashes"),
    (StepKind::LookupSS, "LookupSS"),
    (StepKind::WaitModal, "WaitModal"),
    (StepKind::BuildPackage, "BuildPackage"),
    (StepKind::CopyRom, "CopyRom"),
    (StepKind::DownloadRom, "DownloadRom"),
    (StepKind::DownloadMedias, "DownloadMedias"),
    (StepKind::SaveState, "SaveState"),
  ]
  .into_iter()
  .filter_map(|(kind, label)| {
    let durations = duration_buckets.get(&kind)?;
    if durations.is_empty() {
      return None;
    }
    let avg = durations.iter().sum::<std::time::Duration>() / durations.len() as u32;
    Some((label, avg))
  })
  .collect();

  let mut summary = ui.summary();
  summary.step_avg_durations = step_avg_durations;
  drop(ui);
  summary.print();
}

#[cfg(test)]
mod tests {
  use super::*;

  /// An invalid glob in rompom.yml used to panic — under the TUI, so the message was
  /// painted over the interface and lost. The error has to name the offending pattern,
  /// otherwise the user has no way to tell which of their filters is at fault.
  #[test]
  fn compile_patterns_names_the_offending_pattern() {
    let err = compile_patterns(&["*.zip".to_string(), "[".to_string()])
      .expect_err("an unclosed character class is not a valid glob");
    assert!(
      err.contains("\"[\""),
      "error should quote the pattern: {err}"
    );
    assert!(err.contains("invalid filter pattern"));
  }

  /// Both spellings, in any case, so a script can say what reads best.
  #[test]
  fn parse_resume_flag_reads_both_answers() {
    for yes in ["yes", "y", "YES", "Yes", "true"] {
      assert_eq!(parse_resume_flag(Some(yes)), Ok(Some(true)), "{}", yes);
    }
    for no in ["no", "n", "NO", "No", "false"] {
      assert_eq!(parse_resume_flag(Some(no)), Ok(Some(false)), "{}", no);
    }
    assert_eq!(parse_resume_flag(None), Ok(None));
  }

  /// A typo must not silently become one of the two answers. `--resume=maybe` deciding
  /// on its own whether to replay an interrupted run is the worst of the three outcomes.
  #[test]
  fn parse_resume_flag_refuses_anything_else() {
    let err = parse_resume_flag(Some("maybe")).expect_err("maybe is not an answer");
    assert!(err.contains("maybe"), "got: {}", err);
    assert!(err.contains("yes"), "got: {}", err);
    assert!(parse_resume_flag(Some("")).is_err());
  }

  #[test]
  fn compile_patterns_accepts_the_usual_filters() {
    let patterns = compile_patterns(&["*.zip".to_string(), "*.chd".to_string()]).unwrap();
    assert_eq!(patterns.len(), 2);
    assert!(patterns[0].matches("Sonic.zip"));
    assert!(patterns[1].matches("Panzer Dragoon Saga (Disc 1).chd"));
    assert!(!patterns[0].matches("Sonic.7z"));
  }

  /// `*` matches path separators too, unless `require_literal_separator` is set — so
  /// `*.zip` happily matches a whole path. That is precisely why `collect_sources` feeds
  /// these patterns the base name and never the full path: a filter meant to select files
  /// in one folder would otherwise reach into subfolders as well. Pins the library
  /// behaviour the collection relies on.
  #[test]
  fn a_star_pattern_also_matches_path_separators() {
    let patterns = compile_patterns(&["*.zip".to_string()]).unwrap();
    assert!(patterns[0].matches("Sonic.zip"));
    assert!(patterns[0].matches("/roms/megadrive/Sonic.zip"));
  }
}
