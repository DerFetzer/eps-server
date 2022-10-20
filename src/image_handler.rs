use crate::config::Config;
use eyre::{eyre, Context};
use std::{
    fmt::Display,
    fs::{read_dir, remove_file},
    str::FromStr,
};
use tokio::task;

const MAC_LEN: usize = 8;

pub(crate) struct ImageHandler {
    config: Config,
}

impl ImageHandler {
    pub fn new(config: Config) -> Self {
        ImageHandler { config }
    }

    pub async fn get_macs(&self) -> Result<Vec<EpdMac>, eyre::Error> {
        let image_dir = self.config.image_dir.clone();

        task::spawn_blocking(move || {
            read_dir(image_dir)?
                .flatten()
                .filter_map(|f| {
                    let path = f.path();
                    match path.extension() {
                        Some(ext) if ext.to_str()? == "png" => {
                            Some(f.path().file_stem()?.to_str()?.parse())
                        }
                        _ => None,
                    }
                })
                .collect()
        })
        .await?
    }

    pub async fn delete_images(&self, mac: EpdMac) -> Result<(), eyre::Error> {
        let image_dir = self.config.image_dir.clone();

        let png_path = image_dir.join(mac.to_string().to_lowercase() + ".png");
        let bmp_path = image_dir.join(mac.to_string().to_lowercase() + ".bmp");
        let svg_path = image_dir.join(mac.to_string().to_lowercase() + ".svg");

        task::spawn_blocking(move || {
            match (
                remove_file(svg_path),
                remove_file(bmp_path),
                remove_file(png_path),
            ) {
                (Err(_), Err(_), Err(_)) => {
                    Err(eyre!("Could not find any images for MAC {}.", mac))
                }
                _ => Ok(()),
            }
        })
        .await?
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EpdMac(pub [u8; MAC_LEN]);

impl FromStr for EpdMac {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != MAC_LEN * 2 {
            return Err(eyre::eyre!("Mac must be {} bytes long!", MAC_LEN));
        }
        let bytes: Vec<_> = (0..s.len())
            .step_by(2)
            .filter_map(|i| {
                s.get(i..i + 2)
                    .and_then(|sub| u8::from_str_radix(sub, 16).ok())
            })
            .collect();
        Ok(Self(
            bytes
                .as_slice()
                .try_into()
                .wrap_err("Could not parse MAC from {s}")?,
        ))
    }
}

impl Display for EpdMac {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.0.iter() {
            write!(f, "{b:02X}")?
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_from_str() {
        let exp = EpdMac([0xaa, 0xbb, 0xcc, 0xdd, 0x00, 0x11, 0x22, 0x33]);
        let m1: EpdMac = "aabbccdd00112233".parse().unwrap();
        assert_eq!(m1, exp);
    }

    #[test]
    fn mac_from_str_invalid() {
        assert!("00112233445566".parse::<EpdMac>().is_err());
        assert!("001122334455667z".parse::<EpdMac>().is_err());
    }

    #[test]
    fn mac_display() {
        let mac = EpdMac([0xaa, 0xbb, 0xcc, 0xdd, 0x00, 0x11, 0x22, 0x33]);
        assert_eq!(format!("{mac}"), "AABBCCDD00112233".to_string());
    }
}
