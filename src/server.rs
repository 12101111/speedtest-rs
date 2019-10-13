use crate::io::ping;
use failure::Error;
use log::info;
use serde::Deserialize;
use std::fmt;
use std::net::TcpStream;

#[derive(Clone, Deserialize)]
pub struct Server {
    pub lat: String,
    pub lon: String,
    pub distance: Option<i32>,
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
        if let Some(dis) = self.distance {
            write!(
                f,
                "[id: {:5}] {:4}Km [{}, {}]\t{}",
                self.id, dis, self.name, self.cc, self.sponsor
            )
        } else {
            write!(
                f,
                "[id: {:5}] [{}, {}]\t{}",
                self.id, self.name, self.cc, self.sponsor
            )
        }
    }
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(dis) = self.distance {
            write!(
                f,
                "[id: {:5}] {:4}Km(lat: {}°, lon: {}°) {}, {}, {}: {}",
                self.id, dis, self.lat, self.lon, self.name, self.country, self.sponsor, self.host
            )
        } else {
            write!(
                f,
                "[id: {:5}] (lat: {}°, lon: {}°) {}, {}, {}: {}",
                self.id, self.lat, self.lon, self.name, self.country, self.sponsor, self.host
            )
        }
    }
}

impl Server {
    pub fn near() -> Result<Vec<Server>, Error> {
        Ok(reqwest::get("https://speedtest.net/api/js/servers?engine=js")?.json()?)
    }

    pub fn all() -> Result<Vec<Server>, Error> {
        use roxmltree::*;
        info!("Get all servers from https://www.speedtest.net/speedtest-servers-static.php");
        let xml = reqwest::get("https://www.speedtest.net/speedtest-servers-static.php")?.text()?;
        let doc = Document::parse(&xml)?;
        Ok(doc
            .descendants()
            .filter(|n| n.has_tag_name("server"))
            .map(|n| Server {
                lat: n.attribute("lat").unwrap_or_default().to_string(),
                lon: n.attribute("lon").unwrap_or_default().to_string(),
                distance: None,
                name: n.attribute("name").unwrap_or_default().to_string(),
                country: n.attribute("country").unwrap_or_default().to_string(),
                cc: n.attribute("cc").unwrap_or_default().to_string(),
                sponsor: n.attribute("sponsor").unwrap_or_default().to_string(),
                id: n.attribute("id").unwrap_or_default().to_string(),
                host: n.attribute("host").unwrap_or_default().to_string(),
                latency: 0.0,
            })
            .collect())
    }

    pub fn best() -> Result<Server, Error> {
        info!("Finding best server...");
        let mut servers = Self::near()?;
        servers.sort_by_key(|s| s.distance);
        servers.truncate(4);
        servers.iter_mut().for_each(|s| {
            info!("ping {}", s.sponsor);
            s.latency = match TcpStream::connect(&s.host) {
                Ok(mut stream) => ping(&mut stream).unwrap(),
                Err(_) => std::f64::MAX,
            };
            info!("{} ping result: {}ms", s.sponsor, s.latency);
        });
        servers.sort_by(|a, b| a.latency.partial_cmp(&b.latency).unwrap());
        let best = servers[0].clone();
        info!("Select server {} as best", best.sponsor);
        Ok(best)
    }
}
