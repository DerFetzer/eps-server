use crate::{config::Config, error::AppError};
use eyre::{eyre, Context};
use std::{
    fmt::Display,
    fs::{read_dir, remove_file},
    io::Write,
    path::Path,
    str::FromStr,
};
use tokio::{fs::File, io::AsyncWriteExt, task};
use tokio_util::io::ReaderStream;

const MAC_LEN: usize = 8;
const SVG_EXT: &str = ".svg";
const BMP_EXT: &str = ".bmp";
const PNG_EXT: &str = ".png";

pub(crate) struct ImageHandler {
    config: Config,
    svg_opts: usvg::Options,
}

impl ImageHandler {
    pub fn new(config: Config) -> Self {
        let mut svg_opts = usvg::Options::default();
        svg_opts.fontdb.load_system_fonts();

        ImageHandler { config, svg_opts }
    }

    pub async fn get_macs(&self) -> Result<Vec<EpdMac>, AppError> {
        let image_dir = self.config.image_dir.clone();

        task::spawn_blocking::<_, Result<Vec<EpdMac>, eyre::Error>>(move || {
            read_dir(image_dir)?
                .flatten()
                .filter_map(|f| {
                    let path = f.path();
                    match path.extension() {
                        Some(ext) if ext.to_str()? == "png" => {
                            Some(f.path().file_stem()?.to_str()?.parse::<EpdMac>())
                        }
                        _ => None,
                    }
                })
                .collect()
        })
        .await
        .map_err(|e| AppError::InternalServerError(e.into()))?
        .map_err(AppError::InternalServerError)
    }

    pub async fn get_svg(&self, mac: EpdMac) -> Result<ReaderStream<File>, AppError> {
        let image_dir = self.config.image_dir.clone();

        let svg_path = image_dir.join(mac.to_string().to_lowercase() + SVG_EXT);
        self.get_file(svg_path).await
    }

    pub async fn get_png(&self, mac: EpdMac) -> Result<ReaderStream<File>, AppError> {
        let image_dir = self.config.image_dir.clone();

        let png_path = image_dir.join(mac.to_string().to_lowercase() + PNG_EXT);
        self.get_file(png_path).await
    }

    async fn get_file(&self, path: impl AsRef<Path>) -> Result<ReaderStream<File>, AppError> {
        let file = File::open(path)
            .await
            .map_err(|e| AppError::NotFound(e.into()))?;
        Ok(ReaderStream::new(file))
    }

    pub async fn delete_images(&self, mac: EpdMac) -> Result<(), AppError> {
        let image_dir = self.config.image_dir.clone();

        let png_path = image_dir.join(mac.to_string().to_lowercase() + PNG_EXT);
        let bmp_path = image_dir.join(mac.to_string().to_lowercase() + BMP_EXT);
        let svg_path = image_dir.join(mac.to_string().to_lowercase() + SVG_EXT);

        task::spawn_blocking(move || {
            match (
                remove_file(svg_path),
                remove_file(bmp_path),
                remove_file(png_path),
            ) {
                (Err(_), Err(_), Err(_)) => Err(AppError::NotFound(eyre!(
                    "Could not find any images for MAC {}.",
                    mac
                ))),
                _ => Ok(()),
            }
        })
        .await
        .map_err(|e| AppError::InternalServerError(e.into()))?
    }

    pub async fn post_svg_body(&self, mac: EpdMac, svg_body: &str) -> Result<(), AppError> {
        let image_dir = self.config.image_dir.clone();

        let svg_path = image_dir.join(mac.to_string().to_lowercase() + SVG_EXT);
        let png_path = image_dir.join(mac.to_string().to_lowercase() + PNG_EXT);

        let mut file = File::create(svg_path)
            .await
            .map_err(|e| AppError::InternalServerError(e.into()))?;

        let mut buf = vec![];
        write!(
            buf,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\">",
            self.config.epd_width, self.config.epd_height
        )
        .map_err(|e| AppError::InternalServerError(e.into()))?;
        buf.extend_from_slice(svg_body.as_bytes());
        write!(buf, "</svg>").map_err(|e| AppError::InternalServerError(e.into()))?;

        // https://docs.rs/tokio/latest/tokio/fn.spawn.html#using-send-values-from-a-task
        // Could not get to work with `spawn_blocking`
        {
            let rtree = usvg::Tree::from_data(&buf, &self.svg_opts.to_ref())
                .map_err(|e| AppError::BadRequest(e.into()))?;

            let pixmap_size = rtree.svg_node().size.to_screen_size();
            let mut pixmap =
                tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
            resvg::render(
                &rtree,
                usvg::FitTo::Original,
                tiny_skia::Transform::default(),
                pixmap.as_mut(),
            )
            .ok_or_else(|| AppError::InternalServerError(eyre!("Could not render svg!")))?;

            pixmap
                .save_png(png_path)
                .map_err(|e| AppError::InternalServerError(e.into()))?;
        }

        file.write_all(&buf)
            .await
            .map_err(|e| AppError::InternalServerError(e.into()))
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
