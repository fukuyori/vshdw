use std::{
    collections::HashSet,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

use anyhow::{Context, Result, bail};
use chrono::Local;
use clap::{CommandFactory, Parser};
use filetime::{FileTime, set_file_mtime};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use serde::Deserialize;
use walkdir::{DirEntry, WalkDir};

const TRASH_DIR: &str = ".deleted";
static PRIORITY_LOWERED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Parser)]
#[command(version, about = "One-way directory mirroring tool")]
struct Args {
    /// TOML config file containing mirror jobs. Relative FILE names are resolved under the standard config directory.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Master-side directory. Used when --config is not specified.
    #[arg(long, value_name = "DIR")]
    source: Option<PathBuf>,

    /// Backup destination directory. Used when --config is not specified.
    #[arg(long, value_name = "DIR")]
    dest: Option<PathBuf>,

    /// Print planned copies and deletes without changing files.
    #[arg(long)]
    dry_run: bool,

    /// Move deleted destination entries into dest/.deleted instead of removing them.
    #[arg(long)]
    trash: bool,

    /// Copy and update files, but do not remove destination-only entries.
    #[arg(long)]
    no_delete: bool,

    /// Disable progress bars.
    #[arg(long)]
    no_progress: bool,

    /// Lower the process priority while mirroring.
    #[arg(long)]
    low_priority: bool,

    /// Honor .gitignore files found under the source directory.
    #[arg(long)]
    use_gitignore: bool,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    defaults: ConfigDefaults,
    jobs: Vec<JobConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigDefaults {
    dest_root: Option<PathBuf>,
    #[serde(default)]
    trash: bool,
    #[serde(default)]
    no_delete: bool,
    #[serde(default)]
    suppress_warnings: bool,
    #[serde(default)]
    low_priority: bool,
    #[serde(default)]
    use_gitignore: bool,
    include_subdirs: Option<bool>,
    #[serde(default)]
    dirs: Vec<PathBuf>,
    #[serde(default)]
    exclude_dirs: Vec<PathBuf>,
    #[serde(default)]
    dir_patterns: Vec<String>,
    #[serde(default)]
    exclude_dir_patterns: Vec<String>,
    #[serde(default)]
    files: Vec<PathBuf>,
    #[serde(default)]
    include_files: Vec<PathBuf>,
    #[serde(default)]
    exclude_files: Vec<PathBuf>,
    #[serde(default)]
    file_patterns: Vec<String>,
    #[serde(default)]
    include_file_patterns: Vec<String>,
    #[serde(default)]
    exclude_file_patterns: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    exclude_extensions: Vec<String>,
    #[serde(default)]
    extension_patterns: Vec<String>,
    #[serde(default)]
    exclude_extension_patterns: Vec<String>,
    max_size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct JobConfig {
    name: Option<String>,
    source: Option<PathBuf>,
    #[serde(default)]
    sources: Vec<PathBuf>,
    dest: Option<PathBuf>,
    dest_subdir: Option<PathBuf>,
    #[serde(default)]
    trash: Option<bool>,
    #[serde(default)]
    no_delete: Option<bool>,
    #[serde(default)]
    suppress_warnings: Option<bool>,
    #[serde(default)]
    low_priority: Option<bool>,
    #[serde(default)]
    use_gitignore: Option<bool>,
    #[serde(default)]
    include_subdirs: Option<bool>,
    #[serde(default)]
    dirs: Vec<PathBuf>,
    #[serde(default)]
    exclude_dirs: Vec<PathBuf>,
    #[serde(default)]
    dir_patterns: Vec<String>,
    #[serde(default)]
    exclude_dir_patterns: Vec<String>,
    #[serde(default)]
    files: Vec<PathBuf>,
    #[serde(default)]
    include_files: Vec<PathBuf>,
    #[serde(default)]
    exclude_files: Vec<PathBuf>,
    #[serde(default)]
    file_patterns: Vec<String>,
    #[serde(default)]
    include_file_patterns: Vec<String>,
    #[serde(default)]
    exclude_file_patterns: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    exclude_extensions: Vec<String>,
    #[serde(default)]
    extension_patterns: Vec<String>,
    #[serde(default)]
    exclude_extension_patterns: Vec<String>,
    max_size_bytes: Option<u64>,
}

#[derive(Debug)]
struct Job {
    name: String,
    source: PathBuf,
    dest: PathBuf,
    trash: bool,
    no_delete: bool,
    suppress_warnings: bool,
    low_priority: bool,
    include_subdirs: bool,
    filters: Filters,
}

#[derive(Debug, Default)]
struct Filters {
    exclude_dirs: ExcludeDirs,
    exclude_dir_patterns: Vec<Regex>,
    include_files: Vec<PathBuf>,
    exclude_files: Vec<PathBuf>,
    include_file_patterns: Vec<Regex>,
    exclude_file_patterns: Vec<Regex>,
    exclude_extensions: HashSet<String>,
    exclude_extension_patterns: Vec<Regex>,
    max_size_bytes: Option<u64>,
    gitignore: GitignoreRules,
}

#[derive(Debug, Default)]
struct GitignoreRules {
    files: Vec<GitignoreFile>,
}

#[derive(Debug)]
struct GitignoreFile {
    base: PathBuf,
    matcher: Gitignore,
}

#[derive(Debug, Default)]
struct ExcludeDirs {
    relative: Vec<PathBuf>,
    absolute: Vec<PathBuf>,
}

#[derive(Debug)]
enum Operation {
    MakeDir { path: PathBuf },
    Copy { source: PathBuf, dest: PathBuf },
    Delete { path: PathBuf },
    Trash { path: PathBuf, target: PathBuf },
}

#[derive(Debug, Default)]
struct Summary {
    copied: usize,
    deleted: usize,
    trashed: usize,
    made_dirs: usize,
    skipped: usize,
}

fn main() -> Result<()> {
    if std::env::args_os().len() == 1 {
        Args::command().print_help()?;
        println!();
        return Ok(());
    }

    let args = Args::parse();
    let jobs = load_jobs(&args)?;
    let mut total = Summary::default();

    for job in jobs {
        let summary = mirror(&job, &args)?;
        total.copied += summary.copied;
        total.deleted += summary.deleted;
        total.trashed += summary.trashed;
        total.made_dirs += summary.made_dirs;
        total.skipped += summary.skipped;
    }

    println!(
        "done: dirs={}, copied={}, deleted={}, trashed={}, skipped={}",
        total.made_dirs, total.copied, total.deleted, total.trashed, total.skipped
    );

    Ok(())
}

fn load_jobs(args: &Args) -> Result<Vec<Job>> {
    if args.trash && args.no_delete {
        bail!("--trash and --no-delete cannot be used together");
    }

    let config_path = args.config.clone().map(resolve_config_path).transpose()?;

    if let Some(config_path) = config_path {
        let text = read_config_file(&config_path)?;
        let config: ConfigFile = toml::from_str(&text)
            .with_context(|| format!("failed to parse config: {}", config_path.display()))?;
        let config_dir = config_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        if config.jobs.is_empty() {
            bail!("config must contain at least one [[jobs]] entry");
        }

        let mut jobs = Vec::new();

        for (index, job) in config.jobs.into_iter().enumerate() {
            let mut exclude_dirs = config.defaults.exclude_dirs.clone();
            exclude_dirs.extend(config.defaults.dirs.clone());
            exclude_dirs.extend(job.exclude_dirs);
            exclude_dirs.extend(job.dirs);

            let mut exclude_dir_patterns = config.defaults.exclude_dir_patterns.clone();
            exclude_dir_patterns.extend(config.defaults.dir_patterns.clone());
            exclude_dir_patterns.extend(job.exclude_dir_patterns);
            exclude_dir_patterns.extend(job.dir_patterns);

            let mut include_files = config.defaults.files.clone();
            include_files.extend(config.defaults.include_files.clone());
            include_files.extend(job.files);
            include_files.extend(job.include_files);

            let mut exclude_files = config.defaults.exclude_files.clone();
            exclude_files.extend(job.exclude_files);

            let mut include_file_patterns = config.defaults.include_file_patterns.clone();
            include_file_patterns.extend(config.defaults.file_patterns.clone());
            include_file_patterns.extend(job.include_file_patterns);
            include_file_patterns.extend(job.file_patterns);

            let mut exclude_file_patterns = config.defaults.exclude_file_patterns.clone();
            exclude_file_patterns.extend(job.exclude_file_patterns);

            let mut exclude_extensions = config.defaults.exclude_extensions.clone();
            exclude_extensions.extend(config.defaults.extensions.clone());
            exclude_extensions.extend(job.exclude_extensions);
            exclude_extensions.extend(job.extensions);

            let mut exclude_extension_patterns = config.defaults.exclude_extension_patterns.clone();
            exclude_extension_patterns.extend(config.defaults.extension_patterns.clone());
            exclude_extension_patterns.extend(job.exclude_extension_patterns);
            exclude_extension_patterns.extend(job.extension_patterns);

            let trash = args.trash || job.trash.unwrap_or(config.defaults.trash);
            let no_delete = args.no_delete || job.no_delete.unwrap_or(config.defaults.no_delete);
            let suppress_warnings = job
                .suppress_warnings
                .unwrap_or(config.defaults.suppress_warnings);
            let low_priority =
                args.low_priority || job.low_priority.unwrap_or(config.defaults.low_priority);
            let use_gitignore =
                args.use_gitignore || job.use_gitignore.unwrap_or(config.defaults.use_gitignore);
            let include_subdirs = job
                .include_subdirs
                .or(config.defaults.include_subdirs)
                .unwrap_or(true);

            let name = job.name.unwrap_or_else(|| format!("job-{}", index + 1));

            if trash && no_delete {
                bail!("job {name} cannot enable both trash and no_delete");
            }

            let sources = resolve_sources(config_dir, &name, job.source, job.sources)?;
            let dest = resolve_dest(
                config_dir,
                &name,
                config.defaults.dest_root.as_deref(),
                job.dest,
                job.dest_subdir,
            )?;
            let exclude_dir_patterns = compile_patterns(
                &exclude_dir_patterns,
                &format!("job {name} exclude_dir_patterns"),
            )?;
            let include_file_patterns = compile_patterns(
                &include_file_patterns,
                &format!("job {name} include_file_patterns"),
            )?;
            let exclude_file_patterns = compile_patterns(
                &exclude_file_patterns,
                &format!("job {name} exclude_file_patterns"),
            )?;
            let exclude_extension_patterns = compile_patterns(
                &exclude_extension_patterns,
                &format!("job {name} exclude_extension_patterns"),
            )?;

            for source in &sources {
                let source_name = source_dir_name(source)?;
                let gitignore = if use_gitignore {
                    build_gitignore(source, &name)?
                } else {
                    GitignoreRules::default()
                };
                let expanded_dest = if sources.len() == 1 {
                    dest.clone()
                } else {
                    dest.join(source_name)
                };
                let expanded_name = if sources.len() == 1 {
                    name.clone()
                } else {
                    format!("{}:{}", name, source_name.to_string_lossy())
                };

                jobs.push(Job {
                    name: expanded_name,
                    source: source.clone(),
                    dest: expanded_dest,
                    trash,
                    no_delete,
                    suppress_warnings,
                    low_priority,
                    include_subdirs,
                    filters: Filters {
                        exclude_dirs: normalize_exclude_dirs(source, exclude_dirs.clone())?,
                        exclude_dir_patterns: exclude_dir_patterns.clone(),
                        include_files: normalize_paths(include_files.clone()),
                        exclude_files: normalize_paths(exclude_files.clone()),
                        include_file_patterns: include_file_patterns.clone(),
                        exclude_file_patterns: exclude_file_patterns.clone(),
                        exclude_extensions: normalize_extensions(exclude_extensions.clone()),
                        exclude_extension_patterns: exclude_extension_patterns.clone(),
                        max_size_bytes: job.max_size_bytes.or(config.defaults.max_size_bytes),
                        gitignore,
                    },
                });
            }
        }

        return Ok(jobs);
    }

    let source = args.source.clone().with_context(|| {
            let default = standard_config_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "the standard config directory".to_string());
            format!("--source is required when --config is not specified. Relative config file names are resolved under {default}")
        })?;
    let dest = args
        .dest
        .clone()
        .context("--dest is required when --config is not specified")?;

    let filters = Filters {
        gitignore: if args.use_gitignore {
            build_gitignore(&source, "cli")?
        } else {
            GitignoreRules::default()
        },
        ..Filters::default()
    };

    Ok(vec![Job {
        name: "default".to_string(),
        source,
        dest,
        trash: args.trash,
        no_delete: args.no_delete,
        suppress_warnings: false,
        low_priority: args.low_priority,
        include_subdirs: true,
        filters,
    }])
}

fn read_config_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| {
        let resolved = absolute_path(path)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| path.display().to_string());

        format!(
            "failed to read config: {} (resolved: {}). Config files are not read from the current directory; use the standard config directory or an absolute path.",
            path.display(),
            resolved
        )
    })
}

fn resolve_config_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }

    Ok(standard_config_dir()
        .context("standard config directory is not available on this platform")?
        .join(path))
}

fn standard_config_dir() -> Option<PathBuf> {
    standard_config_dir_impl()
}

#[cfg(target_os = "macos")]
fn standard_config_dir_impl() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("vshdw")
    })
}

#[cfg(target_os = "windows")]
fn standard_config_dir_impl() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(|base| PathBuf::from(base).join("vshdw"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn standard_config_dir_impl() -> Option<PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("vshdw"));
    }

    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("vshdw"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
fn standard_config_dir_impl() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("vshdw"))
}

fn resolve_from(base: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn resolve_dest(
    base: &Path,
    job_name: &str,
    dest_root: Option<&Path>,
    dest: Option<PathBuf>,
    dest_subdir: Option<PathBuf>,
) -> Result<PathBuf> {
    if dest.is_some() && dest_subdir.is_some() {
        bail!("job {job_name} must specify either dest or dest_subdir, not both");
    }

    let dest_root = dest_root.map(|root| resolve_from(base, root.to_path_buf()));

    if let Some(subdir) = dest_subdir {
        if subdir.is_absolute() {
            bail!("job {job_name} dest_subdir must be a relative path");
        }

        let root = dest_root.as_ref().with_context(|| {
            format!("job {job_name} uses dest_subdir but defaults.dest_root is not set")
        })?;
        return Ok(root.join(subdir));
    }

    let dest = dest.with_context(|| format!("job {job_name} must specify dest or dest_subdir"))?;

    if dest.is_absolute() {
        Ok(dest)
    } else if let Some(root) = dest_root {
        Ok(root.join(dest))
    } else {
        Ok(base.join(dest))
    }
}

fn resolve_sources(
    base: &Path,
    job_name: &str,
    source: Option<PathBuf>,
    mut sources: Vec<PathBuf>,
) -> Result<Vec<PathBuf>> {
    if source.is_some() && !sources.is_empty() {
        bail!("job {job_name} must specify either source or sources, not both");
    }

    if let Some(source) = source {
        sources.insert(0, source);
    }

    if sources.is_empty() {
        bail!("job {job_name} must specify source or sources");
    }

    Ok(sources
        .into_iter()
        .map(|source| resolve_from(base, source))
        .collect())
}

fn source_dir_name(source: &Path) -> Result<&std::ffi::OsStr> {
    source
        .file_name()
        .filter(|name| !name.is_empty())
        .with_context(|| format!("source has no directory name: {}", source.display()))
}

fn build_gitignore(source: &Path, job_name: &str) -> Result<GitignoreRules> {
    let mut rules = GitignoreRules::default();

    if source.is_dir() {
        for entry in WalkDir::new(source).follow_links(false) {
            let entry = entry
                .with_context(|| format!("failed to scan .gitignore files for job {job_name}"))?;

            if entry.file_type().is_file() && entry.file_name() == ".gitignore" {
                let base = entry.path().parent().unwrap_or(source);
                let mut builder = GitignoreBuilder::new(base);

                if let Some(err) = builder.add(entry.path()) {
                    return Err(err).with_context(|| {
                        format!(
                            "failed to read .gitignore for job {job_name}: {}",
                            entry.path().display()
                        )
                    });
                }

                let matcher = builder.build().with_context(|| {
                    format!(
                        "failed to compile .gitignore rules for job {job_name}: {}",
                        entry.path().display()
                    )
                })?;
                let base = relative_path(base, source)?;

                rules.files.push(GitignoreFile { base, matcher });
            }
        }
    }

    rules
        .files
        .sort_by_key(|file| file.base.components().count());

    Ok(rules)
}

fn compile_patterns(patterns: &[String], label: &str) -> Result<Vec<Regex>> {
    patterns
        .iter()
        .map(|pattern| {
            Regex::new(pattern).with_context(|| format!("invalid regex in {label}: {pattern}"))
        })
        .collect()
}

fn normalize_exclude_dirs(source: &Path, dirs: Vec<PathBuf>) -> Result<ExcludeDirs> {
    let source = normalize_path(absolute_path(source)?);
    let mut result = ExcludeDirs::default();

    for dir in dirs {
        let dir = normalize_path(dir);
        if dir.as_os_str().is_empty() {
            continue;
        }

        if dir.is_absolute() {
            if let Ok(rel) = dir.strip_prefix(&source) {
                if !rel.as_os_str().is_empty() {
                    result.relative.push(normalize_path(rel.to_path_buf()));
                }
            }
            result.absolute.push(dir);
        } else {
            result.relative.push(dir);
        }
    }

    Ok(result)
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.components().collect()
}

fn normalize_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .map(normalize_path)
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

fn normalize_extensions(extensions: Vec<String>) -> HashSet<String> {
    extensions
        .into_iter()
        .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
        .collect()
}

fn mirror(job: &Job, args: &Args) -> Result<Summary> {
    lower_process_priority_if_requested(job.low_priority)?;
    validate_paths(&job.source, &job.dest, args.dry_run, job.include_subdirs)?;

    println!(
        "[{}] {} -> {}",
        job.name,
        job.source.display(),
        job.dest.display()
    );

    let (plan, skipped) = build_plan(job)?;
    let mut summary = Summary {
        skipped,
        ..Summary::default()
    };

    let show_operations = args.no_progress || args.dry_run;
    let progress = progress_bar(plan.len(), args.no_progress || args.dry_run);

    for operation in plan {
        progress.set_message(operation.progress_message());
        execute_operation(
            operation,
            args.dry_run,
            show_operations,
            job.suppress_warnings,
            &mut summary,
        )?;
        progress.inc(1);
    }

    progress.finish_and_clear();

    println!(
        "[{}] done: dirs={}, copied={}, deleted={}, trashed={}, skipped={}",
        job.name,
        summary.made_dirs,
        summary.copied,
        summary.deleted,
        summary.trashed,
        summary.skipped
    );

    Ok(summary)
}

fn lower_process_priority_if_requested(enabled: bool) -> Result<()> {
    if !enabled || PRIORITY_LOWERED.swap(true, Ordering::Relaxed) {
        return Ok(());
    }

    lower_process_priority()
}

#[cfg(unix)]
fn lower_process_priority() -> Result<()> {
    let result = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 10) };

    if result == -1 {
        return Err(io::Error::last_os_error()).context("failed to lower process priority");
    }

    Ok(())
}

#[cfg(windows)]
fn lower_process_priority() -> Result<()> {
    use windows_sys::Win32::System::Threading::{
        BELOW_NORMAL_PRIORITY_CLASS, GetCurrentProcess, SetPriorityClass,
    };

    let result = unsafe { SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS) };

    if result == 0 {
        return Err(io::Error::last_os_error()).context("failed to lower process priority");
    }

    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn lower_process_priority() -> Result<()> {
    Ok(())
}

fn validate_paths(source: &Path, dest: &Path, dry_run: bool, include_subdirs: bool) -> Result<()> {
    let source = source
        .canonicalize()
        .with_context(|| format!("source does not exist: {}", source.display()))?;

    if !source.is_dir() {
        bail!("source is not a directory: {}", source.display());
    }

    if !dest.exists() && !dry_run {
        fs::create_dir_all(dest)
            .with_context(|| format!("failed to create dest: {}", dest.display()))?;
    }

    let dest = if dest.exists() {
        dest.canonicalize()
            .with_context(|| format!("dest does not exist: {}", dest.display()))?
    } else {
        absolute_path(dest)?
    };

    if dest.exists() && !dest.is_dir() {
        bail!("dest is not a directory: {}", dest.display());
    }

    if source == dest {
        bail!("source and dest must be different directories");
    }

    // When include_subdirs is false, only top-level regular files are mirrored
    // and the source walk never descends into subdirectories, so a dest nested
    // inside source cannot be read back as source content (no copy loop).
    // The reverse (source nested inside dest) is still unsafe: the dest deletion
    // walk would see the source directory as a stray entry, so keep rejecting it.
    let dest_under_source = dest.starts_with(&source);
    let source_under_dest = source.starts_with(&dest);

    if source_under_dest || (dest_under_source && include_subdirs) {
        bail!("source and dest must not be nested");
    }

    Ok(())
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    Ok(std::env::current_dir()
        .context("failed to read current directory")?
        .join(path))
}

fn build_plan(job: &Job) -> Result<(Vec<Operation>, usize)> {
    let mut plan = Vec::new();
    let mut skipped = 0;
    let mut source_entries = HashSet::new();
    let mut protected_source_paths = HashSet::new();

    collect_source_operations(
        job,
        &mut source_entries,
        &mut protected_source_paths,
        &mut plan,
        &mut skipped,
    )?;

    if !job.no_delete {
        collect_delete_operations(
            job,
            &source_entries,
            &protected_source_paths,
            &mut plan,
            &mut skipped,
        )?;
    }

    Ok((plan, skipped))
}

fn collect_source_operations(
    job: &Job,
    source_entries: &mut HashSet<PathBuf>,
    protected_source_paths: &mut HashSet<PathBuf>,
    plan: &mut Vec<Operation>,
    skipped: &mut usize,
) -> Result<()> {
    for entry in WalkDir::new(&job.source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            should_descend(entry, &job.source, &job.filters, false, job.include_subdirs)
        })
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                if !job.suppress_warnings {
                    warn_walk_error("source", &job.source, &err);
                }
                protect_walk_error_path(&job.source, &err, protected_source_paths);
                *skipped += 1;
                continue;
            }
        };
        let source_path = entry.path();

        if source_path == job.source {
            continue;
        }

        let rel = relative_path(source_path, &job.source)?;
        let dest_path = job.dest.join(&rel);

        source_entries.insert(rel.clone());

        if entry.file_type().is_dir() {
            if !dest_path.is_dir() {
                plan.push(Operation::MakeDir { path: dest_path });
            }
        } else if entry.file_type().is_file() {
            if job.filters.excludes_file(&rel, source_path)? {
                *skipped += 1;
            } else if needs_copy(source_path, &dest_path)? {
                plan.push(Operation::Copy {
                    source: source_path.to_path_buf(),
                    dest: dest_path,
                });
            }
        } else {
            *skipped += 1;
        }
    }

    Ok(())
}

fn collect_delete_operations(
    job: &Job,
    source_entries: &HashSet<PathBuf>,
    protected_source_paths: &HashSet<PathBuf>,
    plan: &mut Vec<Operation>,
    skipped: &mut usize,
) -> Result<()> {
    if !job.dest.exists() {
        return Ok(());
    }

    let mut entries = Vec::new();

    for entry in WalkDir::new(&job.dest)
        .contents_first(false)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            should_descend(entry, &job.dest, &job.filters, true, job.include_subdirs)
        })
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                if !job.suppress_warnings {
                    warn_walk_error("dest", &job.dest, &err);
                }
                *skipped += 1;
                continue;
            }
        };
        let path = entry.path();

        if path == job.dest {
            continue;
        }

        let rel = relative_path(path, &job.dest)?;

        if job.trash && is_in_trash(&rel) {
            continue;
        }

        if job.filters.excludes_dest_path(&rel, path)? {
            continue;
        }

        entries.push((rel, path.to_path_buf()));
    }

    entries.sort_by_key(|(rel, _)| rel.components().count());

    let mut handled_dirs = HashSet::new();
    let trash_root = job.trash.then(|| {
        job.dest
            .join(TRASH_DIR)
            .join(Local::now().format("%Y%m%d-%H%M%S").to_string())
    });

    for (rel, path) in entries {
        if source_entries.contains(&rel)
            || is_protected_source_path(&rel, protected_source_paths)
            || is_under_handled_dir(&rel, &handled_dirs)
        {
            continue;
        }

        if path.is_dir() {
            handled_dirs.insert(rel.clone());
        }

        if let Some(trash_root) = &trash_root {
            plan.push(Operation::Trash {
                path,
                target: trash_root.join(rel),
            });
        } else {
            plan.push(Operation::Delete { path });
        }
    }

    Ok(())
}

fn should_descend(
    entry: &DirEntry,
    root: &Path,
    filters: &Filters,
    skip_trash: bool,
    include_subdirs: bool,
) -> bool {
    let path = entry.path();

    if path == root {
        return true;
    }

    let Ok(rel) = path.strip_prefix(root) else {
        return true;
    };

    if skip_trash && is_in_trash(rel) {
        return false;
    }

    if !include_subdirs && entry.file_type().is_dir() {
        return false;
    }

    !entry.file_type().is_dir() || !filters.excludes_dir(rel, path)
}

fn warn_walk_error(kind: &str, root: &Path, err: &walkdir::Error) {
    let path = err
        .path()
        .map(Path::display)
        .map(|path| path.to_string())
        .unwrap_or_else(|| root.display().to_string());

    eprintln!("warning: skipped unreadable {kind} path: {path}: {err}");
}

fn protect_walk_error_path(
    root: &Path,
    err: &walkdir::Error,
    protected_paths: &mut HashSet<PathBuf>,
) {
    let Some(path) = err.path() else {
        return;
    };

    if let Ok(rel) = path.strip_prefix(root)
        && !rel.as_os_str().is_empty()
    {
        protected_paths.insert(rel.to_path_buf());
    }
}

fn is_protected_source_path(rel: &Path, protected_paths: &HashSet<PathBuf>) -> bool {
    protected_paths
        .iter()
        .any(|protected| rel == protected || rel.starts_with(protected))
}

fn relative_path(path: &Path, root: &Path) -> Result<PathBuf> {
    path.strip_prefix(root)
        .with_context(|| format!("failed to make relative path: {}", path.display()))
        .map(Path::to_path_buf)
}

impl Filters {
    fn excludes_dir(&self, rel: &Path, path: &Path) -> bool {
        self.exclude_dirs
            .relative
            .iter()
            .any(|dir| relative_dir_matches(rel, dir))
            || self.matches_dir_pattern(rel)
            || self.matches_gitignore(rel, true)
            || absolute_path(path)
                .map(normalize_path)
                .ok()
                .is_some_and(|path| {
                    self.exclude_dirs
                        .absolute
                        .iter()
                        .any(|dir| path == *dir || path.starts_with(dir))
                        || self.matches_dir_pattern(&path)
                })
    }

    fn excludes_file(&self, rel: &Path, path: &Path) -> Result<bool> {
        if self.excludes_dir(rel, path)
            || !self.includes_file(rel, path)
            || self.matches_exclude_file(rel, path)
            || self.matches_file_pattern(rel, path)
            || self.matches_gitignore(rel, false)
            || self.excludes_extension(path)
        {
            return Ok(true);
        }

        if let Some(limit) = self.max_size_bytes {
            let size = match path.metadata() {
                Ok(meta) => meta.len(),
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(true),
                Err(err) => {
                    return Err(err)
                        .with_context(|| format!("failed to read metadata: {}", path.display()));
                }
            };
            if size > limit {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn excludes_dest_path(&self, rel: &Path, path: &Path) -> Result<bool> {
        if self.excludes_dir(rel, path)
            || (path.is_file()
                && (!self.includes_file(rel, path)
                    || self.matches_exclude_file(rel, path)
                    || self.matches_file_pattern(rel, path)
                    || self.matches_gitignore(rel, false)
                    || self.excludes_extension(path)))
        {
            return Ok(true);
        }

        if path.is_file()
            && let Some(limit) = self.max_size_bytes
        {
            let size = match path.metadata() {
                Ok(meta) => meta.len(),
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(true),
                Err(err) => {
                    return Err(err)
                        .with_context(|| format!("failed to read metadata: {}", path.display()));
                }
            };
            if size > limit {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn excludes_extension(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext = ext.trim_start_matches('.');
                self.exclude_extensions.contains(&ext.to_ascii_lowercase())
                    || self
                        .exclude_extension_patterns
                        .iter()
                        .any(|pattern| pattern.is_match(ext))
            })
            .unwrap_or(false)
    }

    fn matches_dir_pattern(&self, path: &Path) -> bool {
        if self.exclude_dir_patterns.is_empty() {
            return false;
        }

        path_matches_patterns(&self.exclude_dir_patterns, path)
    }

    fn matches_gitignore(&self, rel: &Path, is_dir: bool) -> bool {
        if !is_dir && self.gitignore_ignored_ancestor(rel) {
            return true;
        }

        let mut ignored = false;

        for file in &self.gitignore.files {
            let matched_path = if file.base.as_os_str().is_empty() {
                Some(rel)
            } else {
                rel.strip_prefix(&file.base).ok()
            };

            let Some(matched_path) = matched_path else {
                continue;
            };

            let matched = file.matcher.matched(matched_path, is_dir);
            if matched.is_ignore() {
                ignored = true;
            } else if matched.is_whitelist() {
                ignored = false;
            }
        }

        ignored
    }

    fn gitignore_ignored_ancestor(&self, rel: &Path) -> bool {
        rel.ancestors()
            .skip(1)
            .filter(|ancestor| !ancestor.as_os_str().is_empty())
            .any(|ancestor| self.matches_gitignore(ancestor, true))
    }

    fn matches_file_pattern(&self, rel: &Path, path: &Path) -> bool {
        if self.exclude_file_patterns.is_empty() {
            return false;
        }

        path_matches_patterns(&self.exclude_file_patterns, rel)
            || absolute_path(path)
                .map(normalize_path)
                .ok()
                .is_some_and(|path| path_matches_patterns(&self.exclude_file_patterns, &path))
            || path.file_name().is_some_and(|name| {
                self.exclude_file_patterns
                    .iter()
                    .any(|pattern| pattern.is_match(&name.to_string_lossy()))
            })
    }

    fn includes_file(&self, rel: &Path, path: &Path) -> bool {
        if self.include_files.is_empty() && self.include_file_patterns.is_empty() {
            return true;
        }

        self.matches_include_file(rel, path) || self.matches_include_file_pattern(rel, path)
    }

    fn matches_include_file(&self, rel: &Path, path: &Path) -> bool {
        path_matches_literals(&self.include_files, rel, path)
    }

    fn matches_exclude_file(&self, rel: &Path, path: &Path) -> bool {
        path_matches_literals(&self.exclude_files, rel, path)
    }

    fn matches_include_file_pattern(&self, rel: &Path, path: &Path) -> bool {
        path_matches_patterns(&self.include_file_patterns, rel)
            || absolute_path(path)
                .map(normalize_path)
                .ok()
                .is_some_and(|path| path_matches_patterns(&self.include_file_patterns, &path))
            || path.file_name().is_some_and(|name| {
                self.include_file_patterns
                    .iter()
                    .any(|pattern| pattern.is_match(&name.to_string_lossy()))
            })
    }
}

fn relative_dir_matches(rel: &Path, exclude_dir: &Path) -> bool {
    if rel == exclude_dir || rel.starts_with(exclude_dir) {
        return true;
    }

    if exclude_dir.components().count() != 1 {
        return false;
    }

    rel.components().any(
        |component| matches!(component, Component::Normal(name) if name == exclude_dir.as_os_str()),
    )
}

fn path_matches_literals(literals: &[PathBuf], rel: &Path, path: &Path) -> bool {
    literals.iter().any(|file| {
        rel == file
            || path
                .file_name()
                .is_some_and(|name| name == file.as_os_str())
            || (file.is_absolute()
                && absolute_path(path)
                    .map(normalize_path)
                    .ok()
                    .is_some_and(|path| path == *file))
    })
}

fn path_matches_patterns(patterns: &[Regex], path: &Path) -> bool {
    let raw = path.to_string_lossy();
    let slash = raw.replace(['\\', '¥'], "/");
    let yen = slash.replace('/', "¥");
    let backslash = slash.replace('/', "\\");

    patterns.iter().any(|pattern| {
        pattern.is_match(&raw)
            || pattern.is_match(&slash)
            || pattern.is_match(&yen)
            || pattern.is_match(&backslash)
    })
}

impl Operation {
    fn progress_message(&self) -> &'static str {
        match self {
            Operation::MakeDir { .. } => "mkdir",
            Operation::Copy { .. } => "copy",
            Operation::Delete { .. } => "delete",
            Operation::Trash { .. } => "trash",
        }
    }
}

fn execute_operation(
    operation: Operation,
    dry_run: bool,
    show_operation: bool,
    suppress_warnings: bool,
    summary: &mut Summary,
) -> Result<()> {
    match operation {
        Operation::MakeDir { path } => {
            if show_operation {
                println!("mkdir: {}", path.display());
            }
            if !dry_run {
                if path.exists() && !path.is_dir() {
                    remove_path(&path)?;
                }
                fs::create_dir_all(&path)
                    .with_context(|| format!("failed to create directory: {}", path.display()))?;
            }
            summary.made_dirs += 1;
        }
        Operation::Copy { source, dest } => {
            if show_operation {
                println!("copy: {} -> {}", source.display(), dest.display());
            }
            if !dry_run {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create parent directory: {}", parent.display())
                    })?;
                }

                if dest.exists() && dest.is_dir() {
                    remove_path(&dest)?;
                }

                match fs::copy(&source, &dest) {
                    Ok(_) => {}
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        warn_skipped_transient_source(&source, suppress_warnings);
                        summary.skipped += 1;
                        return Ok(());
                    }
                    Err(err) => {
                        return Err(err).with_context(|| {
                            format!("failed to copy {} to {}", source.display(), dest.display())
                        });
                    }
                }
                preserve_modified_time(&source, &dest)?;
            }
            summary.copied += 1;
        }
        Operation::Delete { path } => {
            if show_operation {
                println!("delete: {}", path.display());
            }
            if !dry_run {
                if !remove_path_for_delete(&path, suppress_warnings)? {
                    summary.skipped += 1;
                    return Ok(());
                }
            }
            summary.deleted += 1;
        }
        Operation::Trash { path, target } => {
            if show_operation {
                println!("trash: {} -> {}", path.display(), target.display());
            }
            if !dry_run {
                move_to_trash(&path, &target)?;
            }
            summary.trashed += 1;
        }
    }

    Ok(())
}

fn warn_skipped_transient_source(path: &Path, suppress_warnings: bool) {
    if !suppress_warnings {
        eprintln!(
            "warning: skipped source path that disappeared during copy: {}",
            path.display()
        );
    }
}

fn warn_skipped_delete(path: &Path, err: &anyhow::Error, suppress_warnings: bool) {
    if !suppress_warnings {
        eprintln!(
            "warning: skipped destination path that could not be deleted: {}: {err:#}",
            path.display()
        );
    }
}

fn progress_bar(len: usize, disabled: bool) -> ProgressBar {
    let progress = if disabled {
        ProgressBar::hidden()
    } else {
        ProgressBar::new(len as u64)
    };

    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("#>-");

    progress.set_style(style);
    progress
}

fn needs_copy(source_path: &Path, dest_path: &Path) -> Result<bool> {
    if !dest_path.exists() {
        return Ok(true);
    }

    if !dest_path.is_file() {
        return Ok(true);
    }

    let source_meta = match source_path.metadata() {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read metadata: {}", source_path.display()));
        }
    };
    let dest_meta = match dest_path.metadata() {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read metadata: {}", dest_path.display()));
        }
    };

    if source_meta.len() != dest_meta.len() {
        return Ok(true);
    }

    let source_modified = modified_time(&source_meta)?;
    let dest_modified = modified_time(&dest_meta)?;

    Ok(source_modified
        .duration_since(dest_modified)
        .is_ok_and(|diff| diff.as_secs() >= 1))
}

fn modified_time(meta: &fs::Metadata) -> Result<SystemTime> {
    meta.modified().context("failed to read modified time")
}

fn preserve_modified_time(source_path: &Path, dest_path: &Path) -> Result<()> {
    let source_meta = match source_path.metadata() {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read metadata: {}", source_path.display()));
        }
    };
    let modified = FileTime::from_last_modification_time(&source_meta);

    set_file_mtime(dest_path, modified)
        .with_context(|| format!("failed to set modified time: {}", dest_path.display()))
}

fn is_in_trash(rel: &Path) -> bool {
    matches!(
        rel.components().next(),
        Some(Component::Normal(name)) if name == TRASH_DIR
    )
}

fn is_under_handled_dir(rel: &Path, handled_dirs: &HashSet<PathBuf>) -> bool {
    rel.ancestors()
        .skip(1)
        .any(|ancestor| handled_dirs.contains(ancestor))
}

fn move_to_trash(path: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create trash directory: {}", parent.display()))?;
    }

    fs::rename(path, target).or_else(|_| {
        if path.is_dir() {
            copy_dir_all(path, target)?;
            fs::remove_dir_all(path)?;
        } else {
            fs::copy(path, target)?;
            fs::remove_file(path)?;
        }
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    let file_type = path
        .symlink_metadata()
        .with_context(|| format!("failed to read metadata: {}", path.display()))?
        .file_type();

    if file_type.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory: {}", path.display()))?;
    } else {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove file: {}", path.display()))?;
    }

    Ok(())
}

fn remove_path_for_delete(path: &Path, suppress_warnings: bool) -> Result<bool> {
    match remove_path(path) {
        Ok(()) => Ok(true),
        Err(err) => {
            if err
                .chain()
                .find_map(|cause| cause.downcast_ref::<io::Error>())
                .is_some_and(|err| err.kind() == io::ErrorKind::NotFound)
            {
                return Ok(false);
            }

            warn_skipped_delete(path, &err, suppress_warnings);
            Ok(false)
        }
    }
}

fn copy_dir_all(source: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)
        .with_context(|| format!("failed to create directory: {}", dest.display()))?;

    for entry in WalkDir::new(source).follow_links(false) {
        let entry =
            entry.with_context(|| format!("failed to read directory: {}", source.display()))?;
        let path = entry.path();

        if path == source {
            continue;
        }

        let rel = relative_path(path, source)?;
        let target = dest.join(rel);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create directory: {}", target.display()))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory: {}", parent.display()))?;
            }
            fs::copy(path, &target).with_context(|| {
                format!("failed to copy {} to {}", path.display(), target.display())
            })?;
            preserve_modified_time(path, &target)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclude_dir_name_matches_nested_directory() {
        assert!(relative_dir_matches(
            Path::new("project/node_modules"),
            Path::new("node_modules")
        ));
        assert!(relative_dir_matches(
            Path::new("project/node_modules/package"),
            Path::new("node_modules")
        ));
        assert!(relative_dir_matches(
            Path::new("target"),
            Path::new("target")
        ));
        assert!(relative_dir_matches(
            Path::new("Code/logs/20260617T175753/window1/server.log"),
            Path::new("logs")
        ));
    }

    #[test]
    fn exclude_dir_name_does_not_match_partial_component() {
        assert!(!relative_dir_matches(
            Path::new("project/node_modules_backup"),
            Path::new("node_modules")
        ));
        assert!(!relative_dir_matches(
            Path::new("project/mytarget"),
            Path::new("target")
        ));
    }

    #[test]
    fn exclude_relative_dir_path_matches_from_source_root() {
        assert!(relative_dir_matches(
            Path::new("foo/bar/cache"),
            Path::new("foo/bar")
        ));
        assert!(!relative_dir_matches(
            Path::new("project/foo/bar"),
            Path::new("foo/bar")
        ));
    }

    #[test]
    fn relative_dest_uses_config_dir_without_dest_root() {
        let dest = resolve_dest(
            Path::new("/configs/vshdw"),
            "docs",
            None,
            Some(PathBuf::from("Documents")),
            None,
        )
        .unwrap();

        assert_eq!(dest, PathBuf::from("/configs/vshdw/Documents"));
    }

    #[test]
    fn relative_dest_uses_dest_root_when_set() {
        let dest = resolve_dest(
            Path::new("/configs/vshdw"),
            "docs",
            Some(Path::new("/Volumes/Backup")),
            Some(PathBuf::from("Documents")),
            None,
        )
        .unwrap();

        assert_eq!(dest, PathBuf::from("/Volumes/Backup/Documents"));
    }

    #[test]
    fn absolute_dest_overrides_dest_root() {
        let dest = resolve_dest(
            Path::new("/configs/vshdw"),
            "docs",
            Some(Path::new("/Volumes/Backup")),
            Some(PathBuf::from("/Other/Backup/Documents")),
            None,
        )
        .unwrap();

        assert_eq!(dest, PathBuf::from("/Other/Backup/Documents"));
    }

    #[test]
    fn dest_subdir_requires_dest_root() {
        let err = resolve_dest(
            Path::new("/configs/vshdw"),
            "docs",
            None,
            None,
            Some(PathBuf::from("Documents")),
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("job docs uses dest_subdir but defaults.dest_root is not set")
        );
    }

    #[test]
    fn gitignore_excludes_root_rules() {
        let root = test_temp_dir("gitignore-root");
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join(".gitignore"), "target/\n*.log\n!important.log\n").unwrap();

        let filters = Filters {
            gitignore: build_gitignore(&root, "test").unwrap(),
            ..Filters::default()
        };

        assert!(filters.excludes_dir(Path::new("target"), &root.join("target")));
        assert!(
            filters
                .excludes_file(Path::new("debug.log"), &root.join("debug.log"))
                .unwrap()
        );
        assert!(
            !filters
                .excludes_file(Path::new("important.log"), &root.join("important.log"))
                .unwrap()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gitignore_excludes_nested_rules_from_their_directory() {
        let root = test_temp_dir("gitignore-nested");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/.gitignore"), "*.tmp\n").unwrap();

        let filters = Filters {
            gitignore: build_gitignore(&root, "test").unwrap(),
            ..Filters::default()
        };

        assert!(
            filters
                .excludes_file(Path::new("src/cache.tmp"), &root.join("src/cache.tmp"))
                .unwrap()
        );
        assert!(
            !filters
                .excludes_file(Path::new("cache.tmp"), &root.join("cache.tmp"))
                .unwrap()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gitignore_ignored_parent_keeps_children_ignored() {
        let root = test_temp_dir("gitignore-parent");
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        fs::write(root.join("target/.gitignore"), "!keep.log\n").unwrap();

        let filters = Filters {
            gitignore: build_gitignore(&root, "test").unwrap(),
            ..Filters::default()
        };

        assert!(
            filters
                .excludes_file(Path::new("target/keep.log"), &root.join("target/keep.log"))
                .unwrap()
        );

        let _ = fs::remove_dir_all(root);
    }

    fn test_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("vshdw-{name}-{}-{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
}
