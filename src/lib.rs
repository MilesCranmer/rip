use clap::CommandFactory;
use log::debug;
use std::fs::Metadata;
use std::io::{BufRead, BufReader, Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};
use walkdir::WalkDir;

// Platform-specific imports
#[cfg(unix)]
use std::os::unix::fs::{symlink, FileTypeExt, PermissionsExt};

#[cfg(target_os = "windows")]
use std::os::windows::fs::symlink_file as symlink;

pub mod args;
pub mod completions;
pub mod record;
pub mod util;

use args::Args;
use record::{Record, RecordItem};

const LINES_TO_INSPECT: usize = 6;
const FILES_TO_INSPECT: usize = 6;
pub const BIG_FILE_THRESHOLD: u64 = 500000000; // 500 MB

pub fn run(cli: Args, mode: impl util::TestingMode, stream: &mut impl Write) -> Result<(), Error> {
    args::validate_args(&cli)?;
    // This selects the location of deleted
    // files based on the following order (from
    // first choice to last):
    // 1. Path passed with --graveyard
    // 2. Path pointed by the $GRAVEYARD variable
    // 3. $XDG_DATA_HOME/graveyard (only if XDG_DATA_HOME is defined)
    // 4. /tmp/graveyard-user
    let graveyard: &PathBuf = &{
        if let Some(flag) = cli.graveyard {
            flag
        } else if let Ok(env_graveyard) = env::var("RIP_GRAVEYARD") {
            PathBuf::from(env_graveyard)
        } else if let Ok(mut env_graveyard) = env::var("XDG_DATA_HOME") {
            if !env_graveyard.ends_with(std::path::MAIN_SEPARATOR) {
                env_graveyard.push(std::path::MAIN_SEPARATOR);
            }
            env_graveyard.push_str("graveyard");
            PathBuf::from(env_graveyard)
        } else {
            default_graveyard()
        }
    };
    debug!("Graveyard set to: {}", graveyard.display());

    if !graveyard.exists() {
        debug!("Creating graveyard at {}", graveyard.display());
        fs::create_dir_all(graveyard)?;

        #[cfg(unix)]
        {
            let metadata = graveyard.metadata()?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            debug!("Setting permissions on graveyard to 700");
        }
        // TODO: Default permissions on windows should be good, but need to double-check.
    }

    // If the user wishes to restore everything
    if cli.decompose {
        debug!("Decomposing graveyard");
        if util::prompt_yes("Really unlink the entire graveyard?", &mode, stream)? {
            fs::remove_dir_all(graveyard)?;
            debug!("Finished removing all directories");
        }
        return Ok(());
    }

    // Stores the deleted files
    let record = Record::new(graveyard);
    let cwd = &env::current_dir()?;
    debug!("Current working directory: {}", cwd.display());

    if let Some(mut graves_to_exhume) = cli.unbury {
        // Vector to hold the grave path of items we want to unbury.
        // This will be used to determine which items to remove from the
        // record following the unbury.
        // Initialize it with the targets passed to -r
        debug!(
            "Entered unbury mode with {:?} explicitly passed",
            graves_to_exhume
        );

        // If -s is also passed, push all files found by seance onto
        // the graves_to_exhume.
        if cli.seance && record.open().is_ok() {
            debug!("Seance mode enabled");
            let gravepath = util::join_absolute(graveyard, cwd)
                .to_string_lossy()
                .into_owned();
            for grave in record.seance(gravepath) {
                graves_to_exhume.push(grave);
            }
            debug!("Found graves to exhume: {:?}", graves_to_exhume);
        }

        // Otherwise, add the last deleted file
        if graves_to_exhume.is_empty() {
            debug!("No graves passed, checking for last buried file");
            if let Ok(s) = record.get_last_bury() {
                graves_to_exhume.push(s);
                debug!("Found last buried file: {:?}", graves_to_exhume);
            }
        }

        // Go through the graveyard and exhume all the graves
        for line in record.lines_of_graves(&graves_to_exhume) {
            let entry = RecordItem::new(&line);
            debug!("Exhuming: {:?}", entry);
            let orig: PathBuf = match util::symlink_exists(entry.orig) {
                true => util::rename_grave(entry.orig),
                false => PathBuf::from(entry.orig),
            };
            debug!(
                "Executing move_target from {} to {}",
                entry.dest.display(),
                orig.display()
            );
            move_target(entry.dest, &orig, &mode, stream).map_err(|e| {
                Error::new(
                    e.kind(),
                    format!(
                        "Unbury failed: couldn't copy files from {} to {}",
                        entry.dest.display(),
                        orig.display()
                    ),
                )
            })?;
            writeln!(
                stream,
                "Returned {} to {}",
                entry.dest.display(),
                orig.display()
            )?;
        }
        debug!("Finished exhuming graves");
        record.log_exhumed_graves(&graves_to_exhume)?;

        return Ok(());
    }

    if cli.seance {
        debug!("Seance mode enabled");
        let gravepath = util::join_absolute(graveyard, cwd);
        debug!("Checking for graves in {}", gravepath.display());
        for grave in record.seance(gravepath.to_string_lossy()) {
            writeln!(stream, "{}", grave.display())?;
        }
        return Ok(());
    }

    if cli.targets.is_empty() {
        debug!("Found no targets, printing help");
        Args::command().print_help()?;
        return Ok(());
    }

    for target in cli.targets {
        debug!("Burying target: {}", target.display());
        bury_target(&target, graveyard, &record, cwd, cli.inspect, &mode, stream)?;
    }

    Ok(())
}

fn bury_target(
    target: &PathBuf,
    graveyard: &PathBuf,
    record: &Record,
    cwd: &Path,
    inspect: bool,
    mode: &impl util::TestingMode,
    stream: &mut impl Write,
) -> Result<(), Error> {
    // Check if source exists
    let metadata = fs::symlink_metadata(target).map_err(|_| {
        Error::new(
            ErrorKind::NotFound,
            format!(
                "Cannot remove {}: no such file or directory",
                target.to_str().unwrap()
            ),
        )
    })?;
    debug!("Found metadata for target: {:?}", metadata);
    // Canonicalize the path unless it's a symlink
    let source = &if !metadata.file_type().is_symlink() {
        debug!("Not a symlink, canonicalizing path");
        dunce::canonicalize(cwd.join(target))
            .map_err(|e| Error::new(e.kind(), "Failed to canonicalize path"))?
    } else {
        debug!("Symlink detected, using path as is");
        cwd.join(target)
    };
    debug!("Using canonicalized path for target: {}", source.display());

    if inspect {
        let moved_to_graveyard = do_inspection(target, source, metadata, mode, stream)?;
        if moved_to_graveyard {
            return Ok(());
        }
    }

    // If rip is called on a file already in the graveyard, prompt
    // to permanently delete it instead.
    if source.starts_with(graveyard) {
        writeln!(stream, "{} is already in the graveyard.", source.display())?;
        if util::prompt_yes("Permanently unlink it?", mode, stream)? {
            if fs::remove_dir_all(source).is_err() {
                fs::remove_file(source).map_err(|e| {
                    Error::new(e.kind(), format!("Couldn't unlink {}", source.display()))
                })?;
            }
            return Ok(());
        } else {
            writeln!(stream, "Skipping {}", source.display())?;
            // TODO: In the original code, this was a hard return from the entire
            // method (i.e., `run`). I think it should just be a return from the bury
            // (meaning a `continue` in the original code's loop). But I'm not sure.
            return Ok(());
        }
    }

    let dest: &Path = &{
        let dest = util::join_absolute(graveyard, source);
        // Resolve a name conflict if necessary
        if util::symlink_exists(&dest) {
            util::rename_grave(dest)
        } else {
            dest
        }
    };

    move_target(source, dest, mode, stream).map_err(|e| {
        fs::remove_dir_all(dest).ok();
        Error::new(e.kind(), "Failed to bury file")
    })?;

    // Clean up any partial buries due to permission error
    record.write_log(source, dest)?;

    Ok(())
}

fn do_inspection(
    target: &Path,
    source: &PathBuf,
    metadata: Metadata,
    mode: &impl util::TestingMode,
    stream: &mut impl Write,
) -> Result<bool, Error> {
    if metadata.is_dir() {
        debug!("Detected target to be a directory; printing content summary");
        // Get the size of the directory and all its contents
        writeln!(
            stream,
            "{}: directory, {} including:",
            target.to_str().unwrap(),
            util::humanize_bytes(
                WalkDir::new(source)
                    .into_iter()
                    .filter_map(|x| x.ok())
                    .filter_map(|x| x.metadata().ok())
                    .map(|x| x.len())
                    .sum::<u64>(),
            )
        )?;

        debug!("Printing first {} files in directory", FILES_TO_INSPECT);
        // Print the first few top-level files in the directory
        for entry in WalkDir::new(source)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .take(FILES_TO_INSPECT)
        {
            writeln!(stream, "{}", entry.path().display())?;
        }
    } else {
        debug!("Detected target to be a file");
        writeln!(
            stream,
            "{}: file, {}",
            &target.to_str().unwrap(),
            util::humanize_bytes(metadata.len())
        )?;
        // Read the file and print the first few lines
        if let Ok(source_file) = fs::File::open(source) {
            debug!("Printing first {} lines of file", LINES_TO_INSPECT);
            for line in BufReader::new(source_file)
                .lines()
                .take(LINES_TO_INSPECT)
                .filter_map(|line| line.ok())
            {
                writeln!(stream, "> {}", line)?;
            }
        } else {
            writeln!(stream, "Error reading {}", source.display())?;
        }
    }
    Ok(!util::prompt_yes(
        format!("Send {} to the graveyard?", target.to_str().unwrap()),
        mode,
        stream,
    )?)
}

pub fn move_target(
    target: &Path,
    dest: &Path,
    mode: &impl util::TestingMode,
    stream: &mut impl Write,
) -> Result<(), Error> {
    // Try a simple rename, which will only work within the same mount point.
    // Trying to rename across filesystems will throw errno 18.
    debug!(
        "Attempting a simple rename from {} to {}",
        target.display(),
        dest.display()
    );
    if fs::rename(target, dest).is_ok() {
        return Ok(());
    }

    debug!("Simple rename failed, attempting to copy and remove");
    // If that didn't work, then copy and rm.
    {
        let parent = dest
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Could not get parent of dest!"))?;

        fs::create_dir_all(parent)?
    }

    let sym_link_data = fs::symlink_metadata(target)?;
    if sym_link_data.is_dir() {
        // Walk the source, creating directories and copying files as needed
        for entry in WalkDir::new(target).into_iter().filter_map(|e| e.ok()) {
            // Path without the top-level directory
            let orphan = entry.path().strip_prefix(target).map_err(|_| {
                Error::new(
                    ErrorKind::Other,
                    "Parent directory isn't a prefix of child directories?",
                )
            })?;

            if entry.file_type().is_dir() {
                fs::create_dir_all(dest.join(orphan)).map_err(|e| {
                    Error::new(
                        e.kind(),
                        format!(
                            "Failed to create dir: {} in {}",
                            entry.path().display(),
                            dest.join(orphan).display()
                        ),
                    )
                })?;
            } else {
                copy_file(entry.path(), &dest.join(orphan), mode, stream).map_err(|e| {
                    Error::new(
                        e.kind(),
                        format!(
                            "Failed to copy file from {} to {}",
                            entry.path().display(),
                            dest.join(orphan).display()
                        ),
                    )
                })?;
            }
        }
        fs::remove_dir_all(target).map_err(|e| {
            Error::new(
                e.kind(),
                format!("Failed to remove dir: {}", target.display()),
            )
        })?;
    } else {
        copy_file(target, dest, mode, stream).map_err(|e| {
            Error::new(
                e.kind(),
                format!(
                    "Failed to copy file from {} to {}",
                    target.display(),
                    dest.display()
                ),
            )
        })?;
        fs::remove_file(target).map_err(|e| {
            Error::new(
                e.kind(),
                format!("Failed to remove file: {}", target.display()),
            )
        })?;
    }

    Ok(())
}

pub fn copy_file(
    source: &Path,
    dest: &Path,
    mode: &impl util::TestingMode,
    stream: &mut impl Write,
) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(source)?;
    let filetype = metadata.file_type();

    if metadata.len() > BIG_FILE_THRESHOLD {
        writeln!(
            stream,
            "About to copy a big file ({} is {})",
            source.display(),
            util::humanize_bytes(metadata.len())
        )?;
        if util::prompt_yes("Permanently delete this file instead?", mode, stream)? {
            return Ok(());
        }
    }

    if filetype.is_file() {
        fs::copy(source, dest)?;
        return Ok(());
    }

    #[cfg(unix)]
    if filetype.is_fifo() {
        let metadata_mode = metadata.permissions().mode();
        std::process::Command::new("mkfifo")
            .arg(dest)
            .arg("-m")
            .arg(metadata_mode.to_string())
            .output()?;
        return Ok(());
    }

    if filetype.is_symlink() {
        let target = fs::read_link(source)?;
        symlink(target, dest)?;
        return Ok(());
    }

    if let Err(e) = fs::copy(source, dest) {
        // Special file: Try copying it as normal, but this probably won't work
        writeln!(
            stream,
            "Non-regular file or directory: {}",
            source.display()
        )?;
        if !util::prompt_yes("Permanently delete the file?", mode, stream)? {
            return Err(e);
        }
        // Create a dummy file to act as a marker in the graveyard
        let mut marker = fs::File::create(dest)?;
        marker.write_all(
            b"This is a marker for a file that was \
                           permanently deleted.  Requiescat in pace.",
        )?;
    }

    Ok(())
}

fn default_graveyard() -> PathBuf {
    let user = util::get_user();

    #[cfg(unix)]
    let base_path = "/tmp/graveyard";

    #[cfg(target_os = "windows")]
    let base_path = env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\Temp".to_string());

    PathBuf::from(format!("{}-{}", base_path, user))
}
