// ═══════════════════════════════════════════════════════════════════════════════
// w767-os bootlog — on-screen status painter for PID 1.
//
// Forked from sol-os/crates/sol_init/src/bootlog.rs (same approach: open
// /dev/tty0, paint ANSI SGR sequences; the VT layer renders to the eDP
// framebuffer via CONFIG_VT + CONFIG_FRAMEBUFFER_CONSOLE).
//
// Only the banner text changes ("w767-os" vs "SolOS"). The panic hook + glyph
// classifier + handoff helpers are kept verbatim.
// ═══════════════════════════════════════════════════════════════════════════════

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

static BOOT_TTY: Mutex<Option<File>> = Mutex::new(None);

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_DIM:   &str = "\x1b[2m";
const ANSI_BOLD:  &str = "\x1b[1m";

const FG_GREEN:  &str = "\x1b[38;5;114m";
const FG_YELLOW: &str = "\x1b[38;5;221m";
const FG_RED:    &str = "\x1b[38;5;203m";
const FG_CYAN:   &str = "\x1b[38;5;111m";
const FG_GREY:   &str = "\x1b[38;5;244m";

const BG_DEEP:      &str = "\x1b[48;5;234m";
const CLEAR_SCREEN: &str = "\x1b[2J\x1b[H";
const HIDE_CURSOR:  &str = "\x1b[?25l";
const SHOW_CURSOR:  &str = "\x1b[?25h";

#[derive(Copy, Clone)]
enum Glyph { Ok, Warn, Fail, Info }

impl Glyph {
    fn classify(line: &str) -> Self {
        if line.contains("[  OK  ]") || line.contains("READY") {
            Glyph::Ok
        } else if line.contains("[ WARN ]") || line.contains("WARNING") || line.contains("WARN:") {
            Glyph::Warn
        } else if line.contains("[ FAIL ]")
            || line.contains("[MISSING]")
            || line.contains("FATAL")
            || line.contains("ERROR")
            || line.contains("crashed")
        {
            Glyph::Fail
        } else {
            Glyph::Info
        }
    }
    fn colour(self) -> &'static str {
        match self {
            Glyph::Ok => FG_GREEN, Glyph::Warn => FG_YELLOW,
            Glyph::Fail => FG_RED, Glyph::Info => FG_CYAN,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Glyph::Ok => "  OK  ", Glyph::Warn => " WARN ",
            Glyph::Fail => " FAIL ", Glyph::Info => " INFO ",
        }
    }
}

pub fn init() {
    let mut guard = BOOT_TTY.lock().unwrap();
    if guard.is_some() { return; }

    let tty = ["/dev/tty0", "/dev/tty1", "/dev/console"]
        .iter()
        .find_map(|p| OpenOptions::new().write(true).open(p).ok());
    let Some(mut tty) = tty else { return; };

    let _ = write!(
        tty,
        "{reset}{bg}{clear}{hide}{bold}{cyan} w767-os {reset}{dim}{grey}  boot log{reset}\r\n\r\n",
        reset = ANSI_RESET, bg = BG_DEEP, clear = CLEAR_SCREEN, hide = HIDE_CURSOR,
        bold = ANSI_BOLD, cyan = FG_CYAN, dim = ANSI_DIM, grey = FG_GREY,
    );
    let _ = tty.flush();

    *guard = Some(tty);

    std::panic::set_hook(Box::new(|info| {
        let msg: String = if let Some(s) = info.payload().downcast_ref::<&'static str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "w767_init panic (non-string payload)".to_string()
        };
        let loc = info.location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());

        if let Ok(mut k) = OpenOptions::new().write(true).open("/dev/kmsg") {
            let _ = writeln!(k, "w767_init PANIC at {loc}: {msg}");
        }
        if let Ok(mut guard) = BOOT_TTY.lock() {
            if let Some(tty) = guard.as_mut() {
                let _ = write!(
                    tty,
                    "\r\n{reset}{red}{bold}  !!  w767_init PANIC  !!  {reset}\r\n\
                     {red}  at {loc}{reset}\r\n\
                     {red}  {msg}{reset}\r\n\r\n\
                     {grey}  (hold power button to force reset){reset}\r\n",
                    reset = ANSI_RESET, red = FG_RED, bold = ANSI_BOLD,
                    loc = loc, msg = msg, grey = FG_GREY,
                );
                let _ = tty.flush();
            }
        }
        loop { std::thread::sleep(std::time::Duration::from_secs(3600)); }
    }));
}

pub fn paint(line: &str) {
    let Ok(mut guard) = BOOT_TTY.lock() else { return; };
    let Some(tty) = guard.as_mut() else { return; };
    let glyph = Glyph::classify(line);
    let body = strip_known_prefix(line);
    let _ = write!(
        tty,
        "{reset}{col}[{lbl}]{reset} {grey}{body}{reset}\r\n",
        reset = ANSI_RESET, col = glyph.colour(), lbl = glyph.label(),
        grey = FG_GREY, body = body,
    );
    let _ = tty.flush();
}

fn strip_known_prefix(line: &str) -> &str {
    const PREFIXES: &[&str] = &[
        "[  OK  ]", "[ WARN ]", "[ FAIL ]", "[MISSING]", "[ INFO ]",
    ];
    let trimmed = line.trim_start();
    for p in PREFIXES {
        if let Some(rest) = trimmed.strip_prefix(p) {
            return rest.trim_start();
        }
    }
    trimmed
}

pub fn handoff() {
    let Ok(mut guard) = BOOT_TTY.lock() else { return; };
    let Some(tty) = guard.as_mut() else { return; };
    let _ = write!(
        tty,
        "\r\n{reset}{green}[  OK  ]{reset} {grey}Services up — see SSH output on host.{reset}\r\n{reset}{show}",
        reset = ANSI_RESET, green = FG_GREEN, grey = FG_GREY, show = SHOW_CURSOR,
    );
    let _ = tty.flush();
}
