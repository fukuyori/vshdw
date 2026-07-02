# vshdw

English | [Japanese](README.ja.md)

**vshdw** is a one-way directory mirroring tool that reproduces the current state of a master directory into a backup directory on another disk. The name is shortened from `volume shadow` and is pronounced "vee shadow". The destination is treated as a shadow volume of the source, not as a simple copy bucket.

vshdw treats the source side as authoritative. Files that exist only in the source are copied to the destination. Files that exist on both sides are compared by file size and modified time; vshdw copies the source file again when the source is newer or the size differs. Files and directories deleted from the source are also deleted from the destination. As a result, the destination stays in the same shape as the source and does not keep old unnecessary files.

This is different from append-only backup tools. Append-only backups may keep files that existed in the past. vshdw mirrors the current source state, so the destination is a replica of the current working area, not a history archive. The goal is not to preserve everything that ever existed. The goal is to keep the same directory state on another disk.

## Usage

When using a config file, write the source directories, destinations, and filtering rules in TOML.

Config files are stored in the standard per-OS config directory. vshdw does not define a default config filename.

| OS | Directory |
| --- | --- |
| macOS | `~/Library/Application Support/vshdw/` |
| Windows | `%APPDATA%\vshdw\` |
| Linux | `$XDG_CONFIG_HOME/vshdw/` |
| Linux fallback | `~/.config/vshdw/` |

Running `vshdw` without arguments prints help. To run with a config file, pass the filename to `--config`.

```bash
vshdw --config default.toml
```

A relative config filename is resolved under the standard config directory, not the current directory.

```bash
vshdw --config photos.toml
```

On macOS, this reads `~/Library/Application Support/vshdw/photos.toml`. On Windows, it reads `%APPDATA%\vshdw\photos.toml`. On Linux, it reads `$XDG_CONFIG_HOME/vshdw/photos.toml` or `~/.config/vshdw/photos.toml`.

vshdw does not read config files from the current directory. `--config default.toml` is always treated as a filename inside the standard config directory.

To use a config file outside the standard config directory, pass an absolute path.

```bash
vshdw --config /path/to/vshdw.toml
```

Print the version with `--version`.

```bash
vshdw --version
```

## Configuration

Config files use TOML. Put common settings under `[defaults]`, and define one or more mirroring jobs with `[[jobs]]`.

The repository includes `vshdw.example.toml` as a template. To start from it, copy it to the standard config directory and edit it.

macOS:

```bash
mkdir -p "$HOME/Library/Application Support/vshdw"
cp vshdw.example.toml "$HOME/Library/Application Support/vshdw/default.toml"
vshdw --config default.toml
```

Linux:

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/vshdw"
cp vshdw.example.toml "${XDG_CONFIG_HOME:-$HOME/.config}/vshdw/default.toml"
vshdw --config default.toml
```

Windows PowerShell:

```powershell
New-Item -ItemType Directory -Force "$env:APPDATA\vshdw"
Copy-Item vshdw.example.toml "$env:APPDATA\vshdw\default.toml"
vshdw --config default.toml
```

```toml
[defaults]
dest_root = "E:\\Backup"
trash = true
no_delete = false
suppress_warnings = false
low_priority = false
use_gitignore = true
include_subdirs = true
include_files = []
include_file_patterns = []
exclude_dirs = ["target", "node_modules", ".git"]
exclude_dir_patterns = ['(^|[\\/¥])\.cache($|[\\/¥])']
exclude_files = [".DS_Store", "Thumbs.db"]
exclude_file_patterns = []
exclude_extensions = ["tmp", "log"]
exclude_extension_patterns = ['^bak[0-9]+$']
max_size_bytes = 1073741824

[[jobs]]
name = "documents"
source = "D:\\Data"
dest = "Data"

[[jobs]]
name = "projects"
sources = ["D:\\Projects", "D:\\Labs"]
dest_subdir = "Projects"
exclude_dirs = ["target", "dist"]
exclude_extensions = ["tmp"]
```

### defaults

`[defaults]` is optional. Values written here are applied to all `[[jobs]]`. When the same kind of setting also appears in a job, `include_*`, `exclude_*`, and `*_patterns` values are merged, while `trash`, `no_delete`, `suppress_warnings`, `low_priority`, `use_gitignore`, `include_subdirs`, and `max_size_bytes` are overridden by the job value.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `trash` | boolean | `false` | Move deleted destination entries into `.deleted/<timestamp>/` under the destination instead of deleting them permanently. |
| `no_delete` | boolean | `false` | Do not delete files or directories that exist only in the destination. |
| `suppress_warnings` | boolean | `false` | Suppress `warning:` messages for unreadable skipped paths. Skipping and deletion protection still happen normally. |
| `low_priority` | boolean | `false` | Lower the vshdw process priority. macOS/Linux use a nice value; Windows uses a process priority class. |
| `use_gitignore` | boolean | `false` | Read `.gitignore` files under the source and exclude matching files and directories from mirroring. |
| `include_subdirs` | boolean | `true` | Include subdirectories. If `false`, only direct files under `source` are mirrored. |
| `dest_root` | string | unset | Common destination root. When set, relative `dest` and `dest_subdir` values are resolved under this directory. |
| `include_files` | string array | `[]` | Files to include. Values are literal filenames, relative paths, or absolute paths, not regular expressions. Empty means include all files. |
| `include_file_patterns` | string array | `[]` | Regular expressions for files to include. They are tested against filenames, relative paths, and absolute paths. |
| `exclude_dirs` | string array | `[]` | Directories to exclude. Values are literal directory names, paths relative to `source`, or absolute paths, not regular expressions. |
| `exclude_dir_patterns` | string array | `[]` | Regular expressions for directories to exclude. They are tested against both relative and absolute paths. |
| `exclude_files` | string array | `[]` | Files to exclude. Values are literal filenames, relative paths, or absolute paths, not regular expressions. |
| `exclude_file_patterns` | string array | `[]` | Regular expressions for files to exclude. They are tested against filenames, relative paths, and absolute paths. |
| `exclude_extensions` | string array | `[]` | File extensions to exclude. A leading `.` may be included or omitted. |
| `exclude_extension_patterns` | string array | `[]` | Regular expressions for extensions to exclude. They are tested only against the final extension part. |
| `max_size_bytes` | integer | unset | Exclude files larger than this size, in bytes. |

`trash` and `no_delete` cannot both be enabled. Any configuration that makes both values `true` is an error.

If `[defaults]` has `trash = true` and a job has `no_delete = true`, that job would enable both options and fail.

```toml
[defaults]
trash = true

[[jobs]]
name = "copy-only"
source = "/Users/me/Inbox"
dest = "/Volumes/Backup/Inbox"
no_delete = true
```

In that case, put `trash = true` only on jobs that should use trash.

```toml
[[jobs]]
name = "mirror-with-trash"
source = "/Users/me/Documents"
dest = "/Volumes/Backup/Documents"
trash = true

[[jobs]]
name = "copy-only"
source = "/Users/me/Inbox"
dest = "/Volumes/Backup/Inbox"
no_delete = true
```

### jobs

At least one `[[jobs]]` entry is required. One job describes one source-to-destination mirror. To back up multiple directories, write multiple jobs.

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | no | Name shown in logs. If omitted, vshdw uses `job-1`, `job-2`, and so on. |
| `source` | string | either `source` or `sources` | Master directory. vshdw treats this side as authoritative. |
| `sources` | string array | either `source` or `sources` | Multiple master directories. Each source is mirrored into `dest/<source-directory-name>/`. |
| `dest` | string | either `dest` or `dest_subdir` | Backup destination directory. Absolute paths are used as-is. Relative paths are resolved under `defaults.dest_root` when it is set, or relative to the config file directory otherwise. |
| `dest_subdir` | string | either `dest` or `dest_subdir` | Subdirectory under `defaults.dest_root`. It cannot be used together with `dest`. |
| `trash` | boolean | no | Override `trash` for this job. |
| `no_delete` | boolean | no | Override `no_delete` for this job. |
| `suppress_warnings` | boolean | no | Override warning suppression for this job. |
| `low_priority` | boolean | no | Lower the vshdw process priority before this job runs. |
| `use_gitignore` | boolean | no | Override whether this job honors `.gitignore` files. |
| `include_subdirs` | boolean | no | Override whether subdirectories are included for this job. |
| `include_files` | string array | no | Add literal files to include for this job. Merged with defaults. |
| `include_file_patterns` | string array | no | Add include-file regular expressions for this job. Merged with defaults. |
| `exclude_dirs` | string array | no | Add literal directories to exclude for this job. Merged with defaults. |
| `exclude_dir_patterns` | string array | no | Add exclude-directory regular expressions for this job. Merged with defaults. |
| `exclude_files` | string array | no | Add literal files to exclude for this job. Merged with defaults. |
| `exclude_file_patterns` | string array | no | Add exclude-file regular expressions for this job. Merged with defaults. |
| `exclude_extensions` | string array | no | Add literal extensions to exclude for this job. Merged with defaults. |
| `exclude_extension_patterns` | string array | no | Add exclude-extension regular expressions for this job. Merged with defaults. |
| `max_size_bytes` | integer | no | Override the size limit for this job. |

`no_delete` can be set per job. `[defaults].no_delete` is the default for all jobs, and a job-level `no_delete` overrides it. The CLI `--no-delete` option disables deletion for all jobs.

```toml
[defaults]
no_delete = false

[[jobs]]
name = "mirror"
source = "/Users/me/Documents"
dest = "/Volumes/Backup/Documents"

[[jobs]]
name = "copy-only"
source = "/Users/me/Inbox"
dest = "/Volumes/Backup/Inbox"
no_delete = true
```

Set `suppress_warnings = true` to hide warning messages. This only suppresses `warning:` output for unreadable skipped paths. The skipped count remains in the summary, and destination-side deletion protection for unreadable source paths still works.

```toml
[defaults]
suppress_warnings = true

[[jobs]]
name = "application-support"
source = "/Users/me/Library/Application Support"
dest = "/Volumes/Backup/Application Support"
```

Set `low_priority = true` to lower process priority. This can make copy work less disruptive, but it may take longer to finish. Priority is changed at the OS process level, so vshdw does not restore it between jobs.

```toml
[defaults]
low_priority = true
```

Relative `source` values, and relative `dest` values when `defaults.dest_root` is not set, are resolved relative to the directory that contains the config file. When `defaults.dest_root` is set, relative `dest` and `dest_subdir` values are resolved under that root. Absolute `dest` values are used as-is even when `defaults.dest_root` is set. If `exclude_dirs` contains only a directory name such as `node_modules`, it applies to matching directories at any depth under the source. If it contains a relative path such as `foo/bar`, it is treated as a path relative to `source`. If it contains an absolute path, that path itself is excluded. If the absolute path is under `source`, the corresponding relative path is also excluded so the matching destination path is protected from deletion.

Excluded files and directories are not copied. If they already exist in the destination, they are also excluded from deletion. For example, with `exclude_extensions = ["log"]`, `.log` files on the source side are not copied, and existing `.log` files on the destination side are not deleted. When `use_gitignore = true`, files and directories matched by `.gitignore` are excluded from both copying and deletion in the same way.

Regular expressions use Rust's `regex` syntax and are supported only by `include_file_patterns`, `exclude_dir_patterns`, `exclude_file_patterns`, and `exclude_extension_patterns`. In TOML, single-quoted literal strings such as `'\.cache'` avoid double escaping backslashes. `exclude_dir_patterns` are tested against both relative and absolute paths. `include_file_patterns` and `exclude_file_patterns` are tested against filenames, relative paths, and absolute paths. During matching, vshdw tests path variants using `/`, `\`, and `¥` separators to absorb Windows and macOS/Linux separator differences. `exclude_extension_patterns` are tested against only the final extension, such as `gz`, not the whole suffix `tar.gz`. Invalid regular expressions cause startup errors.

For normal path strings, Windows uses `\`, and macOS/Linux use `/`. In TOML double-quoted strings, Windows backslashes must be escaped as `\\`.

```toml
source = "C:\\Users\\me\\Data"
```

In TOML single-quoted literal strings, Windows paths can be written as-is.

```toml
source = 'C:\Users\me\Data'
```

macOS/Linux paths use `/`.

```toml
source = "/Users/me/Data"
source = "/home/me/Data"
```

When writing directory separators in regular expressions, use a character class that matches `/`, `\`, and `¥`.

```toml
exclude_dir_patterns = ['(^|[\\/¥])node_modules($|[\\/¥])']
```

To include only files whose names start with `.`, use:

```toml
include_file_patterns = ['(^|[\\/¥])\.[^\\/¥]+$']
```

When using `sources`, vshdw does not merge all source directories directly into the same destination directory. Instead, each source is placed under a subdirectory named after the source directory. For example, with `sources = ["/data/docs", "/data/photos"]` and `dest = "/backup"`, the destinations are `/backup/docs` and `/backup/photos`.

If `include_subdirs = false`, vshdw mirrors only files directly under `source`. Directories under `source` and their contents are not copied, and destination-side subdirectories are excluded from deletion.

### examples

Minimal config:

```toml
[[jobs]]
source = "/Users/me/Documents"
dest = "/Volumes/Backup/Documents"
```

Multiple directories with common exclusions:

```toml
[defaults]
dest_root = "/Volumes/Backup"
trash = true
use_gitignore = true
include_subdirs = true
include_files = []
include_file_patterns = []
exclude_dirs = [".git", "target", "node_modules"]
exclude_dir_patterns = ['(^|[\\/¥])\.cache($|[\\/¥])']
exclude_files = [".DS_Store", "Thumbs.db"]
exclude_file_patterns = []
exclude_extensions = ["tmp", "log"]
exclude_extension_patterns = ['^bak[0-9]+$']
max_size_bytes = 1073741824

[[jobs]]
name = "documents"
source = "/Users/me/Documents"
dest = "Documents"

[[jobs]]
name = "projects"
sources = ["/Users/me/Projects", "/Users/me/Labs"]
dest_subdir = "Projects"
exclude_dirs = ["dist", "build"]
```

The `projects` job above mirrors into `/Volumes/Backup/Projects/Projects` and `/Volumes/Backup/Projects/Labs`.

Exclude a directory by absolute path:

```toml
[[jobs]]
name = "photos"
source = "/Users/me/Pictures"
dest = "/Volumes/Backup/Pictures"
exclude_dirs = ["/Users/me/Pictures/RawCache"]
```

Use regular expressions for directory exclusion, included files, and extension exclusion:

```toml
[[jobs]]
name = "work"
source = "/Users/me/Work"
dest = "/Volumes/Backup/Work"
exclude_dir_patterns = ['(^|[\\/¥])\.venv($|[\\/¥])', '(^|[\\/¥])cache-[0-9]+($|[\\/¥])']
include_file_patterns = ['(^|[\\/¥])\.[^\\/¥]+$']
exclude_extension_patterns = ['^(tmp|bak[0-9]+)$']
```

Mirror only direct files, without subdirectories:

```toml
[[jobs]]
name = "desktop-files"
source = "/Users/me/Desktop"
dest = "/Volumes/Backup/Desktop"
include_subdirs = false
```

For one-off runs, `--source` and `--dest` can be passed directly without a config file.

```bash
vshdw --source D:\Data --dest E:\Backup\Data
```

This copies, updates, and deletes destination entries to match the source.

Mirroring with deletion is dangerous. If a file is accidentally deleted from the source, vshdw treats that deletion as the correct state and reflects it to the destination. Use `--dry-run` to preview copy and delete operations without changing files.

```bash
vshdw --source D:\Data --dest E:\Backup\Data --dry-run
```

Use `--trash` to move destination-only entries into `.deleted` under the destination instead of deleting them immediately.

```bash
vshdw --source D:\Data --dest E:\Backup\Data --trash
```

This preserves some recovery room while still aligning the destination to the source. Use `--no-delete` to temporarily disable deletion.

```bash
vshdw --source D:\Data --dest E:\Backup\Data --no-delete
```

During execution, vshdw shows a progress bar based on the planned operation count. Normal runs do not print per-file messages such as `copy:`. Pass `--no-progress` to show detailed operation logs. In `--dry-run`, vshdw lists planned operations without applying changes.

```bash
vshdw --config default.toml --no-progress
```

Pass `--low-priority` to lower process priority for a one-off run.

```bash
vshdw --config default.toml --low-priority
```

Pass `--use-gitignore` to honor `.gitignore` files under each source. With `--config`, this enables `.gitignore` handling for every job.

```bash
vshdw --config default.toml --use-gitignore
```

## Comparison

vshdw initially prioritizes practicality and speed. It compares files by size and modified time. A file is copied when it does not exist in the destination, when the size differs, or when the source modified time is newer. After copying, vshdw sets the destination modified time to match the source, which avoids repeatedly copying unchanged files. Hash comparison may be added later for stricter checks.

## Safety

vshdw refuses to run when `source` and `dest` are the same directory or nested inside each other. This prevents accidental recursion and unintended deletes when the destination is inside the mirror target.

If a source path cannot be read because of permissions or similar errors, vshdw prints a warning and skips that path. The corresponding destination-side path is also excluded from deletion. This avoids deleting backup data just because the source path was unreadable.

Some files, such as log files or database temporary files, may disappear after directory scanning but before copying. In that case, vshdw skips the file and continues instead of failing.

If a destination-only file cannot be deleted because it disappeared or the OS refuses removal, vshdw skips that delete operation and continues. The skipped operation is counted in the summary. Set `suppress_warnings = true` to hide the warning output.

When copying from macOS to an external volume, AppleDouble files such as `._000413.log` may be created temporarily. To exclude these files, use:

```toml
exclude_file_patterns = ['(^|[\\/¥])\._']
```

## Goal

vshdw is a small tool for keeping working directories, development projects, document folders, photos, and reference data in the same state on another disk. It is not cloud sync or a history management system. It creates a reliable local shadow of the current state: a lightweight, understandable one-way mirroring CLI written in Rust.
