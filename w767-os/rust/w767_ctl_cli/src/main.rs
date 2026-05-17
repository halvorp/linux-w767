//! w767_ctl_cli — on-device CLI for the w767_ctl daemon.
//!
//! Connects to `/run/w767/ctl.sock` via UDS. Intended to be used over SSH:
//!   ssh root@<device> 'w767_ctl_cli reboot warm'
//!   ssh root@<device> 'w767_ctl_cli modprobe ath10k_pci && w767_ctl_cli dmesg-tail 100'

use clap::{Parser, Subcommand};
use sol_ipc::transport::{connect_tarpc_raw, make_tarpc_transport};
use w767_ctl::{RebootKind, W767CtlClient};

#[derive(Parser)]
#[command(version, about = "w767-os control CLI (UDS client)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print daemon version.
    Version,
    /// Print a machine status report (uptime, mem, interfaces).
    Status,
    /// List loaded kernel modules (parsed /proc/modules).
    Lsmod,
    /// modprobe a module by name.
    Modprobe { name: String },
    /// rmmod a module by name.
    Rmmod {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Dump last N kernel log lines.
    DmesgTail {
        #[arg(default_value_t = 100)]
        n: usize,
    },
    /// Read a file's contents.
    Read { path: String },
    /// Write a file (content from stdin).
    Write { path: String },
    /// Run an arbitrary command.
    Run {
        #[arg(long, default_value_t = 30)]
        timeout: u32,
        /// Everything after -- becomes the argv.
        argv: Vec<String>,
    },
    /// Reboot the machine.
    Reboot {
        #[arg(value_enum)]
        kind: RebootMode,
    },
    /// List /sys/fs/pstore entries.
    PstoreList,
    /// Read one /sys/fs/pstore entry.
    PstoreRead { name: String },
}

#[derive(Copy, Clone, clap::ValueEnum)]
enum RebootMode { Warm, Cold, Kexec }

impl From<RebootMode> for RebootKind {
    fn from(m: RebootMode) -> Self {
        match m {
            RebootMode::Warm  => RebootKind::Warm,
            RebootMode::Cold  => RebootKind::Cold,
            RebootMode::Kexec => RebootKind::Kexec,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();

    let stream = connect_tarpc_raw(sol_ipc::SOCK_CTL)
        .await
        .map_err(|e| anyhow::anyhow!("connect {}: {}", sol_ipc::socket_path(sol_ipc::SOCK_CTL), e))?;
    let transport = make_tarpc_transport(stream);
    let client = W767CtlClient::new(tarpc::client::Config::default(), transport).spawn();
    let ctx = sol_ipc::rpc_context_secs(60);

    match cli.cmd {
        Cmd::Version => {
            println!("{}", client.version(ctx).await?);
        }
        Cmd::Status => {
            let s = client.status(ctx).await?;
            println!("version:     {}", s.version);
            println!("kernel:      {}", s.kernel);
            println!("uptime:      {}s", s.uptime_secs);
            println!("loadavg:     {:.2} {:.2} {:.2}", s.loadavg[0], s.loadavg[1], s.loadavg[2]);
            println!("memtotal:    {} KB", s.mem_total_kb);
            println!("memfree:     {} KB", s.mem_free_kb);
            println!("interfaces:");
            for i in s.iface_summary {
                let up = if i.is_up { "UP" } else { "down" };
                let carrier = if i.has_carrier { "LINK" } else { "no-link" };
                println!("  {:<10} {:<4} {:<8} {}", i.name, up, carrier, i.addrs.join(", "));
            }
        }
        Cmd::Lsmod => {
            for m in client.lsmod(ctx).await? {
                println!("{:<28} {:>8}  refs={}  by=[{}]", m.name, m.size, m.refcount, m.used_by.join(","));
            }
        }
        Cmd::Modprobe { name } => {
            match client.modprobe(ctx, name.clone()).await? {
                Ok(out) => { if !out.is_empty() { print!("{out}"); } println!("OK: modprobe {name}"); }
                Err(e)  => { eprintln!("FAIL: {e}"); std::process::exit(1); }
            }
        }
        Cmd::Rmmod { name, force } => {
            match client.rmmod(ctx, name.clone(), force).await? {
                Ok(out) => { if !out.is_empty() { print!("{out}"); } println!("OK: rmmod {name}"); }
                Err(e)  => { eprintln!("FAIL: {e}"); std::process::exit(1); }
            }
        }
        Cmd::DmesgTail { n } => {
            for line in client.dmesg_tail(ctx, n).await? {
                println!("{line}");
            }
        }
        Cmd::Read { path } => {
            match client.read_file(ctx, path.clone()).await? {
                Ok(c) => print!("{c}"),
                Err(e) => { eprintln!("FAIL read {path}: {e}"); std::process::exit(1); }
            }
        }
        Cmd::Write { path } => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            match client.write_file(ctx, path.clone(), buf).await? {
                Ok(()) => println!("OK: wrote {path}"),
                Err(e) => { eprintln!("FAIL: {e}"); std::process::exit(1); }
            }
        }
        Cmd::Run { timeout, argv } => {
            if argv.is_empty() {
                eprintln!("error: empty argv (pass command after --)");
                std::process::exit(2);
            }
            let r = client.run(ctx, argv, timeout).await?;
            if !r.stdout.is_empty() { print!("{}", r.stdout); }
            if !r.stderr.is_empty() { eprint!("{}", r.stderr); }
            if r.timed_out { eprintln!("(timed out)"); }
            std::process::exit(r.exit_code);
        }
        Cmd::Reboot { kind } => {
            client.reboot(ctx, kind.into()).await?;
            println!("reboot({:?}) requested", kind as u8);
        }
        Cmd::PstoreList => {
            for e in client.pstore_list(ctx).await? { println!("{e}"); }
        }
        Cmd::PstoreRead { name } => {
            match client.pstore_read(ctx, name.clone()).await? {
                Ok(c) => print!("{c}"),
                Err(e) => { eprintln!("FAIL pstore_read {name}: {e}"); std::process::exit(1); }
            }
        }
    }
    Ok(())
}
