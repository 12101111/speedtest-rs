use log::{error, info, LevelFilter};
use simplelog::*;
use speedtest::*;
use std::error::Error;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::thread;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(subcommand)]
    cmd: Command,
    /// Show verbose output
    #[structopt(short, long)]
    verbose: bool,
    /// Specify output path of log file
    #[structopt(short, long, parse(from_os_str))]
    log: Option<PathBuf>,
    /// Use all servers instead of near servers
    #[structopt(short, long)]
    all: bool,
    /// Number of bytes to test (only used in upload or download test)
    #[structopt(short, long)]
    bytes: Option<usize>,
    /// Specify id of server to test, id can get from `list` command
    #[structopt(short, long)]
    id: Option<String>,
    /// Specify hostname of server to test
    #[structopt(short = "n", long)]
    host: Option<String>,
    /// Count of threads to test
    #[structopt(short, long)]
    thread: Option<usize>,
    /// Count of times to test
    #[structopt(short, long)]
    count: Option<usize>,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Lists available servers
    List,
    /// Upload test
    Upload,
    /// Download test
    Download,
    /// Ping test
    Ping,
}

fn main() {
    let opt = Opt::from_args();
    let mut log_target: Vec<Box<dyn SharedLogger>> = Vec::new();
    let log_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();
    if let Some(ref path) = &opt.log {
        let write_logger = WriteLogger::new(
            LevelFilter::Info,
            log_config.clone(),
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
                .unwrap(),
        );
        log_target.push(write_logger);
    }
    let level = if opt.verbose {
        LevelFilter::Info
    } else {
        LevelFilter::Warn
    };
    let term_logger = TermLogger::new(level, log_config, TerminalMode::Mixed).unwrap();
    log_target.push(term_logger);
    CombinedLogger::init(log_target).unwrap();
    if let Err(e) = run(opt) {
        error!("{}", e);
        std::process::exit(1);
    }
}

fn run(opt: Opt) -> Result<(), Box<dyn Error>> {
    // option `all`
    let list = if opt.all { Server::all } else { Server::near };

    // command `ping`
    if let Command::List = opt.cmd {
        for s in list()? {
            if opt.verbose {
                println!("{:?}", s);
            } else {
                println!("{}", s);
            }
        }
        return Ok(());
    }

    // Get hostname to test.
    let host = if let Some(id) = opt.id {
        match list()?.into_iter().find(|s| s.id == id) {
            Some(s) => {
                info!("Select server: {} based on id: {}", s.sponsor, id);
                info!("Server hostname: {}", s.host);
                Ok(s.host)
            }
            None => Err(format!("Can't find server with id {}", id)),
        }?
    } else if let Some(host) = opt.host {
        info!("Select server: {} based on host settings", host);
        host
    } else {
        Server::best()?.host
    };

    let count = opt
        .count
        .unwrap_or(if let Command::Ping = opt.cmd { 3 } else { 1 });
    info!("Test will run {} time(s)", count);

    if let Command::Ping = opt.cmd {
        let mut stream = connect(&host)?;
        let mut result = 0.0;
        for i in 0..count {
            let res = ping(&mut stream)?;
            result += res;
            info!("seq={:?} result={} ms", i + 1, res);
        }
        println!("{:?} result={} ms", opt.cmd, result / count as f64);
    }

    let bytes = opt.bytes.clone().unwrap_or(match opt.cmd {
        Command::Download => 128 * 1024 * 1024,
        Command::Upload => 40 * 1024 * 1024,
        _ => unreachable!(),
    });
    info!("{:?} Size: {} MB", opt.cmd, bytes as f64 / MB as f64);

    let mut result = 0.0;
    for i in 0..count {
        let res = if let Some(t) = opt.thread {
                match opt.cmd {
                    Command::Download => download_mt(host, bytes,t)?,
                    Command::Upload => upload_mt(host, bytes,t)?,
                    _ => unreachable!(),
                };
            unimplemented!()
        } else {
            let stream = connect(&host)?;
            match opt.cmd {
                Command::Download => download_st(stream, bytes)?,
                Command::Upload => upload_st(stream, bytes)?,
                _ => unreachable!(),
            }
        };
        result += res;
        info!("seq={:?} result={} Mbps ({} MB/s)", i + 1, res, res / 8.0);
        if i > 0 && i != count {
            thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    println!(
        "{:?} result={} Mbps ({} MB/s)",
        opt.cmd,
        result / count as f64,
        result / count as f64 / 8.0
    );
    Ok(())
}
