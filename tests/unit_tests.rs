use lazy_static::lazy_static;
use rip2::args::{validate_args, Args, Commands};
use rip2::completions;
use rip2::util::{humanize_bytes, TestMode};
use rstest::rstest;
use std::fs;
use std::io::{Cursor, ErrorKind};
use std::path::PathBuf;
use std::process;
use std::sync::{Mutex, MutexGuard};
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(target_os = "windows")]
use std::os::windows::fs::symlink_file as symlink;

#[cfg(unix)]
use std::os::unix::net::UnixListener;

#[cfg(target_os = "macos")]
use std::os::unix::fs::FileTypeExt;

lazy_static! {
    static ref GLOBAL_LOCK: Mutex<()> = Mutex::new(());
}

fn aquire_lock() -> MutexGuard<'static, ()> {
    GLOBAL_LOCK.lock().unwrap()
}

#[rstest]
fn test_validation() {
    let bad_completions = Args {
        command: Some(Commands::Completions {
            shell: "bash".to_string(),
        }),
        decompose: true,
        ..Args::default()
    };
    validate_args(&bad_completions).expect_err("--completions can only be used by itself");

    let bad_decompose = Args {
        decompose: true,
        seance: true,
        ..Args::default()
    };
    validate_args(&bad_decompose).expect_err("-d,--decompose can only be used with --graveyard");
}

#[rstest]
fn test_filetypes(
    #[values("regular", "big", "fifo", "symlink", "socket")] file_type: &str,
    #[values(false, true)] copy: bool,
) {
    if ["big", "socket"].contains(&file_type) && !copy {
        return;
    }

    #[cfg(target_os = "windows")]
    {
        if ["fifo", "socket"].contains(&file_type) {
            return;
        }
    }
    let tmpdir = tempdir().unwrap();
    let path = PathBuf::from(tmpdir.path());
    let source_path = path.join("test_file");
    let dest_path = path.join("test_file_copy");

    match file_type {
        "regular" => {
            fs::File::create(&source_path).unwrap();
        }
        "big" => {
            let file = fs::File::create(&source_path).unwrap();
            let len = rip2::BIG_FILE_THRESHOLD + 1;
            file.set_len(len).unwrap();
        }
        "fifo" => {
            process::Command::new("mkfifo")
                .arg(&source_path)
                .output()
                .unwrap();
        }
        "symlink" => {
            let target_path = path.join("symlink_target");
            fs::File::create(&target_path).unwrap();
            symlink(&target_path, &source_path).unwrap();
        }
        "socket" => {
            #[cfg(unix)]
            {
                UnixListener::bind(&source_path).unwrap();
            }
        }
        _ => unreachable!(),
    }

    let mut log = Vec::new();
    let mode = TestMode;

    if copy {
        rip2::copy_file(&source_path, &dest_path, &mode, &mut log).unwrap();
    } else {
        rip2::move_target(&source_path, &dest_path, &mode, &mut log).unwrap();
    }

    let log_s = String::from_utf8(log).unwrap();

    // Check logs
    match file_type {
        "big" => {
            assert!(log_s.contains("About to copy a big file"));
        }
        "socket" => {
            assert!(log_s.contains("Non-regular file or directory:"));
            assert!(log_s.contains("Permanently delete the file?"));
        }
        _ => {
            assert!(log_s.is_empty())
        }
    }

    // Check graveyard contents and file type
    // let metadata = fs::symlink_metadata(dest_path).unwrap();
    // let ftype = metadata.file_type();
    let ftype = fs::symlink_metadata(&dest_path).map(|m| m.file_type());
    match file_type {
        "regular" => {
            assert!(dest_path.is_file());
            assert!(ftype.unwrap().is_file());
        }
        "big" => {
            assert!(!dest_path.exists());
        }
        "fifo" => {
            #[cfg(target_os = "macos")]
            {
                assert!(dest_path.exists());
                assert!(ftype.unwrap().is_fifo());
                // TODO: Why does this fail on Linux?
            }
        }
        "symlink" => {
            assert!(dest_path.exists());
            assert!(ftype.unwrap().is_symlink());
        }
        "socket" => {
            // Socket files are not copied, so are instead simply deleted
            assert!(!dest_path.exists());
        }
        _ => {}
    }
}

#[rstest]
fn test_prompt_read(#[values("y", "Y", "n", "N", "", "\n", "q", "Q", "k")] key: &str) {
    let input = Cursor::new(key);
    let result = rip2::util::yes_no_quit(input);
    match key {
        "y" | "Y" => assert!(result.unwrap()),
        "n" | "N" | "" | "\n" => assert!(!result.unwrap()),
        "q" | "Q" => {
            let err = result.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Interrupted);
            assert_eq!(err.to_string(), "User requested to quit");
        }
        "k" => {
            let err = result.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput);
            assert_eq!(err.to_string(), "Invalid input");
        }
        _ => {}
    }
}

#[rstest]
fn test_completions(
    #[values("bash", "elvish", "fish", "powershell", "zsh", "nushell", "fake")] shell: &str,
) {
    let mut output = Vec::new();
    let result = completions::generate_shell_completions(shell, &mut output);
    let output_s = String::from_utf8(output).unwrap();
    match shell {
        "bash" => {
            assert!(output_s.contains("complete -F"));
        }
        "elvish" => {
            assert!(output_s.contains("set edit:completion:arg-completer[rip]"));
        }
        "fish" => {
            assert!(output_s.contains("complete -c"));
        }
        "powershell" => {
            assert!(output_s.contains("Register-ArgumentCompleter"));
        }
        "zsh" => {
            assert!(output_s.contains("compdef"));
        }
        "nushell" => {
            assert!(output_s.contains("export extern"));
        }
        "fake" => {
            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("Invalid shell specification: fake"));
            assert!(
                err_msg.contains("Available shells: bash, elvish, fish, powershell, zsh, nushell")
            );
        }
        _ => {}
    }
}

#[rstest]
fn test_graveyard_path() {
    let _env_lock = aquire_lock();

    // Clear env:
    std::env::remove_var("RIP_GRAVEYARD");
    std::env::remove_var("XDG_DATA_HOME");

    // Check default graveyard path
    let graveyard = rip2::get_graveyard(None);
    assert_eq!(
        graveyard,
        std::env::temp_dir().join(format!("graveyard-{}", rip2::util::get_user()))
    );
}

#[rstest]
fn test_humanize_bytes() {
    assert_eq!(humanize_bytes(0), "0 B");
    assert_eq!(humanize_bytes(1), "1 B");
    assert_eq!(humanize_bytes(1024), "1.0 KiB");
    assert_eq!(humanize_bytes(1024 * 1024), "1.0 MiB");
    assert_eq!(humanize_bytes(1024 * 1024 * 1024), "1.0 GiB");
    assert_eq!(humanize_bytes(1024 * 1024 * 1024 * 1024), "1.0 TiB");

    assert_eq!(humanize_bytes(1024 * 1024 + 1024 * 512), "1.5 MiB");
}
