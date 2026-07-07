# vshdw

日本語 | [English](README.md)

**vshdw** は、マスター側ディレクトリの現在の状態を、別ディスク上のバックアップディレクトリへそのまま再現する一方向ミラーリングツールである。名称は `volume shadow` を縮めたもので、読みは「ブイシャドウ」とする。バックアップ先を単なるコピー置き場ではなく、マスター側の内容を映した「影のボリューム」として扱う点に特徴がある。

vshdw の基本動作は、マスター側を正とし、バックアップ側をそれに完全に合わせることである。マスター側に存在するファイルがバックアップ側に存在しない場合はコピーする。両方に存在するファイルについては、ファイルサイズと更新日時を比較し、マスター側の方が新しい、またはサイズが異なる場合にコピーし直す。そして、マスター側で削除されたファイルやディレクトリは、バックアップ側からも削除する。これにより、バックアップ側には古い不要ファイルが残らず、常にマスター側と同じ構成が保たれる。

このアプリは、通常の「追加保存型バックアップ」とは異なる。追加保存型では、過去に存在したファイルがバックアップ側に残り続けることがある。一方、vshdw は現在のマスター側を忠実に反映するため、バックアップ側は履歴保管庫ではなく、現在の作業領域の複製として機能する。つまり、目的は「過去のすべてを保存すること」ではなく、「別ディスクに同じ状態のディレクトリを維持すること」である。

## Usage

設定ファイルを使う場合は、コピーするディレクトリとコピー先、除外条件を TOML で記述する。

設定ファイルの標準フォルダは、各 OS の慣習に合わせて次のパスとする。標準のファイル名は決めない。

| OS | Directory |
| --- | --- |
| macOS | `~/Library/Application Support/vshdw/` |
| Windows | `%APPDATA%\vshdw\` |
| Linux | `$XDG_CONFIG_HOME/vshdw/` |
| Linux fallback | `~/.config/vshdw/` |

引数なしで実行した場合、`vshdw` はヘルプを表示する。設定ファイルを使って実行する場合は、`--config` にファイル名を指定する。

```bash
vshdw --config default.toml
```

相対ファイル名は、カレントディレクトリではなく標準設定フォルダ内のファイル名として解釈する。

```bash
vshdw --config photos.toml
```

この例では、macOS なら `~/Library/Application Support/vshdw/photos.toml`、Windows なら `%APPDATA%\vshdw\photos.toml`、Linux なら `$XDG_CONFIG_HOME/vshdw/photos.toml` または `~/.config/vshdw/photos.toml` を読む。

カレントディレクトリの設定ファイルは使用しない。`--config default.toml` のような相対指定も、カレントディレクトリではなく標準設定フォルダ内のファイル名として解釈する。

標準設定フォルダ以外の設定ファイルを使う場合は、`--config` に絶対パスを指定する。

```bash
vshdw --config /path/to/vshdw.toml
```

バージョンは `--version` で確認できる。

```bash
vshdw --version
```

## Configuration

設定ファイルは TOML 形式で記述する。全体共通の設定は `[defaults]` に書き、実際のミラーリング対象は `[[jobs]]` に 1 件以上書く。

リポジトリには雛形として `vshdw.example.toml` を含めている。まず手元用の設定ファイルを作る場合は、標準の設定ファイル置き場へコピーして編集する。

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

`[defaults]` は省略可能である。ここに書いた値はすべての `[[jobs]]` に適用される。ジョブ側にも同じ種類の設定を書いた場合、`include_*`、`exclude_*` と各 `*_patterns` は defaults と job の内容を結合し、`trash`、`no_delete`、`suppress_warnings`、`low_priority`、`use_gitignore`、`include_subdirs`、`max_size_bytes` は job 側の値を優先する。

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `trash` | boolean | `false` | 削除対象を完全削除せず、バックアップ先の `.deleted/<timestamp>/` に退避する。 |
| `no_delete` | boolean | `false` | バックアップ先だけに存在するファイルやディレクトリを削除しない。 |
| `suppress_warnings` | boolean | `false` | 権限不足などで読み取れないパスをスキップしたときの `warning:` 表示を抑制する。スキップ処理と削除保護は通常どおり行う。 |
| `low_priority` | boolean | `false` | vshdw プロセスの実行優先度を下げる。macOS/Linux では nice 値、Windows ではプロセス優先度クラスを使う。 |
| `use_gitignore` | boolean | `false` | source 配下の各 `.gitignore` を読み、マッチしたファイルやディレクトリを同期対象から外す。 |
| `include_subdirs` | boolean | `true` | サブディレクトリを同期対象に含める。`false` の場合は `source` 直下のファイルだけを対象にする。 |
| `dest_root` | string | unset | バックアップ先の共通ルート。これを指定すると、相対 `dest` と `dest_subdir` はこの配下のパスとして扱う。 |
| `include_files` | string array | `[]` | 除外条件に一致しても強制的にコピー対象へ戻すファイル。正規表現ではなく、ファイル名、相対パス、または絶対パスで指定する。ホワイトリストではなく、除外の上書きとして働く。 |
| `include_file_patterns` | string array | `[]` | 除外条件に一致しても強制的にコピー対象へ戻すファイルの正規表現。ファイル名、相対パス、絶対パスに対して判定する。ホワイトリストではなく、除外の上書きとして働く。 |
| `exclude_dirs` | string array | `[]` | 同期対象から外すディレクトリ。正規表現ではなく、ディレクトリ名、`source` から見た相対パス、または絶対パスで指定する。 |
| `exclude_dir_patterns` | string array | `[]` | 同期対象から外すディレクトリの正規表現。相対パスと絶対パスの両方に対して判定する。 |
| `exclude_files` | string array | `[]` | 同期対象から外すファイル。正規表現ではなく、ファイル名、相対パス、または絶対パスで指定する。 |
| `exclude_file_patterns` | string array | `[]` | 同期対象から外すファイルの正規表現。ファイル名、相対パス、絶対パスに対して判定する。 |
| `exclude_extensions` | string array | `[]` | 同期対象から外す拡張子。正規表現ではなく、先頭の `.` は書いても書かなくてもよい。 |
| `exclude_extension_patterns` | string array | `[]` | 同期対象から外す拡張子の正規表現。拡張子部分だけに対して判定する。 |
| `max_size_bytes` | integer | unset | このサイズを超えるファイルを同期対象から外す。単位はバイト。 |

`trash` と `no_delete` は同時に有効化できない。両方が `true` になる設定はエラーになる。

`[defaults]` に `trash = true` を書いた状態で、個別 job に `no_delete = true` を書くと、その job では両方が有効になってしまうためエラーになる。

```toml
[defaults]
trash = true

[[jobs]]
name = "copy-only"
source = "/Users/me/Inbox"
dest = "/Volumes/Backup/Inbox"
no_delete = true
```

この場合は、`trash` を defaults ではなく削除退避したい job 側にだけ書く。

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

`[[jobs]]` は 1 件以上必要である。1 つの job が 1 組のミラーリング元とミラーリング先を表す。複数のディレクトリをバックアップしたい場合は、`[[jobs]]` を複数書く。

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | no | ログ表示用の名前。省略時は `job-1`、`job-2` のように自動設定される。 |
| `source` | string | `source` または `sources` のどちらかが必要 | マスター側ディレクトリ。vshdw はこの内容を正とする。 |
| `sources` | string array | `source` または `sources` のどちらかが必要 | 複数のマスター側ディレクトリ。各 source は `dest/<sourceディレクトリ名>/` に展開して同期される。 |
| `dest` | string | `dest` または `dest_subdir` のどちらかが必要 | バックアップ先ディレクトリ。絶対パスの場合はそのまま使う。相対パスの場合、`defaults.dest_root` があればその配下、なければ設定ファイルのあるディレクトリからの相対パスとして扱う。 |
| `dest_subdir` | string | `dest` または `dest_subdir` のどちらかが必要 | `defaults.dest_root` 配下のサブディレクトリ。`dest` と同時には指定できない。 |
| `trash` | boolean | no | この job だけ `trash` の設定を上書きする。 |
| `no_delete` | boolean | no | この job だけ `no_delete` の設定を上書きする。 |
| `suppress_warnings` | boolean | no | この job だけ警告表示の抑制設定を上書きする。 |
| `low_priority` | boolean | no | この job の実行前に vshdw プロセスの実行優先度を下げる。 |
| `use_gitignore` | boolean | no | この job だけ `.gitignore` の使用有無を上書きする。 |
| `include_subdirs` | boolean | no | この job だけサブディレクトリを含めるかどうかを上書きする。 |
| `include_files` | string array | no | この job に追加する対象ファイル。正規表現なしで指定し、defaults の値と結合される。 |
| `include_file_patterns` | string array | no | この job に追加する対象ファイル正規表現。defaults の値と結合される。 |
| `exclude_dirs` | string array | no | この job に追加する除外ディレクトリ。正規表現なしで指定し、defaults の値と結合される。 |
| `exclude_dir_patterns` | string array | no | この job に追加する除外ディレクトリ正規表現。defaults の値と結合される。 |
| `exclude_files` | string array | no | この job に追加する除外ファイル。正規表現なしで指定し、defaults の値と結合される。 |
| `exclude_file_patterns` | string array | no | この job に追加する除外ファイル正規表現。defaults の値と結合される。 |
| `exclude_extensions` | string array | no | この job に追加する除外拡張子。正規表現なしで指定し、defaults の値と結合される。 |
| `exclude_extension_patterns` | string array | no | この job に追加する除外拡張子正規表現。defaults の値と結合される。 |
| `max_size_bytes` | integer | no | この job だけサイズ上限を上書きする。 |

`no_delete` は job ごとに設定できる。`[defaults]` の `no_delete` は全 job の既定値になり、各 `[[jobs]]` の `no_delete` がそれを上書きする。CLI の `--no-delete` を指定した場合は、すべての job で削除反映を無効にする。

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

警告表示を抑制したい場合は `suppress_warnings = true` を設定する。これは権限不足などで読み取れないパスをスキップしたときの `warning:` 表示だけを消す。スキップ数は summary に残り、読み取れなかった source パスに対応する dest 側の削除保護も通常どおり行う。

```toml
[defaults]
suppress_warnings = true

[[jobs]]
name = "application-support"
source = "/Users/me/Library/Application Support"
dest = "/Volumes/Backup/Application Support"
```

実行優先度を下げたい場合は `low_priority = true` を設定する。コピー処理が他の作業を妨げにくくなるが、完了までの時間は長くなる場合がある。優先度は OS のプロセス単位で下がるため、複数 job の途中で元に戻す動作はしない。

```toml
[defaults]
low_priority = true
```

設定ファイル内の相対 `source` と、`defaults.dest_root` がない場合の相対 `dest` は、設定ファイルが置かれているディレクトリからの相対パスとして解釈される。`defaults.dest_root` を指定した場合、相対 `dest` と `dest_subdir` はその配下のパスとして扱う。絶対 `dest` は `defaults.dest_root` があってもそのまま使う。`exclude_dirs` に `node_modules` のようなディレクトリ名だけを書いた場合は、source 配下のどの階層にある同名ディレクトリにも適用される。`foo/bar` のような相対パスを書いた場合は、`source` から見た相対ディレクトリとして扱う。絶対パスを書いた場合は、そのパス自体を除外する。絶対パスが `source` 配下にある場合は、対応する相対パスも除外扱いになるため、バックアップ先の同じ位置にあるディレクトリも削除対象から外れる。

除外されたファイルやディレクトリはコピーされず、バックアップ先に存在していても削除対象から外される。例えば `exclude_extensions = ["log"]` の場合、`source` 側の `.log` ファイルはコピーされず、`dest` 側に既にある `.log` ファイルも削除されない。`use_gitignore = true` の場合、`.gitignore` にマッチしたファイルやディレクトリも同じようにコピー対象と削除対象から外される。

`include_files` または `include_file_patterns` に明示一致したファイルは、`exclude_dirs`、`exclude_files`、`exclude_extensions`、`.gitignore` より優先される。除外ディレクトリの内側にあるファイルでも、include に一致すればコピー対象になる。ただし `max_size_bytes` は上限として引き続き適用される。

正規表現は Rust の `regex` 構文で指定し、`include_file_patterns`、`exclude_dir_patterns`、`exclude_file_patterns`、`exclude_extension_patterns` だけで使う。TOML では `'\.cache'` のような single-quoted literal string を使うと、バックスラッシュを二重に書かずに済む。`exclude_dir_patterns` は相対パスと絶対パスの両方に対して判定する。`include_file_patterns` と `exclude_file_patterns` はファイル名、相対パス、絶対パスに対して判定する。判定時には同じパスを `/` 区切り、`\` 区切り、`¥` 区切りの候補として扱うため、Windows と macOS/Linux の区切り文字の違いを吸収できる。`exclude_extension_patterns` は `tar.gz` 全体ではなく、最後の拡張子 `gz` のような拡張子部分だけに対して判定する。無効な正規表現がある場合、vshdw は起動時にエラーを返す。

通常のパス文字列では、Windows は `\`、macOS/Linux は `/` を使う。TOML の double-quoted string で Windows パスを書く場合は `\` を `\\` にする。

```toml
source = "C:\\Users\\me\\Data"
```

TOML の single-quoted literal string なら、Windows パスもそのまま書ける。

```toml
source = 'C:\Users\me\Data'
```

macOS/Linux のパスは `/` のまま書く。

```toml
source = "/Users/me/Data"
source = "/home/me/Data"
```

ディレクトリ区切りを正規表現で明示する場合は、`/`、`\`、`¥` のどれにもマッチする文字クラスを書くと分かりやすい。

```toml
exclude_dir_patterns = ['(^|[\\/¥])node_modules($|[\\/¥])']
```

除外条件に一致しても、`.` で始まるファイルは必ずコピー対象へ戻す例は次のとおりである。

```toml
include_file_patterns = ['(^|[\\/¥])\.[^\\/¥]+$']
```

`sources` を使う場合、複数の source を同じ dest 直下に直接混ぜるのではなく、各 source のディレクトリ名でサブディレクトリを作る。例えば `sources = ["/data/docs", "/data/photos"]`、`dest = "/backup"` の場合、同期先は `/backup/docs` と `/backup/photos` になる。

`include_subdirs = false` の場合、`source` 直下のファイルだけを同期する。`source` 直下のディレクトリとその中身はコピーされず、`dest` 側に存在するサブディレクトリも削除対象から外される。

### examples

最小構成は次のとおりである。

```toml
[[jobs]]
source = "/Users/me/Documents"
dest = "/Volumes/Backup/Documents"
```

複数のディレクトリを同期し、共通の除外条件を設定する例は次のとおりである。

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

上の `projects` は、`/Volumes/Backup/Projects/Projects` と `/Volumes/Backup/Projects/Labs` に分けて同期する。

絶対パスで除外ディレクトリを指定する例は次のとおりである。

```toml
[[jobs]]
name = "photos"
source = "/Users/me/Pictures"
dest = "/Volumes/Backup/Pictures"
exclude_dirs = ["/Users/me/Pictures/RawCache"]
```

正規表現でディレクトリ除外、対象ファイル、拡張子除外を指定する例は次のとおりである。

```toml
[[jobs]]
name = "work"
source = "/Users/me/Work"
dest = "/Volumes/Backup/Work"
exclude_dir_patterns = ['(^|[\\/¥])\.venv($|[\\/¥])', '(^|[\\/¥])cache-[0-9]+($|[\\/¥])']
include_file_patterns = ['(^|[\\/¥])\.[^\\/¥]+$']
exclude_extension_patterns = ['^(tmp|bak[0-9]+)$']
```

サブディレクトリを含めず、直下のファイルだけを同期する例は次のとおりである。

```toml
[[jobs]]
name = "desktop-files"
source = "/Users/me/Desktop"
dest = "/Volumes/Backup/Desktop"
include_subdirs = false
```

単発実行では、設定ファイルを使わずに `--source` と `--dest` を直接指定できる。

```bash
vshdw --source D:\Data --dest E:\Backup\Data
```

このコマンドを実行すると、コピー、更新、削除を行い、バックアップ先をマスター側と同じ状態に揃える。

削除を伴うミラーリングには危険もある。マスター側で誤ってファイルを削除した場合、その削除も正しい変更としてバックアップ側に反映されるためである。そのため、vshdw では安全確認のための `--dry-run` を用意する。`--dry-run` を指定した場合、実際のコピーや削除は行わず、どのファイルがコピーされ、どのファイルが削除されるかだけを表示する。

```bash
vshdw --source D:\Data --dest E:\Backup\Data --dry-run
```

削除対象を即座に完全削除するのではなく、バックアップ先の `.deleted` ディレクトリへ退避する `--trash` オプションも用意する。

```bash
vshdw --source D:\Data --dest E:\Backup\Data --trash
```

これにより、マスター側との同期を保ちながら、誤削除に対する一定の保険を残すことができる。必要に応じて、削除反映を一時的に無効化する `--no-delete` も利用できる。

```bash
vshdw --source D:\Data --dest E:\Backup\Data --no-delete
```

実行中は、計画した操作数をもとに進捗状況をプログレスバーで表示する。通常実行では `copy:` などのファイルごとのメッセージは表示しない。コピー、削除、ゴミ箱移動などの詳細ログを出したい場合は `-v` または `--verbose` を指定する。`--dry-run` の場合は、実際の変更を行わずに操作予定を一覧表示する。

```bash
vshdw --config default.toml -v
```

プログレスバーだけを消したい場合は `--no-progress` を指定する。

単発実行や一時的な実行で優先度を下げたい場合は `--low-priority` を指定する。

```bash
vshdw --config default.toml --low-priority
```

source 配下の `.gitignore` を使いたい場合は `--use-gitignore` を指定する。設定ファイル実行時に指定すると、すべての job で `.gitignore` が有効になる。

```bash
vshdw --config default.toml --use-gitignore
```

## Comparison

vshdw の比較処理は、まず実用性と高速性を重視し、ファイルサイズと更新日時を基準にする。バックアップ側にファイルが存在しない場合、サイズが異なる場合、またはマスター側の更新日時が新しい場合にコピー対象とする。コピー後はバックアップ側の更新日時をマスター側に合わせるため、変更されていないファイルを毎回コピーし直すことを避ける。将来的には、より厳密な比較が必要な場合に備えて、ハッシュ比較機能を追加できるようにする。

## Safety

vshdw は、source と dest が同一ディレクトリ、または入れ子の関係にある場合は実行を拒否する。これは、ミラーリング対象の中にバックアップ先が含まれることで、意図しないコピーや削除が発生することを避けるためである。

権限不足などで読み取れない source パスがある場合、vshdw は警告を表示してそのパスをスキップする。その場合、対応する dest 側のパスも削除対象から外す。これは、読み取り不能なだけの source パスを「存在しない」と誤判定して、バックアップ先を削除してしまうことを避けるためである。

ログファイルやデータベースの一時ファイルのように、走査後からコピー前までの間に source 側で削除されるファイルがある。その場合、vshdw はエラー終了せず、そのファイルをスキップして処理を続行する。

バックアップ先だけに存在するファイルを削除するとき、そのファイルが既に消えている、または OS が削除を拒否した場合、vshdw はその削除をスキップして処理を続行する。スキップした操作は summary の `skipped` に含める。`suppress_warnings = true` を設定すると、この警告表示も抑制される。

macOS から外部ボリュームへコピーする場合、`._000413.log` のような `._` で始まる AppleDouble ファイルが一時的に作成されることがある。これらを同期対象から外したい場合は次のように指定する。

```toml
exclude_file_patterns = ['(^|[\\/¥])\._']
```

## Goal

vshdw は、作業用ディレクトリ、開発プロジェクト、文書フォルダ、写真や資料の保存先などを、別ディスクに同じ状態で保持するための小さな道具である。クラウド同期や履歴管理システムではなく、手元のディスク間で確実に「現在の影」を作る。そのための、軽量で分かりやすい Rust 製一方向ミラーリングツールとして設計する。
