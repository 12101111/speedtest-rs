use log::info;
use rand::Rng;
use serde::Deserialize;
use std::error::Error;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Instant;

const MB: usize = 1024 * 1024;

#[derive(Clone, Deserialize)]
pub struct Server {
    pub lat: String,
    pub lon: String,
    pub distance: i32,
    pub name: String,
    pub country: String,
    pub cc: String,
    pub sponsor: String,
    pub id: String,
    pub host: String,
    #[serde(skip)]
    pub latency: f64,
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "[id: {:5}] {:4}Km [{}, {}]\t{}",
            self.id, self.distance, self.name, self.cc, self.sponsor
        )
    }
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "[id: {:5}] {:4}Km(lat: {}°, lon: {}°) {}, {}\n{}: {}\n",
            self.id,
            self.distance,
            self.lat,
            self.lon,
            self.name,
            self.country,
            self.sponsor,
            self.host
        )
    }
}

pub fn upload(host: &str, bytes: usize) -> Result<f64, Box<dyn Error>> {
    info!("Upload {} MB", bytes as f64 / MB as f64);
    info!("connect to server: {}", host);
    let mut stream = TcpStream::connect(host)?;
    let ulstring = format!("UPLOAD {} 0\r\n", bytes);
    info!("send upload message: {:?}", ulstring);
    stream.write_all(ulstring.as_bytes())?;
    info!("generating random bytes");
    let mut randstring: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(bytes - ulstring.len())
        .collect();
    randstring.push('\n');
    info!("uploading...");
    let mut line = String::new();
    let now = Instant::now();
    stream.write_all(randstring.as_bytes())?;
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line)?;
    let elapsed = now.elapsed().as_micros();
    info!("Server response: {:?}", line);
    info!("Upload took {} ms", elapsed as f64 / 1000.0);
    Ok(bytes as f64 / elapsed as f64 * 8.0)
}

pub fn download(host: &str, bytes: usize) -> Result<f64, Box<dyn Error>> {
    info!("Download {} MB", bytes as f64 / MB as f64);
    info!("connect to server: {}", host);
    let mut stream = TcpStream::connect(host)?;
    let dlstring = format!("DOWNLOAD {}\r\n", bytes);
    info!("send download message: {:?}", dlstring);
    stream.write_all(dlstring.as_bytes())?;
    let mut reader = BufReader::with_capacity(MB, stream);
    info!("downloading...");
    let mut len = 0;
    let now = Instant::now();
    let mut old = now;
    let mut old_len = 0;
    let step = bytes / 32;
    loop {
        let buffer = reader.fill_buf()?;
        let length = buffer.len();
        len += length;
        let len_since_last_measure = len - old_len;
        if len_since_last_measure > step {
            let time = old.elapsed().as_micros();
            info!(
                "buffer length: {} KB, time: {} ms, speed: {} Mbps",
                len_since_last_measure as f64 / 1024.0,
                time as f64 / 1000.0,
                len_since_last_measure as f64 / time as f64 * 8.0
            );
            old = Instant::now();
            old_len = len;
        }
        if length == 0 || buffer.last() == Some(&b'\n') {
            break;
        }
        reader.consume(length);
    }
    let elapsed = now.elapsed().as_micros();
    info!("Download took {} ms", elapsed as f64 / 1000.0);
    info!("Download size: {} MB", len as f64 / MB as f64);
    Ok(len as f64 / elapsed as f64 * 8.0)
}

pub fn ping_server(host: &str) -> Result<f64, Box<dyn Error>> {
    info!("Ping Test");
    info!("connect to server: {}", host);
    let mut line = String::new();
    let mut stream = TcpStream::connect(host)?;
    info!("Send \"HI\" to server");
    let now = Instant::now();
    stream.write_all(b"HI\r\n")?;
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line)?;
    let elapsed = now.elapsed().as_micros();
    Ok(elapsed as f64 / 1000.0)
}

pub fn list_servers() -> Result<Vec<Server>, Box<dyn Error>> {
    Ok(reqwest::get("https://speedtest.net/api/js/servers?engine=js")?.json()?)
}

pub fn best_server() -> Result<Server, Box<dyn Error>> {
    info!("Finding best server...");
    let mut servers = list_servers()?;
    servers.sort_by_key(|s| s.distance);
    servers.truncate(3);
    servers.iter_mut().for_each(|s| {
        info!("ping {}", s.sponsor);
        s.latency = ping_server(&s.host).unwrap();
        info!("{} ping result: {}ms", s.sponsor, s.latency);
    });
    servers.sort_by(|a, b| a.latency.partial_cmp(&b.latency).unwrap());
    let best = servers[0].clone();
    info!("Select server {}", best.sponsor);
    Ok(best)
}
