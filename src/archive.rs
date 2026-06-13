//! Cross-platform `zip` and `unzip` built on the pure-Rust `zip` crate, so the
//! commands behave identically on every platform rush targets.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

struct ZipOptions {
    recurse: bool,
    junk_paths: bool,
    quiet: bool,
    method: CompressionMethod,
    level: Option<i64>,
    archive: OsString,
    inputs: Vec<OsString>,
}

/// `zip [-r] [-j] [-q] [-0..-9] ARCHIVE FILE...`
pub fn zip_main(args: impl IntoIterator<Item = OsString>) -> i32 {
    let options = match parse_zip(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("zip: {message}");
            return 2;
        }
    };
    match create_archive(&options) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("zip: {error}");
            1
        }
    }
}

/// `unzip [-l] [-o] [-q] [-d DIR] ARCHIVE [MEMBER...]`
pub fn unzip_main(args: impl IntoIterator<Item = OsString>) -> i32 {
    let options = match parse_unzip(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("unzip: {message}");
            return 2;
        }
    };
    match run_unzip(&options) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("unzip: {error}");
            1
        }
    }
}

fn create_archive(options: &ZipOptions) -> io::Result<()> {
    let file = File::create(&options.archive)?;
    let mut writer = ZipWriter::new(file);
    let mut file_options = SimpleFileOptions::default().compression_method(options.method);
    if let Some(level) = options.level {
        file_options = file_options.compression_level(Some(level));
    }
    for input in &options.inputs {
        let disk = Path::new(input);
        let arcname = arcname_of(input);
        add_entry(&mut writer, disk, &arcname, file_options, options)?;
    }
    writer.finish()?;
    Ok(())
}

fn add_entry(
    writer: &mut ZipWriter<File>,
    disk: &Path,
    arcname: &str,
    file_options: SimpleFileOptions,
    options: &ZipOptions,
) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(disk) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("zip: {}: {error}", disk.display());
            return Ok(());
        }
    };
    if metadata.is_dir() {
        if !options.recurse {
            eprintln!("zip: {}: is a directory (use -r)", disk.display());
            return Ok(());
        }
        if !options.junk_paths {
            writer.add_directory(format!("{arcname}/"), file_options)?;
        }
        let mut children: Vec<_> = fs::read_dir(disk)?.filter_map(Result::ok).collect();
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let name = child.file_name().to_string_lossy().into_owned();
            let child_arcname = format!("{arcname}/{name}");
            add_entry(writer, &child.path(), &child_arcname, file_options, options)?;
        }
        return Ok(());
    }
    let stored = if options.junk_paths {
        Path::new(arcname)
            .file_name()
            .map_or_else(|| arcname.to_owned(), |name| name.to_string_lossy().into_owned())
    } else {
        arcname.to_owned()
    };
    writer.start_file(stored.clone(), file_options)?;
    let mut source = File::open(disk)?;
    io::copy(&mut source, writer)?;
    if !options.quiet {
        eprintln!("  adding: {stored}");
    }
    Ok(())
}

struct UnzipOptions {
    list: bool,
    quiet: bool,
    directory: PathBuf,
    archive: OsString,
    members: HashSet<String>,
}

fn run_unzip(options: &UnzipOptions) -> io::Result<()> {
    let file = File::open(&options.archive)?;
    let mut archive = ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        if !options.members.is_empty() && !options.members.contains(&name) {
            continue;
        }
        if options.list {
            println!("{:>10}  {}", entry.size(), name);
            continue;
        }
        let Some(relative) = entry.enclosed_name() else {
            eprintln!("unzip: skipping unsafe path: {name}");
            continue;
        };
        let target = options.directory.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&target)?;
        io::copy(&mut entry, &mut output)?;
        if !options.quiet {
            println!(" extracting: {}", target.display());
        }
    }
    Ok(())
}

fn parse_zip(args: impl IntoIterator<Item = OsString>) -> Result<ZipOptions, String> {
    let mut options = ZipOptions {
        recurse: false,
        junk_paths: false,
        quiet: false,
        method: CompressionMethod::Deflated,
        level: None,
        archive: OsString::new(),
        inputs: Vec::new(),
    };
    let mut positionals = Vec::new();
    for argument in args.into_iter().skip(1) {
        let text = argument.to_string_lossy();
        if let Some(flags) = short_flags(&text) {
            for flag in flags.chars() {
                match flag {
                    'r' => options.recurse = true,
                    'j' => options.junk_paths = true,
                    'q' => options.quiet = true,
                    '0' => options.method = CompressionMethod::Stored,
                    '1'..='9' => {
                        options.method = CompressionMethod::Deflated;
                        options.level = Some(i64::from(flag as u8 - b'0'));
                    }
                    other => return Err(format!("invalid option `-{other}`")),
                }
            }
        } else {
            positionals.push(argument);
        }
    }
    let mut positionals = positionals.into_iter();
    options.archive = positionals
        .next()
        .ok_or("missing archive name")?;
    options.inputs = positionals.collect();
    if options.inputs.is_empty() {
        return Err("nothing to do (no input files)".into());
    }
    Ok(options)
}

fn parse_unzip(args: impl IntoIterator<Item = OsString>) -> Result<UnzipOptions, String> {
    let mut options = UnzipOptions {
        list: false,
        quiet: false,
        directory: PathBuf::from("."),
        archive: OsString::new(),
        members: HashSet::new(),
    };
    let mut arguments = args.into_iter().skip(1).peekable();
    let mut positionals = Vec::new();
    while let Some(argument) = arguments.next() {
        let text = argument.to_string_lossy();
        if text == "-d" {
            options.directory = arguments
                .next()
                .map(PathBuf::from)
                .ok_or("option `-d` requires a directory")?;
        } else if let Some(flags) = short_flags(&text) {
            for flag in flags.chars() {
                match flag {
                    'l' => options.list = true,
                    'o' => {}
                    'q' => options.quiet = true,
                    other => return Err(format!("invalid option `-{other}`")),
                }
            }
        } else {
            positionals.push(argument);
        }
    }
    let mut positionals = positionals.into_iter();
    options.archive = positionals.next().ok_or("missing archive name")?;
    options.members = positionals
        .map(|member| member.to_string_lossy().into_owned())
        .collect();
    Ok(options)
}

/// Returns the flag characters of a short-option argument (`-rq` -> `rq`), or
/// `None` for positionals and the lone `-` stdin marker.
fn short_flags(text: &str) -> Option<&str> {
    text.strip_prefix('-').filter(|rest| !rest.is_empty())
}

fn arcname_of(input: &OsString) -> String {
    let text = input.to_string_lossy();
    text.replace('\\', "/")
        .trim_end_matches('/')
        .trim_start_matches("./")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osargs(args: &[&str]) -> Vec<OsString> {
        std::iter::once(OsString::from("zip"))
            .chain(args.iter().map(OsString::from))
            .collect()
    }

    #[test]
    fn roundtrips_files_and_directories() {
        let work = tempfile::tempdir().unwrap();
        let root = work.path();
        fs::create_dir(root.join("data")).unwrap();
        fs::write(root.join("data").join("a.txt"), b"alpha").unwrap();
        fs::write(root.join("top.txt"), b"top").unwrap();

        let archive = root.join("out.zip");
        let status = zip_main(osargs(&[
            "-r",
            "-q",
            archive.to_str().unwrap(),
            root.join("data").to_str().unwrap(),
            root.join("top.txt").to_str().unwrap(),
        ]));
        assert_eq!(status, 0);

        let out = root.join("extracted");
        let status = unzip_main(
            std::iter::once(OsString::from("unzip"))
                .chain(
                    ["-q", "-d", out.to_str().unwrap(), archive.to_str().unwrap()]
                        .iter()
                        .map(OsString::from),
                )
                .collect::<Vec<_>>(),
        );
        assert_eq!(status, 0);

        // entries keep their given path; locate a.txt regardless of prefix shape
        let found = walk_find(&out, "a.txt").expect("a.txt extracted");
        assert_eq!(fs::read(found).unwrap(), b"alpha");
    }

    fn walk_find(root: &Path, name: &str) -> Option<PathBuf> {
        for entry in fs::read_dir(root).ok()?.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = walk_find(&path, name) {
                    return Some(found);
                }
            } else if path.file_name().is_some_and(|file| file == name) {
                return Some(path);
            }
        }
        None
    }

    #[test]
    fn junk_paths_stores_basenames() {
        let work = tempfile::tempdir().unwrap();
        let root = work.path();
        fs::write(root.join("keep.txt"), b"x").unwrap();
        let archive = root.join("j.zip");
        assert_eq!(
            zip_main(osargs(&["-j", "-q", archive.to_str().unwrap(), root.join("keep.txt").to_str().unwrap()])),
            0
        );
        let reader = ZipArchive::new(File::open(&archive).unwrap()).unwrap();
        let names: Vec<_> = reader.file_names().collect();
        assert_eq!(names, ["keep.txt"]);
    }
}
