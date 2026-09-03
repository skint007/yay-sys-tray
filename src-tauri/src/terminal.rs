use crate::tailscale::ssh_target;
use crate::tray::TrayState;
use crate::AppState;
use base64::Engine;
use tauri::{Emitter, Manager};
use tokio::process::Command;

/// How to invoke a given terminal emulator. One row per known terminal instead
/// of the same knowledge scattered across three parallel match arms, so adding
/// a terminal is a single table entry and the flags for one terminal can't drift
/// apart.
struct TermSpec {
    /// Leading argv — the program plus any subcommand (e.g. `["wezterm",
    /// "start"]`) or flag that is passed on every launch.
    argv0: &'static [&'static str],
    /// Flag/separator placed just before the command to run (`-e`, `--`, `-x`),
    /// or `None` when the command is positional (kitty/foot).
    exec_flag: Option<&'static str>,
    /// Flag that keeps the window open after the command exits, if supported. A
    /// flag ending in `=` is passed on every launch with `true`/`false` joined
    /// onto it, for terminals that would otherwise read holding out of the
    /// user's own config and ignore this app's setting (Ghostty).
    hold_flag: Option<&'static str>,
    /// Flag that makes the launcher stay alive until the command exits, for
    /// client/server terminals whose CLI otherwise returns as soon as the
    /// server owns the window. Without it the update looks finished the moment
    /// it starts.
    wait_flag: Option<&'static str>,
    /// Flag that sets the window title, if supported. A flag ending in `=`
    /// takes its value joined onto the same argv entry, for terminals that
    /// reject a space-separated value (Ghostty).
    title_flag: Option<&'static str>,
    /// Whether the terminal refuses to close its window when a command exits
    /// non-zero very quickly, treating it as a failed spawn worth showing. The
    /// window then outlives the command, so the launcher never returns and the
    /// update never looks finished. Commands for these terminals go through
    /// [`wrap_command`].
    holds_on_fast_failure: bool,
}

/// Look up a terminal's invocation spec. Unknown terminals fall back to a
/// minimal `<term> -e <cmd>` with no hold/title flags: `-e` is the most widely
/// supported exec flag, and blindly adding `--hold`/title flags a terminal
/// doesn't accept makes it reject the whole command line and never launch.
fn term_spec(terminal: &str) -> TermSpec {
    match terminal {
        "kitty" => TermSpec { argv0: &["kitty"], exec_flag: None, wait_flag: None, hold_flag: Some("--hold"), holds_on_fast_failure: false, title_flag: Some("--title") },
        "konsole" => TermSpec { argv0: &["konsole"], exec_flag: Some("-e"), wait_flag: None, hold_flag: Some("--hold"), holds_on_fast_failure: false, title_flag: None },
        "alacritty" => TermSpec { argv0: &["alacritty"], exec_flag: Some("-e"), wait_flag: None, hold_flag: Some("--hold"), holds_on_fast_failure: false, title_flag: Some("--title") },
        "foot" => TermSpec { argv0: &["foot"], exec_flag: None, wait_flag: None, hold_flag: Some("--hold"), holds_on_fast_failure: false, title_flag: Some("--title") },
        "xterm" => TermSpec { argv0: &["xterm"], exec_flag: Some("-e"), wait_flag: None, hold_flag: Some("-hold"), holds_on_fast_failure: false, title_flag: Some("-T") },
        "xfce4-terminal" => TermSpec { argv0: &["xfce4-terminal"], exec_flag: Some("-x"), wait_flag: None, hold_flag: Some("--hold"), holds_on_fast_failure: false, title_flag: Some("--title") },
        // gnome-terminal/ptyxis/wezterm take the command after `--`; none has a
        // usable hold flag, so leave it off rather than break the launch.
        "gnome-terminal" => TermSpec { argv0: &["gnome-terminal"], exec_flag: Some("--"), wait_flag: Some("--wait"), hold_flag: None, holds_on_fast_failure: false, title_flag: None },
        // Ptyxis needs no wait flag and offers none. A `--` command implies
        // single-instance mode, so this process *is* the terminal rather than a
        // client of a running one, and it stays alive until the command exits
        // and closes the window (#38). `--wait` would be rejected as an unknown
        // option and nothing would launch. `--title` is accepted on that same
        // standalone path and becomes the tab's title prefix.
        "ptyxis" => TermSpec { argv0: &["ptyxis"], exec_flag: Some("--"), wait_flag: None, hold_flag: None, holds_on_fast_failure: true, title_flag: Some("--title") },
        "wezterm" => TermSpec { argv0: &["wezterm", "start"], exec_flag: Some("--"), wait_flag: None, hold_flag: None, holds_on_fast_failure: false, title_flag: None },
        // Ghostty takes every argument after `-e` as the command, and that flag
        // already implies `gtk-single-instance=false` plus
        // `quit-after-last-window-closed=true`, so this process owns the window
        // and lives exactly as long as the command — no wait flag needed (nor
        // offered). Its flags are config keys, and its CLI parser rejects a
        // value passed as a separate argument, hence the trailing `=`. Every
        // key also has a value in the user's ghostty config, so the ones this
        // app has an opinion about are always passed rather than only when on.
        // `abnormal-command-exit-runtime=0` is that same defence for Ghostty's
        // habit of holding the window open on a command that exits non-zero
        // within 250ms (the Ptyxis trap from #43, which the user could widen to
        // seconds); [`wrap_command`] still covers a command that manages to
        // exit within the same millisecond.
        "ghostty" => TermSpec { argv0: &["ghostty", "--abnormal-command-exit-runtime=0"], exec_flag: Some("-e"), wait_flag: None, hold_flag: Some("--wait-after-command="), holds_on_fast_failure: true, title_flag: Some("--title=") },
        _ => TermSpec { argv0: &[], exec_flag: Some("-e"), wait_flag: None, hold_flag: None, holds_on_fast_failure: false, title_flag: None },
    }
}

/// Build a terminal command prefix, optionally holding the window open and
/// setting its title. Order: `program [wait] [hold] [title T] [exec_flag]` then
/// the command the caller appends.
fn terminal_prefix(terminal: &str, title: Option<&str>, hold: bool) -> Vec<String> {
    let spec = term_spec(terminal);
    let mut out: Vec<String> = Vec::new();

    if spec.argv0.is_empty() {
        // Unknown terminal: the configured name is the program.
        out.push(terminal.to_string());
    } else {
        out.extend(spec.argv0.iter().map(|s| s.to_string()));
    }

    if let Some(flag) = spec.wait_flag {
        out.push(flag.to_string());
    }
    match spec.hold_flag {
        // A flag carrying its own value can also say *not* to hold, which is
        // the only way to override a terminal that reads its default from the
        // user's config.
        Some(flag) if flag.ends_with('=') => out.push(format!("{flag}{hold}")),
        Some(flag) if hold => out.push(flag.to_string()),
        _ => {}
    }
    if let (Some(title), Some(flag)) = (title, spec.title_flag) {
        if flag.ends_with('=') {
            out.push(format!("{flag}{title}"));
        } else {
            out.push(flag.to_string());
            out.push(title.to_string());
        }
    }
    if let Some(flag) = spec.exec_flag {
        out.push(flag.to_string());
    }

    out
}

/// Shell run by [`wrap_command`]. `"$@"` runs the real command straight from
/// argv, so nothing has to be re-quoted, and the sleep only costs anything on a
/// command that already failed. One second clears the widest "too fast to be
/// real" window these terminals apply (Ptyxis 0.5s; Ghostty is pinned to 0 at
/// launch). The original exit status is preserved, and a command killed by a
/// signal (which Ptyxis never closes the window for, at any speed) becomes a
/// normal `exit 128+n` of this shell instead.
const SLOW_FAILURE_SCRIPT: &str = r#""$@"; rc=$?; [ "$rc" -eq 0 ] || sleep 1; exit "$rc""#;

/// Wrap a command for terminals that keep their window open when a command
/// fails immediately, so it can't exit fast *and* non-zero. Without this a
/// failed update leaves the window (and so the terminal process) alive until
/// the user closes it by hand, and `update-finished` never fires (#43). Every
/// other terminal gets its command unchanged.
fn wrap_command(terminal: &str, cmd: Vec<String>) -> Vec<String> {
    if cmd.is_empty() || !term_spec(terminal).holds_on_fast_failure {
        return cmd;
    }

    let mut out = vec![
        "bash".to_string(),
        "-c".to_string(),
        SLOW_FAILURE_SCRIPT.to_string(),
        // $0 for the wrapper shell, so any error it prints is attributable.
        "yay-sys-tray".to_string(),
    ];
    out.extend(cmd);
    out
}

/// Wrap a reboot command with the configured delay (Ctrl+C in the terminal cancels).
fn delayed_reboot_cmd(reboot_cmd: &str, delay: u32) -> String {
    if delay == 0 {
        reboot_cmd.to_string()
    } else {
        format!("echo 'Rebooting in {delay}s (Ctrl+C to cancel)...' && sleep {delay} && {reboot_cmd}")
    }
}

/// Assemble a pacman/yay command line as a single shell string, appending
/// `--noconfirm` and an optional `&& <reboot>` chain. Used for the cases that
/// must run through a shell (the reboot chain, and every remote command, which
/// runs under the ssh login shell). One place so the noconfirm/reboot logic
/// can't diverge across the six update/remove entry points.
fn build_shell_cmd(base: &str, noconfirm: bool, reboot: Option<(&str, u32)>) -> String {
    let mut cmd = base.to_string();
    if noconfirm {
        cmd.push_str(" --noconfirm");
    }
    if let Some((reboot_cmd, delay)) = reboot {
        cmd.push_str(&format!(" && {}", delayed_reboot_cmd(reboot_cmd, delay)));
    }
    cmd
}

/// argv for an interactive remote command.
///
/// `-t` forces a pty on the far side. Without one `sudo` has nowhere to prompt
/// and dies with "a terminal is required to read the password", so remote
/// updates only ever worked on hosts with NOPASSWD. These commands always run
/// inside a terminal emulator, so the local stdin `-t` needs is present.
///
/// Deliberately not used by the *check* path in `tailscale.rs`: that output is
/// parsed, and a pty would echo and line-wrap it.
fn ssh_argv(target: String, cmd: String) -> Vec<String> {
    vec!["ssh".to_string(), "-t".to_string(), target, cmd]
}

const REMOTE_JOB_RETRY: i32 = 75;
const REMOTE_JOB_DETACHED: i32 = 74;
const REMOTE_JOB_NOT_STARTED: i32 = 76;
const REMOTE_JOB_OBSERVED: &str = "yay-sys-tray-job-observed";
const REMOTE_POST_TERMINAL_PROBES: u8 = 12;

/// Runs inside tmux on the remote host. State is written before the update and
/// before a requested reboot, which lets a later SSH connection distinguish a
/// completed reboot from a lost update process.
const REMOTE_JOB_RUNNER: &str = r#"#!/usr/bin/env bash
set +e

job_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
job_id=$(cat "$job_dir/current")
tmux set-window-option -t "$TMUX_PANE" remain-on-exit off
exec > >(tee -a "$job_dir/output-$job_id.log") 2>&1

write_state() {
    printf '%s\n' "$1" > "$job_dir/state.next" && mv "$job_dir/state.next" "$job_dir/state"
    printf '%s\n' "$1" > "$job_dir/results/$job_id.next" \
        && mv "$job_dir/results/$job_id.next" "$job_dir/results/$job_id"
}

write_state running
update_command=$(base64 -d < "$job_dir/command.b64")
bash -lc "$update_command"
rc=$?

if [ "$rc" -eq 0 ] && [ "$(cat "$job_dir/restart")" = 1 ]; then
    delay=$(cat "$job_dir/restart-delay")
    if [ "$delay" -gt 0 ]; then
        echo "Rebooting in ${delay}s (Ctrl+C to cancel)..."
        sleep "$delay"
        rc=$?
    fi

    if [ "$rc" -eq 0 ]; then
        boot_id=$(cat /proc/sys/kernel/random/boot_id)
        write_state "rebooting $boot_id"
        sudo reboot
        rc=$?
        if [ "$rc" -eq 0 ]; then
            # Keep the tmux pane and `rebooting` state alive until systemd
            # actually takes the machine down. A successful reboot request can
            # return before shutdown begins.
            while true; do sleep 60; done
        fi
    fi
fi

if [ "$rc" -eq 0 ]; then
    echo
    echo "Remote update finished."
else
    echo
    echo "Remote update failed with exit status $rc."
fi
write_state "done $rc"
sleep 1
exit "$rc"
"#;

/// Starts or reattaches one remote update. The request id makes the start
/// idempotent: if SSH dies after tmux was created, retrying this exact command
/// attaches to that session instead of launching yay a second time.
const REMOTE_JOB_CONTROLLER: &str = r#"set -u

job_id=$1
command_b64=$2
runner_b64=$3
restart=$4
restart_delay=$5
mode=$6
umask 077
case "$job_id" in
    *[!0-9-]*|'') echo "Invalid remote update job id." >&2; exit 2 ;;
esac
case "$mode" in
    attach|status) ;;
    *) echo "Invalid remote update mode." >&2; exit 2 ;;
esac
session="yay-sys-tray-$job_id"
job_dir=${XDG_STATE_HOME:-"$HOME/.local/state"}/yay-sys-tray/remote-update
result_file="$job_dir/results/$job_id"
output_file="$job_dir/output-$job_id.log"

mkdir -p "$job_dir/results"
chmod 700 "$job_dir" "$job_dir/results"
exec 9> "$job_dir/lock"
flock 9

read_state() {
    state=
    value=
    if [ -f "$job_dir/state" ]; then
        read -r state value < "$job_dir/state" || true
    fi
}

write_state() {
    printf '%s\n' "$1" > "$job_dir/state.next" && mv "$job_dir/state.next" "$job_dir/state"
}

write_result() {
    id=$1
    result=$2
    [ -n "$id" ] || return
    printf '%s\n' "$result" > "$job_dir/results/$id.next" \
        && mv "$job_dir/results/$id.next" "$job_dir/results/$id"
}

current=
if [ -f "$job_dir/current" ]; then
    read -r current < "$job_dir/current" || true
fi
case "$current" in
    *[!0-9-]*) current= ;;
esac
current_session="yay-sys-tray-$current"
read_state

pending=
if [ -f "$job_dir/pending" ]; then
    read -r pending < "$job_dir/pending" || true
fi

# The request files are durable before `pending` is written. If the SSH shell
# dies while publishing `current`, the next visible watcher can finish that
# small transaction without rebuilding or rerunning anything.
if [ "$current" != "$job_id" ] && [ "$pending" = "$job_id" ]; then
    if [ "$mode" = status ]; then
        flock -u 9
        exit 76
    else
        write_state starting
        printf '%s\n' "$job_id" > "$job_dir/current"
        rm -f "$job_dir/pending"
        current=$job_id
        current_session=$session
        state=starting
    fi
fi

# A request id is never reusable. If a late watcher returns after newer jobs
# have run, its own result ends that watcher instead of rerunning its command.
if [ "$current" != "$job_id" ] && [ -f "$result_file" ]; then
    requested_state=
    requested_value=
    read -r requested_state requested_value < "$result_file" || true
    flock -u 9
    if [ "$requested_state" = done ]; then
        if [ "$mode" = attach ] && [ -s "$output_file" ]; then
            cat "$output_file"
        fi
        [ "$requested_value" -eq 0 ] && exit 0
        echo "Remote update failed with exit status $requested_value." >&2
        exit 1
    fi
    echo "This remote update request is no longer active." >&2
    exit 1
fi

# The tray's background monitor may arrive before the visible terminal. It is
# observation-only and must never create a job that could wait for input with
# no terminal attached.
if [ "$mode" = status ] && { [ "$current" != "$job_id" ] || [ -z "$state" ]; }; then
    if ! command -v tmux >/dev/null 2>&1; then
        echo "tmux is required on the remote host for reconnectable updates." >&2
        flock -u 9
        exit 127
    fi
    flock -u 9
    exit 76
fi

# Reconcile the old job before deciding whether this request may replace it.
if [ "$state" = rebooting ]; then
    boot_id=$(cat /proc/sys/kernel/random/boot_id)
    if [ "$boot_id" != "$value" ]; then
        write_state 'done 0'
        write_result "$current" 'done 0'
        state=done
        value=0
    fi
elif [ "$state" = running ] && ! tmux has-session -t "$current_session" 2>/dev/null; then
    write_state 'done 125'
    write_result "$current" 'done 125'
    state=done
    value=125
elif [ "$state" = starting ] && [ "$current" != "$job_id" ] \
    && ! tmux has-session -t "$current_session" 2>/dev/null; then
    write_state 'done 125'
    write_result "$current" 'done 125'
    state=done
    value=125
fi

# A disconnect can kill this controller after it records the request id but
# before it records `starting`. The same request may safely finish that start.
if [ "$current" = "$job_id" ] && [ -z "$state" ]; then
    printf '%s' "$command_b64" > "$job_dir/command.b64"
    printf '%s' "$runner_b64" | base64 -d > "$job_dir/runner.sh"
    printf '%s\n' "$restart" > "$job_dir/restart"
    printf '%s\n' "$restart_delay" > "$job_dir/restart-delay"
    : > "$output_file"
    chmod 700 "$job_dir/runner.sh"
    write_state starting
    write_result "$job_id" starting
    state=starting
fi

if [ -n "$current" ] && [ "$current" != "$job_id" ] \
    && { [ "$state" = starting ] || [ "$state" = running ] || [ "$state" = rebooting ]; }; then
    echo "Another yay-sys-tray update is already running on this host."
    flock -u 9
    exit 73
fi

# The package command is done, but its pane may still be flushing the final
# output into its log. Wait for that short cleanup instead of truncating the
# log for a new request underneath it.
if [ -n "$current" ] && [ "$current" != "$job_id" ] && [ "$state" = done ] \
    && tmux has-session -t "$current_session" 2>/dev/null; then
    echo "The previous remote update is finishing."
    flock -u 9
    exit 75
fi

if [ "$current" != "$job_id" ]; then
    if ! command -v tmux >/dev/null 2>&1; then
        echo "tmux is required on the remote host for reconnectable updates." >&2
        flock -u 9
        exit 127
    fi

    if [ -n "$current" ]; then
        rm -f "$job_dir/output-$current.log"
    fi
    printf '%s' "$command_b64" > "$job_dir/command.b64"
    printf '%s' "$runner_b64" | base64 -d > "$job_dir/runner.sh"
    printf '%s\n' "$restart" > "$job_dir/restart"
    printf '%s\n' "$restart_delay" > "$job_dir/restart-delay"
    : > "$output_file"
    chmod 700 "$job_dir/runner.sh"
    printf '%s\n' "$job_id" > "$job_dir/pending.next"
    mv "$job_dir/pending.next" "$job_dir/pending"
    write_result "$job_id" starting
    write_state starting
    printf '%s\n' "$job_id" > "$job_dir/current"
    rm -f "$job_dir/pending"
    current=$job_id
    state=starting
fi

if [ "$state" = starting ] && [ "$mode" = attach ]; then
    if ! tmux has-session -t "$session" 2>/dev/null; then
        if ! tmux new-session -d -s "$session" "$job_dir/runner.sh"; then
            write_state 'done 125'
            write_result "$job_id" 'done 125'
            flock -u 9
            echo "Could not start the remote tmux session." >&2
            exit 1
        fi
    fi
    read_state
fi

if [ "$state" = rebooting ] && ! tmux has-session -t "$session" 2>/dev/null; then
    echo "The update finished. Waiting for the host to reboot..."
    flock -u 9
    exit 75
fi

if [ "$state" = done ]; then
    flock -u 9
    if [ "$mode" = attach ] && [ -s "$output_file" ]; then
        cat "$output_file"
    fi
    if [ "$value" -eq 0 ]; then
        exit 0
    fi
    echo "Remote update failed with exit status $value." >&2
    exit 1
fi

if [ "$mode" = status ]; then
    flock -u 9
    printf '%s\n' 'yay-sys-tray-job-observed'
    starting_checks=0
    while true; do
        flock 9
        observed_current=
        if [ -f "$job_dir/current" ]; then
            read -r observed_current < "$job_dir/current" || true
        fi
        if [ "$observed_current" != "$job_id" ]; then
            requested_state=
            requested_value=
            if [ -f "$result_file" ]; then
                read -r requested_state requested_value < "$result_file" || true
            fi
            flock -u 9
            if [ "$requested_state" = done ] && [ "$requested_value" -eq 0 ]; then
                exit 0
            fi
            exit 1
        fi
        read_state
        if [ "$state" = done ]; then
            flock -u 9
            [ "$value" -eq 0 ] && exit 0
            exit 1
        fi
        if [ "$state" = running ] && ! tmux has-session -t "$session" 2>/dev/null; then
            write_state 'done 125'
            write_result "$job_id" 'done 125'
            flock -u 9
            exit 1
        fi
        if [ "$state" = starting ] && ! tmux has-session -t "$session" 2>/dev/null; then
            starting_checks=$((starting_checks + 1))
            if [ "$starting_checks" -ge 6 ]; then
                write_state 'done 125'
                write_result "$job_id" 'done 125'
                flock -u 9
                exit 1
            fi
        else
            starting_checks=0
        fi
        flock -u 9
        sleep 5
    done
fi

echo "Attached to the remote update. Press Ctrl+B, then D to stop watching; the update will continue."
flock -u 9
tmux attach-session -t "$session"
attach_rc=$?

# A normal detach or pane exit returns here. Read the durable result rather
# than trusting tmux's status, which does not carry the pane's exit code.
read_state
if [ "$state" = done ]; then
    if [ "$value" -eq 0 ]; then
        exit 0
    fi
    echo "Remote update failed with exit status $value." >&2
    exit 1
fi
[ "$attach_rc" -eq 0 ] && exit 74
exit 75
"#;

fn encode_remote_value(value: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(value)
}

fn remote_job_command(
    job_id: &str,
    update_command: &str,
    restart: bool,
    delay: u32,
    mode: &str,
) -> String {
    let controller = encode_remote_value(REMOTE_JOB_CONTROLLER);
    let runner = encode_remote_value(REMOTE_JOB_RUNNER);
    let command = encode_remote_value(update_command);
    let restart = u8::from(restart);

    // Every interpolated value is base64, numeric, or generated locally from
    // digits. Single quotes therefore protect the remote login shell without a
    // second escaping scheme for the update command itself.
    format!(
        "bash -c \"$(printf %s '{controller}' | base64 -d)\" yay-sys-tray \
         '{job_id}' '{command}' '{runner}' '{restart}' '{delay}' '{mode}'"
    )
}

#[derive(Clone)]
struct RemoteJob {
    id: String,
    target: String,
    update_command: String,
    restart: bool,
    delay: u32,
    timeout: u32,
}

impl RemoteJob {
    fn new(target: String, update_command: String, restart: bool, delay: u32, timeout: u32) -> Self {
        Self {
            id: remote_job_id(),
            target,
            update_command,
            restart,
            delay,
            timeout,
        }
    }

    fn command(&self, mode: &str) -> String {
        remote_job_command(
            &self.id,
            &self.update_command,
            self.restart,
            self.delay,
            mode,
        )
    }
}

fn remote_job_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{now}", std::process::id())
}

/// Entry point used by the copy of this executable launched inside the user's
/// terminal. It owns the SSH retry loop while the already-running tray process
/// remains free to serve the UI.
pub fn run_remote_update_watcher_from_args() -> Option<i32> {
    let mut args = std::env::args();
    let _program = args.next();
    if args.next().as_deref() != Some("--remote-update-watch") {
        return None;
    }

    let Some(target) = args.next() else {
        eprintln!("Missing remote update target.");
        return Some(2);
    };
    let Some(job_id) = args.next() else {
        eprintln!("Missing remote update job id.");
        return Some(2);
    };
    let Some(update_command) = args.next() else {
        eprintln!("Missing remote update command.");
        return Some(2);
    };
    let restart = args.next().as_deref() == Some("1");
    let delay = args.next().and_then(|value| value.parse().ok()).unwrap_or(0);
    let timeout = args.next().and_then(|value| value.parse().ok()).unwrap_or(10);

    let job = RemoteJob {
        id: job_id,
        target,
        update_command,
        restart,
        delay,
        timeout,
    };
    Some(watch_remote_update(&job))
}

fn watch_remote_update(job: &RemoteJob) -> i32 {
    watch_remote_update_with_ssh(std::ffi::OsStr::new("ssh"), job)
}

fn watch_remote_update_with_ssh(ssh: &std::ffi::OsStr, job: &RemoteJob) -> i32 {
    let remote_command = job.command("attach");
    let mut retry_delay = 1;

    loop {
        let status = std::process::Command::new(ssh)
            .args([
                "-tt",
                "-o",
                "ServerAliveInterval=5",
                "-o",
                "ServerAliveCountMax=3",
                "-o",
                &format!("ConnectTimeout={}", job.timeout.max(1)),
                "-o",
                "ConnectionAttempts=1",
                &job.target,
                &remote_command,
            ])
            .status();

        match status.and_then(|status| {
            status
                .code()
                .ok_or_else(|| std::io::Error::other("ssh ended without an exit status"))
        }) {
            Ok(0) => return 0,
            Ok(REMOTE_JOB_DETACHED) => {
                eprintln!("Stopped watching. The remote update is still running.");
                return 0;
            }
            Ok(REMOTE_JOB_RETRY) => {
                retry_delay = 1;
                eprintln!("Remote update still running. Reattaching in 1s...");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Ok(255) | Err(_) => {
                eprintln!(
                    "SSH connection lost. Reconnecting to the same update in {retry_delay}s..."
                );
                std::thread::sleep(std::time::Duration::from_secs(retry_delay));
                retry_delay = (retry_delay * 2).min(15);
            }
            Ok(code) => return code,
        }
    }
}

fn remote_watcher_argv(job: &RemoteJob) -> Vec<String> {
    let executable = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("yay-sys-tray"))
        .to_string_lossy()
        .into_owned();
    vec![
        executable,
        "--remote-update-watch".to_string(),
        job.target.clone(),
        job.id.clone(),
        job.update_command.clone(),
        u8::from(job.restart).to_string(),
        job.delay.to_string(),
        job.timeout.to_string(),
    ]
}

fn keep_waiting_for_unobserved_job(
    job_observed: bool,
    terminal_running: bool,
    post_terminal_probes: &mut u8,
) -> bool {
    if job_observed || terminal_running {
        *post_terminal_probes = 0;
        return true;
    }
    if *post_terminal_probes >= REMOTE_POST_TERMINAL_PROBES {
        return false;
    }
    *post_terminal_probes += 1;
    true
}

/// Wait for the durable remote state without attaching another tmux client.
/// This task belongs to the tray process, so closing the visible terminal does
/// not make the app report completion before the package transaction ends.
async fn wait_for_remote_job(
    job: &RemoteJob,
    terminal_running: &std::sync::atomic::AtomicBool,
) {
    let remote_command = job.command("status");
    let mut retry_delay = 1;
    let mut job_observed = false;
    let mut post_terminal_probes = 0;

    loop {
        let status = Command::new("ssh")
            .args([
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "ServerAliveInterval=5",
                "-o",
                "ServerAliveCountMax=3",
                "-o",
                &format!("ConnectTimeout={}", job.timeout.max(1)),
                "-o",
                "ConnectionAttempts=1",
                &job.target,
                &remote_command,
            ])
            .output()
            .await;

        let (code, observed) = status
            .map(|status| {
                (
                    status.status.code().unwrap_or(255),
                    String::from_utf8_lossy(&status.stdout).contains(REMOTE_JOB_OBSERVED),
                )
            })
            .unwrap_or((255, false));
        job_observed |= observed;
        let terminal_running = terminal_running.load(std::sync::atomic::Ordering::Acquire);
        match code {
            0 | 1 | 73 | 127 => return,
            REMOTE_JOB_NOT_STARTED => {
                if !keep_waiting_for_unobserved_job(
                    job_observed,
                    terminal_running,
                    &mut post_terminal_probes,
                ) {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            REMOTE_JOB_RETRY => {
                job_observed = true;
                retry_delay = 1;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            255 => {
                if !keep_waiting_for_unobserved_job(
                    job_observed,
                    terminal_running,
                    &mut post_terminal_probes,
                ) {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(15);
            }
            _ => return,
        }
    }
}

/// Full-system update for a remote host.
///
/// yay covers repo and AUR packages in one pass, so it is preferred wherever
/// the host has it. Plain `pacman -Syu` skips foreign packages entirely, so on
/// a host without yay the AUR updates this app reports cannot be applied at
/// all. `aur_pending` is how many of them the last check found: when the
/// fallback runs with any of those outstanding it says so first, because
/// pacman's own "there is nothing to do" reads as "already up to date" (#29).
/// The yay check runs on the host, so no extra probe round-trip is needed.
fn remote_full_update_cmd(noconfirm: bool, aur_pending: usize) -> String {
    let flag = if noconfirm { " --noconfirm" } else { "" };
    let pacman = format!("sudo pacman -Syu{flag}");
    let without_yay = if aur_pending > 0 {
        // Repo packages still update — the warning qualifies the run rather
        // than replacing it, matching how remote_install_cmd handles a mixed
        // selection.
        format!(
            "echo 'yay is not installed on this host, so its {aur_pending} AUR update(s) \
             cannot be applied from here; updating repo packages only' >&2; {pacman}"
        )
    } else {
        pacman
    };
    format!("if command -v yay >/dev/null 2>&1; then yay -Syu{flag}; else {without_yay}; fi")
}

/// Install a chosen set of packages on a remote host.
///
/// `yay -S` accepts repo and AUR names together, so the preferred branch just
/// passes everything. The pacman fallback is given repo names only on purpose:
/// `pacman -S` aborts the whole transaction on a name missing from the sync
/// databases, so including an AUR name there would stop the repo packages from
/// installing too. When that costs the user something, the command says so
/// rather than quietly installing a subset.
fn remote_install_cmd(selected: &[String], repo_only: &[String], noconfirm: bool) -> String {
    let flag = if noconfirm { " --noconfirm" } else { "" };
    let with_yay = format!("yay -S {}{flag}", selected.join(" "));

    let without_yay = if repo_only.is_empty() {
        "echo 'yay is not installed on this host, and every selected package is from the AUR' >&2; exit 1"
            .to_string()
    } else if repo_only.len() == selected.len() {
        format!("sudo pacman -S {}{flag}", repo_only.join(" "))
    } else {
        format!(
            "echo 'yay is not installed on this host, skipping the selected AUR packages' >&2; \
             sudo pacman -S {}{flag}",
            repo_only.join(" ")
        )
    };

    format!("if command -v yay >/dev/null 2>&1; then {with_yay}; else {without_yay}; fi")
}

/// Passwordless installs can reboot without sudo (a NOPASSWD systemctl call).
fn local_reboot_cmd(passwordless: bool) -> &'static str {
    if passwordless {
        "systemctl reboot"
    } else {
        "sudo reboot"
    }
}

/// What kind of run a terminal was carrying. Both kinds re-check the target
/// afterwards, but only an update run can satisfy "close the window after
/// updating" — removing a package is not an update.
#[derive(Clone, Copy)]
enum FinishedAction {
    Update,
    Remove,
}

impl FinishedAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Remove => "remove",
        }
    }
}

struct TermCfg {
    terminal: String,
    noconfirm: bool,
    hold: bool,
    passwordless: bool,
    delay: u32,
    ssh_user: String,
    ssh_timeout: u32,
}

async fn term_cfg(app_handle: &tauri::AppHandle) -> TermCfg {
    let state = app_handle.state::<AppState>();
    let config = state.config.read().await;
    TermCfg {
        terminal: config.terminal.clone(),
        noconfirm: config.noconfirm,
        hold: config.hold_terminal,
        passwordless: config.passwordless_updates,
        delay: config.restart_delay_seconds,
        ssh_user: config.tailscale_ssh_user.clone(),
        ssh_timeout: config.tailscale_timeout,
    }
}

/// Launch a local full system update in a terminal.
pub async fn run_local_update(app_handle: tauri::AppHandle, restart: bool) {
    let cfg = term_cfg(&app_handle).await;
    let prefix = terminal_prefix(&cfg.terminal, Some("Updating: local"), cfg.hold);

    let yay_cmd = if restart {
        let reboot = local_reboot_cmd(cfg.passwordless);
        let cmd = build_shell_cmd("yay -Syu", cfg.noconfirm, Some((reboot, cfg.delay)));
        vec!["bash".to_string(), "-c".to_string(), cmd]
    } else {
        let mut cmd = vec!["yay".to_string(), "-Syu".to_string()];
        if cfg.noconfirm {
            cmd.push("--noconfirm".to_string());
        }
        cmd
    };

    spawn_with(app_handle, &cfg.terminal, prefix, yay_cmd, "local".to_string(), FinishedAction::Update).await;
}

/// Update only the selected local packages (`yay -S <pkgs>`).
pub async fn run_local_update_packages(
    app_handle: tauri::AppHandle,
    packages: Vec<String>,
    restart: bool,
) {
    if packages.is_empty() {
        return run_local_update(app_handle, restart).await;
    }
    let cfg = term_cfg(&app_handle).await;
    let prefix = terminal_prefix(&cfg.terminal, Some("Updating: selected"), cfg.hold);

    let yay_cmd = if restart {
        let reboot = local_reboot_cmd(cfg.passwordless);
        let base = format!("yay -S {}", packages.join(" "));
        let cmd = build_shell_cmd(&base, cfg.noconfirm, Some((reboot, cfg.delay)));
        vec!["bash".to_string(), "-c".to_string(), cmd]
    } else {
        // No reboot chain needed, so pass packages as separate argv (no shell).
        let mut cmd = vec!["yay".to_string(), "-S".to_string()];
        cmd.extend(packages);
        if cfg.noconfirm {
            cmd.push("--noconfirm".to_string());
        }
        cmd
    };

    spawn_with(app_handle, &cfg.terminal, prefix, yay_cmd, "local".to_string(), FinishedAction::Update).await;
}

/// How many of a host's pending updates came from the AUR at its last check.
/// Read from the stored results rather than probed, so the warning the fallback
/// prints names the same updates the window is showing.
async fn pending_aur_count(app_handle: &tauri::AppHandle, hostname: &str) -> usize {
    let tray_state = app_handle.state::<TrayState>();
    let hosts = tray_state.remote_results.read().await;
    hosts
        .iter()
        .find(|h| h.hostname == hostname)
        .map(|h| h.updates.iter().filter(|u| u.is_aur()).count())
        .unwrap_or(0)
}

/// Launch a remote full system update via SSH in a terminal.
pub async fn run_remote_update(app_handle: tauri::AppHandle, hostname: &str, restart: bool) {
    let cfg = term_cfg(&app_handle).await;
    let aur_pending = pending_aur_count(&app_handle, hostname).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix = terminal_prefix(&cfg.terminal, Some(&format!("Updating: {hostname}")), cfg.hold);

    let cmd = remote_full_update_cmd(cfg.noconfirm, aur_pending);

    let job = RemoteJob::new(target, cmd, restart, cfg.delay, cfg.ssh_timeout);
    let watcher = remote_watcher_argv(&job);
    spawn_remote_with(app_handle, &cfg.terminal, prefix, watcher, job, hostname.to_string()).await;
}

/// Update only the selected packages on a remote host. `selected` is every
/// chosen package; `repo_only` is the subset that lives in a sync database,
/// which is all the pacman fallback may be given.
pub async fn run_remote_update_packages(
    app_handle: tauri::AppHandle,
    hostname: &str,
    selected: Vec<String>,
    repo_only: Vec<String>,
    restart: bool,
) {
    if selected.is_empty() {
        return run_remote_update(app_handle, hostname, restart).await;
    }
    let cfg = term_cfg(&app_handle).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix =
        terminal_prefix(&cfg.terminal, Some(&format!("Updating: {hostname} (selected)")), cfg.hold);

    let cmd = remote_install_cmd(&selected, &repo_only, cfg.noconfirm);

    let job = RemoteJob::new(target, cmd, restart, cfg.delay, cfg.ssh_timeout);
    let watcher = remote_watcher_argv(&job);
    spawn_remote_with(app_handle, &cfg.terminal, prefix, watcher, job, hostname.to_string()).await;
}

/// Remove a local package in a terminal.
pub async fn run_remove(app_handle: tauri::AppHandle, package: &str, flags: &str) {
    let cfg = term_cfg(&app_handle).await;
    let prefix = terminal_prefix(&cfg.terminal, Some(&format!("Removing: {package}")), cfg.hold);

    let mut yay_cmd = vec!["yay".to_string(), format!("-{flags}"), package.to_string()];
    if cfg.noconfirm {
        yay_cmd.push("--noconfirm".to_string());
    }

    spawn_with(app_handle, &cfg.terminal, prefix, yay_cmd, "local".to_string(), FinishedAction::Remove).await;
}

/// Remove a package on a remote host via SSH.
pub async fn run_remote_remove(
    app_handle: tauri::AppHandle,
    hostname: &str,
    package: &str,
    flags: &str,
) {
    let cfg = term_cfg(&app_handle).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix =
        terminal_prefix(&cfg.terminal, Some(&format!("Removing: {package} ({hostname})")), cfg.hold);

    let base = format!("sudo pacman -{flags} {package}");
    let cmd = build_shell_cmd(&base, cfg.noconfirm, None);

    spawn_with(app_handle, &cfg.terminal, prefix, ssh_argv(target, cmd), hostname.to_string(), FinishedAction::Remove).await;
}

async fn spawn_with(
    app_handle: tauri::AppHandle,
    terminal: &str,
    prefix: Vec<String>,
    cmd: Vec<String>,
    scope: String,
    action: FinishedAction,
) {
    let mut full = prefix;
    full.extend(wrap_command(terminal, cmd));
    spawn_and_wait(app_handle, full, scope, action).await;
}

async fn spawn_remote_with(
    app_handle: tauri::AppHandle,
    terminal: &str,
    prefix: Vec<String>,
    cmd: Vec<String>,
    job: RemoteJob,
    scope: String,
) {
    let mut full = prefix;
    full.extend(wrap_command(terminal, cmd));
    if full.is_empty() {
        return;
    }

    let program = full[0].clone();
    let args = &full[1..];
    match Command::new(&program).args(args).spawn() {
        Ok(mut terminal_child) => {
            let terminal_running =
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let terminal_state = terminal_running.clone();
            tauri::async_runtime::spawn(async move {
                let _ = terminal_child.wait().await;
                terminal_state.store(false, std::sync::atomic::Ordering::Release);
            });

            tauri::async_runtime::spawn(async move {
                wait_for_remote_job(&job, &terminal_running).await;
                let _ = app_handle.emit(
                    "update-finished",
                    serde_json::json!({ "scope": scope, "action": "update" }),
                );
            });
        }
        Err(e) => log::error!("Failed to spawn terminal: {e}"),
    }
}

/// Spawn a terminal command, wait for it to finish, then emit update-finished
/// carrying the scope ("local" or a hostname) so only that target gets
/// re-checked rather than the whole fleet, plus the action that ran so a
/// removal is never mistaken for a completed update.
async fn spawn_and_wait(
    app_handle: tauri::AppHandle,
    cmd: Vec<String>,
    scope: String,
    action: FinishedAction,
) {
    if cmd.is_empty() {
        return;
    }
    let program = cmd[0].clone();
    let args: Vec<String> = cmd[1..].to_vec();

    match Command::new(&program).args(&args).spawn() {
        Ok(mut child) => {
            tauri::async_runtime::spawn(async move {
                let _ = child.wait().await;
                let _ = app_handle.emit(
                    "update-finished",
                    serde_json::json!({ "scope": scope, "action": action.as_str() }),
                );
            });
        }
        Err(e) => log::error!("Failed to spawn terminal: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_update_prefers_yay_and_falls_back_to_pacman() {
        let cmd = remote_full_update_cmd(false, 0);
        assert!(cmd.contains("command -v yay"));
        assert!(cmd.contains("yay -Syu"));
        // pacman -Syu alone would never update the AUR packages this app now
        // reports for remote hosts, so yay must be the preferred branch.
        assert!(cmd.contains("sudo pacman -Syu"));
        assert!(!cmd.contains("--noconfirm"));
        assert_eq!(remote_full_update_cmd(true, 0).matches("--noconfirm").count(), 2);
    }

    #[test]
    fn full_update_without_aur_updates_stays_quiet() {
        // Nothing is being skipped, so a warning would be noise on every host
        // that simply has no AUR packages installed.
        let cmd = remote_full_update_cmd(false, 0);
        assert!(!cmd.contains("echo"));
    }

    #[test]
    fn helperless_full_update_names_the_aur_updates_it_cannot_apply() {
        // Without this, pacman prints "there is nothing to do" and the host
        // reads as up to date while its AUR updates sit there (#29).
        let cmd = remote_full_update_cmd(false, 3);
        let (with_yay, fallback) = cmd.split_once("else").expect("fallback branch");
        assert!(!with_yay.contains("echo"));
        assert!(fallback.contains("3 AUR update(s)"));
        assert!(fallback.contains(">&2"));
        // The repo half is still applied — the warning qualifies the run.
        assert!(fallback.contains("sudo pacman -Syu"));
    }

    #[test]
    fn helperless_warning_still_carries_noconfirm_once_per_branch() {
        let cmd = remote_full_update_cmd(true, 1);
        assert_eq!(cmd.matches("--noconfirm").count(), 2);
    }

    #[test]
    fn install_passes_everything_to_yay() {
        let selected = vec!["repo-pkg".to_string(), "aur-pkg".to_string()];
        let repo_only = vec!["repo-pkg".to_string()];
        let cmd = remote_install_cmd(&selected, &repo_only, false);
        assert!(cmd.contains("yay -S repo-pkg aur-pkg"));
    }

    #[test]
    fn pacman_fallback_never_receives_an_aur_name() {
        // `pacman -S` aborts the whole transaction on a name it can't resolve,
        // so an AUR name in the fallback would block the repo updates too.
        let selected = vec!["repo-pkg".to_string(), "aur-pkg".to_string()];
        let repo_only = vec!["repo-pkg".to_string()];
        let cmd = remote_install_cmd(&selected, &repo_only, false);
        let fallback = cmd.split("else").nth(1).expect("fallback branch");
        assert!(fallback.contains("sudo pacman -S repo-pkg"));
        assert!(!fallback.contains("aur-pkg"));
        assert!(fallback.contains("skipping the selected AUR packages"));
    }

    #[test]
    fn all_aur_selection_without_yay_fails_loudly() {
        // Nothing pacman can do here, and silently succeeding would leave the
        // user thinking the update ran.
        let selected = vec!["aur-pkg".to_string()];
        let cmd = remote_install_cmd(&selected, &[], false);
        let fallback = cmd.split("else").nth(1).expect("fallback branch");
        assert!(fallback.contains("exit 1"));
        assert!(!fallback.contains("pacman -S "));
    }

    #[test]
    fn repo_only_selection_has_no_warning_noise() {
        let selected = vec!["a".to_string(), "b".to_string()];
        let cmd = remote_install_cmd(&selected, &selected, true);
        assert!(cmd.contains("sudo pacman -S a b --noconfirm"));
        assert!(!cmd.contains("skipping"));
    }

    #[test]
    fn gnome_terminal_waits_for_the_command() {
        // Its CLI hands the window to the terminal server and returns straight
        // away otherwise, which would report the update as finished the moment
        // it started.
        let prefix = terminal_prefix("gnome-terminal", Some("Updating: local"), true);
        assert_eq!(prefix, vec!["gnome-terminal", "--wait", "--"]);
    }

    #[test]
    fn ptyxis_launches_without_a_wait_flag() {
        // Ptyxis looks like gnome-terminal but does not behave like it: a `--`
        // command implies single-instance mode, so the process already blocks
        // until the command exits (#38). It has no `--wait` option, and adding
        // one would be rejected as unknown and stop the terminal launching.
        let prefix = terminal_prefix("ptyxis", Some("Updating: local"), true);
        assert_eq!(prefix, vec!["ptyxis", "--title", "Updating: local", "--"]);
        assert!(!prefix.contains(&"--wait".to_string()));
    }

    #[test]
    fn ptyxis_commands_cannot_fail_fast_enough_to_wedge_the_window() {
        // Ptyxis holds the window open with a "Process Exited" banner when a
        // command exits non-zero within half a second, which keeps the terminal
        // process alive and so keeps update-finished from ever firing (#43).
        let cmd = wrap_command("ptyxis", vec!["yay".into(), "-Syu".into()]);
        assert_eq!(
            cmd,
            vec!["bash", "-c", SLOW_FAILURE_SCRIPT, "yay-sys-tray", "yay", "-Syu"]
        );
    }

    #[test]
    fn ghostty_takes_its_title_joined_to_the_flag() {
        // Ghostty's flags are config keys, and its CLI parser refuses a value
        // given as a separate argument, so `--title Updating: local` would be
        // reported as a missing value and the window would open untitled.
        let prefix = terminal_prefix("ghostty", Some("Updating: local"), true);
        assert_eq!(
            prefix,
            vec![
                "ghostty",
                "--abnormal-command-exit-runtime=0",
                "--wait-after-command=true",
                "--title=Updating: local",
                "-e",
            ]
        );
    }

    #[test]
    fn ghostty_is_told_not_to_hold_rather_than_left_to_its_config() {
        // Every Ghostty flag is a config key the user may already have set, so
        // omitting `wait-after-command` doesn't mean "don't hold" — it means
        // "whatever their ghostty config says", which would leave the window
        // (and update-finished) waiting on a keypress. `-e` stays last:
        // everything after it is the command.
        assert_eq!(
            terminal_prefix("ghostty", None, false),
            vec!["ghostty", "--abnormal-command-exit-runtime=0", "--wait-after-command=false", "-e"]
        );
    }

    #[test]
    fn only_ghostty_states_the_hold_it_does_not_want() {
        // Elsewhere the hold flag is a CLI-only switch with no configured
        // default, so a "false" form either doesn't exist or would be rejected.
        assert_eq!(terminal_prefix("xterm", None, false), vec!["xterm", "-e"]);
        assert_eq!(terminal_prefix("konsole", None, false), vec!["konsole", "-e"]);
    }

    #[test]
    fn ghostty_commands_cannot_fail_fast_enough_to_wedge_the_window() {
        // Ghostty treats a non-zero exit within abnormal-command-exit-runtime
        // as a failed spawn and holds the window open with an error message,
        // which keeps update-finished from firing — the same trap Ptyxis sets
        // (#43). The launch pins that threshold to 0, and the wrapper covers
        // the one exit fast enough to still meet it.
        let cmd = wrap_command("ghostty", vec!["yay".into(), "-Syu".into()]);
        assert_eq!(
            cmd,
            vec!["bash", "-c", SLOW_FAILURE_SCRIPT, "yay-sys-tray", "yay", "-Syu"]
        );
    }

    #[test]
    fn other_terminals_run_the_command_unwrapped() {
        // The wrapper is a workaround for one terminal's heuristic, not a tax
        // every terminal pays.
        let cmd = vec!["yay".to_string(), "-Syu".to_string()];
        assert_eq!(wrap_command("kitty", cmd.clone()), cmd);
        assert_eq!(wrap_command("gnome-terminal", cmd.clone()), cmd);
    }

    #[test]
    fn the_wrapper_keeps_the_command_exit_status() {
        // The delay must not swallow the failure it is delaying, and arguments
        // must reach the command as-is rather than through another round of
        // shell quoting.
        let run = |args: &[&str]| {
            std::process::Command::new("bash")
                .arg("-c")
                .arg(SLOW_FAILURE_SCRIPT)
                .arg("yay-sys-tray")
                .args(args)
                .output()
                .expect("bash is available")
        };

        let ok = run(&["printf", "%s", "a b; c"]);
        assert_eq!(ok.status.code(), Some(0));
        assert_eq!(String::from_utf8_lossy(&ok.stdout), "a b; c");

        assert_eq!(run(&["sh", "-c", "exit 7"]).status.code(), Some(7));
    }

    #[test]
    fn terminals_that_block_get_no_wait_flag() {
        // A flag the terminal doesn't accept makes it reject the whole command
        // line and never launch, so it goes only where it's known to exist.
        let prefix = terminal_prefix("kitty", None, false);
        assert_eq!(prefix, vec!["kitty"]);
        assert!(!terminal_prefix("konsole", None, true).contains(&"--wait".to_string()));
    }

    #[test]
    fn an_unobserved_job_gets_a_bounded_post_terminal_probe_window() {
        let mut probes = 0;
        for _ in 0..REMOTE_POST_TERMINAL_PROBES {
            assert!(keep_waiting_for_unobserved_job(false, false, &mut probes));
        }
        assert!(!keep_waiting_for_unobserved_job(false, false, &mut probes));

        assert!(keep_waiting_for_unobserved_job(false, true, &mut probes));
        assert_eq!(probes, 0);
        assert!(keep_waiting_for_unobserved_job(true, false, &mut probes));
        assert_eq!(probes, 0);
    }


    #[cfg(unix)]
    struct RemoteJobFixture {
        root: std::path::PathBuf,
        fake_bin: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl RemoteJobFixture {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;

            let root = std::env::temp_dir().join(format!(
                "yay-sys-tray-remote-job-{}-{}",
                std::process::id(),
                remote_job_id()
            ));
            let fake_bin = root.join("bin");
            std::fs::create_dir_all(&fake_bin).unwrap();

            let tmux = fake_bin.join("tmux");
            std::fs::write(
                &tmux,
                r#"#!/usr/bin/env bash
session="$XDG_STATE_HOME/fake-tmux-session"
printf '%s\n' "$*" >> "$XDG_STATE_HOME/tmux-calls"
case "$1" in
    has-session) test -f "$session" ;;
    new-session)
        touch "$session"
        TMUX_PANE=%0 "$5"
        rm -f "$session"
        exit 0
        ;;
    attach-session) exit 0 ;;
    set-window-option) exit 0 ;;
    *) exit 2 ;;
esac
"#,
            )
            .unwrap();
            std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o700)).unwrap();

            Self { root, fake_bin }
        }

        fn controller(&self, job_id: &str, update_command: &str) -> std::process::Output {
            self.controller_mode(job_id, update_command, "attach")
        }

        fn controller_mode(
            &self,
            job_id: &str,
            update_command: &str,
            mode: &str,
        ) -> std::process::Output {
            let path = format!(
                "{}:{}",
                self.fake_bin.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            std::process::Command::new("bash")
                .arg("-c")
                .arg(remote_job_command(job_id, update_command, false, 0, mode))
                .env("XDG_STATE_HOME", &self.root)
                .env("PATH", path)
                .output()
                .unwrap()
        }

        fn state_dir(&self) -> std::path::PathBuf {
            self.root.join("yay-sys-tray/remote-update")
        }
    }

    #[cfg(unix)]
    impl Drop for RemoteJobFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(unix)]
    #[test]
    fn reconnecting_to_a_completed_request_does_not_run_it_twice() {
        let fixture = RemoteJobFixture::new();
        let count = fixture.root.join("count");
        let update = format!("printf x >> '{}'", count.display());

        assert!(fixture.controller("100-10", &update).status.success());
        assert!(fixture.controller("100-10", &update).status.success());

        assert_eq!(std::fs::read_to_string(count).unwrap(), "x");
        assert_eq!(
            std::fs::read_to_string(fixture.state_dir().join("state")).unwrap(),
            "done 0\n"
        );
        assert!(
            std::fs::read_to_string(fixture.root.join("tmux-calls"))
                .unwrap()
                .contains("set-window-option -t %0 remain-on-exit off")
        );
    }

    #[cfg(unix)]
    #[test]
    fn remote_job_files_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = RemoteJobFixture::new();
        assert!(fixture.controller("100-15", "exit 0").status.success());

        let state_dir = fixture.state_dir();
        assert_eq!(
            std::fs::metadata(&state_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(state_dir.join("command.b64"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_remote_job_keeps_its_result_for_reconnects() {
        let fixture = RemoteJobFixture::new();

        let first = fixture.controller("100-20", "exit 7");
        let second = fixture.controller("100-20", "exit 0");

        assert_eq!(first.status.code(), Some(1));
        assert_eq!(second.status.code(), Some(1));
        assert_eq!(
            std::fs::read_to_string(fixture.state_dir().join("state")).unwrap(),
            "done 7\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reconnect_reattaches_while_reboot_is_waiting_for_input() {
        let fixture = RemoteJobFixture::new();
        let state_dir = fixture.state_dir();
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("current"), "100-30\n").unwrap();
        let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
        std::fs::write(
            state_dir.join("state"),
            format!("rebooting {}\n", boot_id.trim()),
        )
        .unwrap();
        std::fs::write(fixture.root.join("fake-tmux-session"), "").unwrap();

        let result = fixture.controller("100-30", "exit 0");

        assert_eq!(result.status.code(), Some(REMOTE_JOB_DETACHED));
        assert!(String::from_utf8_lossy(&result.stdout).contains("Attached to the remote update"));
    }

    #[cfg(unix)]
    #[test]
    fn status_poll_does_not_start_a_missing_job() {
        let fixture = RemoteJobFixture::new();

        let result = fixture.controller_mode("100-40", "exit 0", "status");

        assert_eq!(result.status.code(), Some(REMOTE_JOB_NOT_STARTED));
        assert!(!fixture.root.join("tmux-calls").exists());
    }

    #[cfg(unix)]
    #[test]
    fn status_poll_waits_for_a_pending_activation() {
        let fixture = RemoteJobFixture::new();
        let state_dir = fixture.state_dir();
        std::fs::create_dir_all(state_dir.join("results")).unwrap();
        std::fs::write(state_dir.join("current"), "100-1\n").unwrap();
        std::fs::write(state_dir.join("state"), "starting\n").unwrap();
        std::fs::write(state_dir.join("pending"), "100-42\n").unwrap();
        std::fs::write(state_dir.join("results/100-42"), "starting\n").unwrap();

        let result = fixture.controller_mode("100-42", "exit 0", "status");

        assert_eq!(result.status.code(), Some(REMOTE_JOB_NOT_STARTED));
        assert_eq!(
            std::fs::read_to_string(state_dir.join("current")).unwrap(),
            "100-1\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reconnect_completes_an_interrupted_request_activation() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = RemoteJobFixture::new();
        let state_dir = fixture.state_dir();
        let count = fixture.root.join("activation-count");
        let update = format!("printf x >> '{}'", count.display());

        std::fs::create_dir_all(state_dir.join("results")).unwrap();
        std::fs::write(state_dir.join("current"), "100-1\n").unwrap();
        std::fs::write(state_dir.join("state"), "starting\n").unwrap();
        std::fs::write(state_dir.join("pending"), "100-45\n").unwrap();
        std::fs::write(state_dir.join("results/100-45"), "starting\n").unwrap();
        std::fs::write(state_dir.join("command.b64"), encode_remote_value(&update)).unwrap();
        std::fs::write(state_dir.join("runner.sh"), REMOTE_JOB_RUNNER).unwrap();
        std::fs::write(state_dir.join("restart"), "0\n").unwrap();
        std::fs::write(state_dir.join("restart-delay"), "0\n").unwrap();
        std::fs::write(state_dir.join("output-100-45.log"), "").unwrap();
        std::fs::set_permissions(
            state_dir.join("runner.sh"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();

        let result = fixture.controller("100-45", "exit 99");

        assert!(result.status.success());
        assert_eq!(std::fs::read_to_string(count).unwrap(), "x");
        assert!(!state_dir.join("pending").exists());
    }

    #[cfg(unix)]
    #[test]
    fn an_old_request_cannot_run_again_after_a_newer_job() {
        let fixture = RemoteJobFixture::new();
        let count = fixture.root.join("count");

        assert!(fixture
            .controller("100-1", &format!("printf a >> '{}'", count.display()))
            .status
            .success());
        assert!(fixture
            .controller("100-2", &format!("printf b >> '{}'", count.display()))
            .status
            .success());
        assert!(fixture
            .controller("100-1", &format!("printf c >> '{}'", count.display()))
            .status
            .success());

        assert_eq!(std::fs::read_to_string(count).unwrap(), "ab");
    }

    #[cfg(unix)]
    #[test]
    fn reconnect_finishes_after_the_boot_id_changes() {
        let fixture = RemoteJobFixture::new();
        let state_dir = fixture.state_dir();
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("current"), "100-50\n").unwrap();
        std::fs::write(state_dir.join("state"), "rebooting previous-boot\n").unwrap();

        let result = fixture.controller("100-50", "exit 0");

        assert!(result.status.success());
        assert_eq!(
            std::fs::read_to_string(state_dir.join("state")).unwrap(),
            "done 0\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn ssh_retry_reattaches_with_the_same_job_id() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = RemoteJobFixture::new();
        let ssh = fixture.fake_bin.join("ssh");
        let ssh_script = r#"#!/usr/bin/env bash
export XDG_STATE_HOME='__STATE_HOME__'
export PATH='__FAKE_BIN__':"$PATH"
attempt="$XDG_STATE_HOME/ssh-attempt"
count=0
test -f "$attempt" && read -r count < "$attempt"
count=$((count + 1))
printf '%s\n' "$count" > "$attempt"
remote_command=${!#}
bash -c "$remote_command"
test "$count" -gt 1 || exit 255
"#
        .replace("__STATE_HOME__", &fixture.root.to_string_lossy())
        .replace("__FAKE_BIN__", &fixture.fake_bin.to_string_lossy());
        std::fs::write(
            &ssh,
            ssh_script,
        )
        .unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700)).unwrap();

        let count = fixture.root.join("update-count");
        let update = format!("printf x >> '{}'", count.display());

        let job = RemoteJob {
            id: "100-60".to_string(),
            target: "test-host".to_string(),
            update_command: update,
            restart: false,
            delay: 0,
            timeout: 1,
        };
        let result = watch_remote_update_with_ssh(ssh.as_os_str(), &job);

        assert_eq!(result, 0);
        assert_eq!(std::fs::read_to_string(count).unwrap(), "x");
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("ssh-attempt")).unwrap(),
            "2\n"
        );
    }
}
