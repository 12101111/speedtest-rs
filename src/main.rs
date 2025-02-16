use log::{error, info, LevelFilter};
use simplelog::*;
use speedtest::*;
use std::error::Error;
use std::fs::OpenOptions;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(subcommand)]
    cmd: Command,
    /// Show verbose output
    #[structopt(short, long)]
    verbose: bool,
    /// Number of bytes to test (only used in upload or download test)
    #[structopt(short, long)]
    bytes: Option<usize>,
    /// Specify id of server to test, id can get from `list` command
    #[structopt(short, long)]
    id: Option<String>,
    /// Specify hostname of server to test
    #[structopt(short = "n", long)]
    host: Option<String>,
    /// Specify output path of log file
    #[structopt(short, long, parse(from_os_str))]
    log: Option<PathBuf>,
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

impl Command {
    fn display(&self, val: f64) -> String {
        match self {
            Command::Upload | Command::Download => format!("{} Mbps ({} MB/s)", val, val / 8.0),
            _ => format!("{} ms", val),
        }
    }
}

fn main() {
    let mut opt = Opt::from_args();
    let mut log_target: Vec<Box<dyn SharedLogger>> = Vec::new();
    let log_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();
    if opt.log.is_some() {
        let write_logger = WriteLogger::new(
            LevelFilter::Info,
            log_config.clone(),
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(opt.log.take().unwrap())
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
    match opt.cmd {
        Command::List => {
            for s in list_servers()? {
                if opt.verbose {
                    println!("{:?}", s);
                } else {
                    println!("{}", s);
                }
            }
        }
        _ => {
            // Get hostname to test. not used in `list` command.
            let host = if opt.id.is_some() {
                let id = opt.id.as_ref().unwrap();
                match list_servers()?.into_iter().find(|s| &s.id == id) {
                    Some(s) => {
                        info!("Select server: {} based on id: {}", s.sponsor, id);
                        Ok(s.host)
                    }
                    None => Err(format!("Can't find server with id {}", id)),
                }?
            } else if opt.host.is_some() {
                let host = opt.host.as_ref().unwrap().clone();
                info!("Select server: {} based on host settings", host);
                host
            } else {
                best_server()?.host
            };
            // Get running count
            let count = opt
                .count
                .unwrap_or(if let Command::Ping = opt.cmd { 3 } else { 1 });
            let mut result = 0.0;
            for i in 0..count {
                let res = match opt.cmd {
                    Command::Download => download(&host, opt.bytes.unwrap_or(100 * 1024 * 1024))?,
                    Command::Upload => upload(&host, opt.bytes.unwrap_or(50 * 1024 * 1024))?,
                    Command::Ping => ping_server(&host)?,
                    _ => unreachable!(),
                };
                result += res;
                info!("seq={:?} result={}", i + 1, opt.cmd.display(res));
            }
            println!(
                "{:?} result={}",
                opt.cmd,
                opt.cmd.display(result / count as f64)
            );
        }
    }
    Ok(())
}
