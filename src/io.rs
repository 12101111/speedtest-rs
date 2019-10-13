use failure::Error;
use log::info;
use rand::{Rng, SeedableRng};
use rand_xoshiro::Xoroshiro128PlusPlus;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

pub const MB: usize = 1024 * 1024;
pub const MEASURE: usize = 32;

pub fn upload(mut stream: TcpStream, bytes: usize, len: Arc<AtomicUsize>) -> Result<f64, Error> {
    let msg = format!("UPLOAD {} 0\r\n", bytes);
    stream.write_all(msg.as_bytes())?;
    len.fetch_add(msg.len(), Ordering::AcqRel);
    let (tx, rx) = mpsc::sync_channel(16);
    thread::spawn(move || {
        let mut left = bytes - msg.len();
        while left > 0 {
            let length = MB.min(left);
            let mut buf: Vec<u8> = Xoroshiro128PlusPlus::from_entropy()
                .sample_iter(&rand::distributions::Alphanumeric)
                .map(|x| x as u8)
                .take(length)
                .collect();
            if left < MB {
                buf.push(b'\n');
            };
            tx.send(buf).unwrap();
            left -= length;
        }
    });
    let mut line = String::new();
    let now = Instant::now();
    loop {
        let buffer = rx.recv()?;
        stream.write_all(&buffer)?;
        let length = buffer.len();
        len.fetch_add(length, Ordering::AcqRel);
        if buffer.last() == Some(&b'\n') {
            break;
        }
    }
    let time = now.elapsed().as_micros();
    info!("Upload took {:?} seconds", time as f64 / 1000000.0);
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line)?;
    info!("server response: {:?}", line);
    if !line.contains(&format!("{}", bytes)) {
        Err(failure::format_err!(
            "Upload was interrupted,upload {} bytes but server response: {:?}",
            bytes,
            line
        ))
    } else {
        Ok(bytes as f64 / time as f64 * 8.0)
    }
}

fn download(mut stream: TcpStream, bytes: usize, len: Arc<AtomicUsize>) -> Result<f64, Error> {
    stream.write_all(format!("DOWNLOAD {}\r\n", bytes).as_bytes())?;
    let mut reader = BufReader::with_capacity(MB, stream);
    let now = Instant::now();
    loop {
        let buffer = reader.fill_buf()?;
        let length = buffer.len();
        len.fetch_add(length, Ordering::AcqRel);
        if length == 0 || buffer.last() == Some(&b'\n') {
            break;
        }
        reader.consume(length);
    }
    let time = now.elapsed().as_micros();
    info!("Download took {:?} seconds", time as f64 / 1000000.0);
    if len.load(Ordering::Acquire) != bytes {
        Err(failure::format_err!("Download was interrupted"))
    } else {
        Ok(bytes as f64 / time as f64 * 8.0)
    }
}

fn measure(bytes: usize, len: Arc<AtomicUsize>) {
    let step = bytes / MEASURE;
    let mut old_len = 0;
    let mut old_time = Instant::now();
    loop {
        let new_len = len.load(Ordering::Acquire);
        let delta = new_len - old_len;
        let elapsed = old_time.elapsed();
        let time = elapsed.as_micros();
        if delta > step {
            info!("Speed now: {} Mbps", delta as f64 / time as f64 * 8.0);
            old_len = new_len;
            old_time = Instant::now();
        }
        if new_len >= bytes || time > 20_000_000 {
            break;
        }
        thread::sleep((elapsed / 4).min(Duration::from_secs(1)));
    }
}

pub fn upload_st(stream: TcpStream, bytes: usize) -> Result<f64, Error> {
    let len = Arc::new(AtomicUsize::new(0));
    let len1 = len.clone();
    let handle = thread::spawn(move || upload(stream, bytes, len1));
    measure(bytes, len);
    Ok(handle.join().unwrap()?)
}

pub fn upload_mt(host: String, bytes: usize, thread: usize) -> Result<f64, Error> {
    let bytes = bytes / thread * thread;
    let len = Arc::new(AtomicUsize::new(0));
    let now = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..thread {
        let lent = len.clone();
        let connection = TcpStream::connect(&host)?;
        let handle = thread::spawn(move || upload(connection, bytes / thread, lent));
        handles.push(handle);
    }
    measure(bytes, len);
    for h in handles {
        h.join().unwrap()?;
    }
    let time = now.elapsed().as_micros();
    Ok(bytes as f64 / time as f64 * 8.0)
}

pub fn download_st(stream: TcpStream, bytes: usize) -> Result<f64, Error> {
    let len = Arc::new(AtomicUsize::new(0));
    let len1 = len.clone();
    let handle = thread::spawn(move || download(stream, bytes, len1));
    measure(bytes, len);
    Ok(handle.join().unwrap()?)
}

pub fn download_mt(host: String, bytes: usize, thread: usize) -> Result<f64, Error> {
    let bytes = bytes / thread * thread;
    let len = Arc::new(AtomicUsize::new(0));
    let now = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..thread {
        let lent = len.clone();
        let connection = TcpStream::connect(&host)?;
        let handle = thread::spawn(move || download(connection, bytes / thread, lent));
        handles.push(handle);
    }
    measure(bytes, len);
    for h in handles {
        h.join().unwrap()?;
    }
    let time = now.elapsed().as_micros();
    Ok(bytes as f64 / time as f64 * 8.0)
}

pub fn ping(stream: &mut TcpStream) -> Result<f64, Error> {
    info!("Ping Test");
    let mut line = String::new();
    info!("Send \"PING \" to server");
    let now = Instant::now();
    stream.write_all(b"PING \r\n")?;
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line)?;
    let elapsed = now.elapsed().as_micros();
    info!("Server response: {:?}", line);
    Ok(elapsed as f64 / 1000.0)
}

pub fn test(stream: &mut TcpStream) -> Result<(), Error> {
    info!("Test connection");
    let mut line = String::new();
    info!("Send \"HI\" to server");
    stream.write_all(b"HI\r\n")?;
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line)?;
    info!("Server response: {:?}", line);
    Ok(())
}

pub fn connect(host: &str) -> Result<TcpStream, Error> {
    info!("connect to server: {}", host);
    let mut stream = TcpStream::connect(host)?;
    test(&mut stream)?;
    Ok(stream)
}
