mod conf;
mod emulationstation;
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
  io::{self, Write as _},
  path::Path,
  sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
  },
  thread,
};

use glob::Pattern;
use internet_archive::metadata::Metadata;
use screenscraper::ScreenScraper;

use crate::conf::{Conf, Source};
use crate::queue::{Semaphore, TaskQueue};
use crate::rom::{FolderSource, IaSource, Rom, RomSource, RomSourceData, StepKind, StepStatus};
use crate::state::SystemState;
use crate::ui::Ui;
use crate::worker::WorkerContext;

// ── Constants ──────────────────────────────────────────────────────────────

/// Extra main-pool workers beyond the SS-semaphore limit.
/// Keeps downloads and packaging running while SS slots are saturated.
const N_EXTRA_MAIN_WORKERS: usize = 8;
const N_BLOCKING_WORKERS: usize = 2;

fn print_usage(program: &str, opts: getopts::Options) {
  let brief = format!("Usage: {} -s SYSTEM", program);
  print!("{}", opts.usage(&brief));
}

// ── main ──────────────────────────────────────────────────────────────────

fn main() {
  let args: Vec<String> = env::args().collect();
  let program = args[0].clone();
  let mut opts = getopts::Options::new();

  let confdir = match dirs::config_dir() {
    Some(x) => x,
    None => {
      eprintln!("Failed to find user configuration dir");
      return;
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
  opts.optflag("h", "help", "print this help menu");

  let matches = match opts.parse(&args[1..]) {
    Ok(m) => m,
    Err(f) => panic!("{}", f),
  };

  if matches.opt_present("h") {
    print_usage(&program, opts);
    return;
  }

  if matches.opt_present("update-config") {
    let conf_path = format!("{}/rompom.yml", confdir.display());
    if let Err(e) = conf::Conf::update(&conf_path) {
      eprintln!("Error: {}", e);
      std::process::exit(1);
    }
    return;
  }

  let conf = match Conf::load(&format!("{}/rompom.yml", confdir.display())) {
    Ok(c) => c,
    Err(e) => {
      eprintln!("Error: {}", e);
      std::process::exit(1);
    }
  };

  let system_name = match matches.opt_str("s") {
    Some(x) => x,
    None => {
      print_usage(&program, opts);
      return;
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

  let system = match conf.find_system(&system_name) {
    Some(s) => s,
    None => {
      eprintln!("System '{}' not found in rompom.yml", system_name);
      return;
    }
  };

  let source = match system.source.clone() {
    Some(s) => s,
    None => {
      eprintln!(
        "System '{}' has no source configured in rompom.yml",
        system_name
      );
      return;
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
        print!(
          "Found interrupted run ({}/{} done). Resume? [Y/n]: ",
          done,
          s.roms.len()
        );
        io::stdout().flush().unwrap();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).unwrap();
        match answer.trim().to_lowercase().as_str() {
          "" | "y" | "yes" => Some(s),
          _ => {
            fs::remove_file(&run_path).ok();
            None
          }
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

  let interrupted = Arc::new(AtomicBool::new(false));
  let queue = TaskQueue::new();
  let ui = Ui::new(Arc::clone(&interrupted), Arc::clone(&queue));
  let mut sources: Vec<RomSourceData> = Vec::new();

  match &source {
    Source::InternetArchive(ia_items) => {
      for item in ia_items {
        ui.fetching_metadata(&item.item);
        let metadata = Arc::new(Metadata::get(&item.item).unwrap());

        for file in metadata.files.iter().filter(|f| {
          let filename = Path::new(&f.name).file_name().unwrap().to_str().unwrap();
          item
            .filter
            .iter()
            .any(|pat| Pattern::new(pat).unwrap().matches(filename))
        }) {
          let filename = Path::new(&file.name)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
          let rom_url = metadata
            .file_urls(&file.name)
            .unwrap()
            .into_iter()
            .next()
            .unwrap_or_default();
          sources.push(RomSourceData {
            file_name: file.name.clone(),
            filename,
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
          });
        }
      }
    }

    Source::Folder(folder) => {
      ui.fetching_metadata(&folder.path);
      let dir = Path::new(&folder.path);
      for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_file() {
          continue;
        }
        let filename = path.file_name().unwrap().to_str().unwrap().to_string();
        if !folder
          .filter
          .iter()
          .any(|pat| Pattern::new(pat).unwrap().matches(&filename))
        {
          continue;
        }
        sources.push(RomSourceData {
          file_name: path.to_str().unwrap().to_string(),
          filename,
          source: RomSource::Folder(FolderSource { local_path: path }),
        });
      }
    }
  }

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

  let ss = ScreenScraper::new(
    &conf.screenscraper.user.login,
    &conf.screenscraper.user.password,
    &conf.screenscraper.dev.login,
    &conf.screenscraper.dev.password,
  )
  .unwrap();

  let n_disc = ss.user_info.maxthreads as usize;
  let modal_tx = ui.modal_sender();
  let state_path = format!("{}.state.yml", system_name);
  let state = Arc::new(Mutex::new(SystemState::load(&state_path)));
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

  for h in handles {
    h.join().unwrap();
  }

  // ── Post-join ─────────────────────────────────────────────────────────

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
