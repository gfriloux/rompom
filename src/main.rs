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
use crate::rom::{
  DiscFile, FolderSource, IaSource, Rom, RomSource, RomSourceData, StepKind, StepStatus,
};
use crate::state::SystemState;
use crate::ui::Ui;
use crate::worker::WorkerContext;

// ── Constants ──────────────────────────────────────────────────────────────

/// Extra main-pool workers beyond the SS-semaphore limit.
/// Keeps downloads and packaging running while SS slots are saturated.
const N_EXTRA_MAIN_WORKERS: usize = 8;
const N_BLOCKING_WORKERS: usize = 2;

// ── Multi-disc grouping ───────────────────────────────────────────────────

/// Detect `(Disc N)` / `(Disk N)` / `(CD N)` patterns in a filename stem.
///
/// Scans all parenthesised groups (e.g. a stem may also contain `(USA)` before
/// the disc indicator) and returns `(base_name, disc_number)` for the last
/// matching group found, `None` if no disc indicator is present.
///
/// Examples:
/// - `"Enemy Zero (USA) (Disc 0)"` → `Some(("Enemy Zero (USA)", 0))`
/// - `"Panzer Dragoon Saga (Disc 1)"` → `Some(("Panzer Dragoon Saga", 1))`
fn disc_indicator(stem: &str) -> Option<(String, u32)> {
  let mut result: Option<(String, u32)> = None;
  let mut search_from = 0;

  while let Some(rel) = stem[search_from..].find('(') {
    let paren = search_from + rel;
    let after = &stem[paren + 1..];
    let lower = after.to_lowercase();

    let num_offset = if lower.starts_with("disc ") || lower.starts_with("disk ") {
      5
    } else if lower.starts_with("disc") || lower.starts_with("disk") {
      4
    } else if lower.starts_with("cd ") {
      3
    } else if lower.starts_with("cd") {
      2
    } else {
      search_from = paren + 1;
      continue;
    };

    let digits: String = after[num_offset..]
      .chars()
      .take_while(|c| c.is_ascii_digit())
      .collect();

    if let Ok(num) = digits.parse::<u32>() {
      let base = stem[..paren].trim_end().to_string();
      result = Some((base, num));
    }

    search_from = paren + 1;
  }

  result
}

/// Group multi-disc files into single `RomSourceData` entries.
///
/// Files whose stems match `(Disc N)` / `(Disk N)` / `(CD N)` and share the
/// same base name + extension are merged:
/// - Disc 1 becomes the primary entry (with the virtual `filename` = base + ext).
/// - Disc 2+ become `extra_discs` on that entry.
/// - Single-disc sources pass through unchanged.
fn group_multi_disc(sources: Vec<RomSourceData>) -> Vec<RomSourceData> {
  use std::collections::BTreeMap;

  // ── Step 1: classify each source ───────────────────────────────────────
  struct Parsed {
    source: RomSourceData,
    base: String, // stem without disc indicator
    disc: Option<u32>,
    ext: String,
  }

  let parsed: Vec<Parsed> = sources
    .into_iter()
    .map(|src| {
      let filename = src.filename.clone();
      let stem = Path::new(&filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&filename)
        .to_string();
      let ext = Path::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
      match disc_indicator(&stem) {
        Some((base, num)) => Parsed {
          source: src,
          base,
          disc: Some(num),
          ext,
        },
        None => Parsed {
          source: src,
          base: stem,
          disc: None,
          ext,
        },
      }
    })
    .collect();

  // ── Step 2: find groups (base, ext) that have more than one disc entry ──
  // Use a BTreeMap so the final output order is deterministic.
  let mut group_count: HashMap<(String, String), usize> = HashMap::new();
  for p in &parsed {
    if p.disc.is_some() {
      *group_count
        .entry((p.base.clone(), p.ext.clone()))
        .or_insert(0usize) += 1;
    }
  }
  let multi: HashMap<(String, String), ()> = group_count
    .into_iter()
    .filter(|(_, c)| *c > 1)
    .map(|(k, _)| (k, ()))
    .collect();

  if multi.is_empty() {
    // Fast path: no multi-disc games, return sources as-is.
    return parsed.into_iter().map(|p| p.source).collect();
  }

  // ── Step 3: collect groups and pass-through entries ─────────────────────
  // groups: (base, ext) → sorted Vec<(disc_num, Parsed)>
  let mut groups: BTreeMap<(String, String), Vec<(u32, Parsed)>> = BTreeMap::new();
  let mut passthrough: Vec<RomSourceData> = Vec::new();

  for p in parsed {
    let key = (p.base.clone(), p.ext.clone());
    if let Some(disc_num) = p.disc.filter(|_| multi.contains_key(&key)) {
      groups.entry(key).or_default().push((disc_num, p));
    } else {
      passthrough.push(p.source);
    }
  }

  // ── Step 4: merge each group into one RomSourceData ─────────────────────
  let mut result: Vec<RomSourceData> = passthrough;

  for ((base, ext), mut discs) in groups {
    // Sort by disc number so disc 1 is always first.
    discs.sort_by_key(|(n, _)| *n);

    // Virtual logical filename: base name + extension (no disc indicator).
    let virtual_filename = if ext.is_empty() {
      base.clone()
    } else {
      format!("{}.{}", base, ext)
    };

    // Build extra_discs from disc 2+.
    let extra_discs: Vec<DiscFile> = discs[1..]
      .iter()
      .map(|(_, p)| {
        let src = &p.source;
        match &src.source {
          RomSource::InternetArchive(ia) => DiscFile {
            file_name: src.file_name.clone(),
            filename: src.filename.clone(),
            rom_url: ia.rom_url.clone(),
            sha1: ia.sha1.clone(),
            md5: ia.md5.clone(),
            crc32: ia.crc32.clone(),
            size: ia.size,
            local_path: None,
          },
          RomSource::Folder(f) => DiscFile {
            file_name: src.file_name.clone(),
            filename: src.filename.clone(),
            rom_url: String::new(),
            sha1: None,
            md5: None,
            crc32: None,
            size: 0,
            local_path: Some(f.local_path.clone()),
          },
        }
      })
      .collect();

    // Primary entry = disc 1, with the virtual filename.
    let (_, primary) = discs.remove(0);
    let mut primary_source = primary.source;
    primary_source.filename = virtual_filename;
    primary_source.extra_discs = extra_discs;
    result.push(primary_source);
  }

  result
}

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
            extra_discs: Vec::new(),
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
          extra_discs: Vec::new(),
        });
      }
    }
  }

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
