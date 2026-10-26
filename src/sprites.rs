//! Rendered frames into Doom sprites: named, offset with grAb chunks and cropped with putpng.
//!
//! A weapon's sprites are named `<code><page>`: its two-character code and two characters
//! numbering a page of 26 frames, the frame being the letter after it. Every render (fire,
//! reload...) starts a page of its own. Frames with nothing in them are left out and drawn as
//! TNT1.

use anyhow::{Context, Result, anyhow, ensure};
use image::{GenericImageView, RgbaImage, imageops};
use putpng::crc::Crc32;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// One frame of a sequence: its sprite and frame letter, or nothing to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// `area` is how many pixels show.
    Sprite {
        name: String,
        letter: char,
        area: usize,
    },
    Empty,
}

impl Frame {
    /// How many pixels show (0 for nothing).
    pub fn area(&self) -> usize {
        match self {
            Self::Sprite { area, .. } => *area,
            Self::Empty => 0,
        }
    }

    /// As a state writes it: `THF0 A`, or `TNT1 A`.
    pub fn state(&self) -> (String, char) {
        match self {
            Self::Sprite { name, letter, .. } => (name.clone(), *letter),
            Self::Empty => ("TNT1".into(), 'A'),
        }
    }
}

/// A weapon's sprite names are its code and a page of 26 frames, in two characters: the first
/// from `ROWS`, the second from `PAGES`, so 1296 pages in all. Every render starts a page.
const ROWS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const PAGES: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// The canvas's top left on Doom's 320x200 weapon screen (see weapon.blend): 80 pixels left of
/// it, and the weapon's resting height (WEAPONTOP) above.
const WEAPON_OFFSET: (i32, i32) = (80, 32);
/// Specks smaller than this (in pixels, their border included) are dropped: thin parts like the
/// Wunderwaffe's coil wires break up into them at sprite size.
const SPECK: usize = 10;
/// Wall drawings are scaled down by this, every one alike, so each keeps the size World at War
/// gives it (a grenade bag's 256 pixels are half a rifle's 512), and drawn at half size.
const CHALK_SHRINK: u32 = 4;

pub struct Sprites {
    crc: Crc32,
    folder: PathBuf,
    code: String,
    /// The pages used so far.
    pages: usize,
}

fn putpng(result: std::result::Result<(), Box<dyn std::error::Error>>, path: &Path) -> Result<()> {
    result.map_err(|e| anyhow!("putpng failed on {}: {e}", path.display()))
}

/// Clears islands of fewer than `SPECK` visible pixels (8-connected) that aren't part of the rest.
fn despeckle(image: &mut RgbaImage) {
    let (width, height) = image.dimensions();
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut stack = vec![];
    for start in 0..w * h {
        if seen[start] || image.as_raw()[start * 4 + 3] == 0 {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut island = vec![];
        while let Some(at) = stack.pop() {
            island.push(at);
            let (x, y) = ((at % w) as i64, (at / w) as i64);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let next = ny as usize * w + nx as usize;
                    if !seen[next] && image.as_raw()[next * 4 + 3] != 0 {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        if island.len() < SPECK {
            for at in island {
                image.put_pixel((at % w) as u32, (at / w) as u32, image::Rgba([0, 0, 0, 0]));
            }
        }
    }
}

impl Sprites {
    /// Sprites for the weapon with sprite `code`, written into `folder`.
    pub fn new(code: &str, folder: PathBuf) -> Result<Self> {
        fs::create_dir_all(&folder)?;
        Ok(Self {
            crc: Crc32::new(),
            folder,
            code: code.to_uppercase(),
            pages: 0,
        })
    }

    /// The sprite name of the next page.
    fn next_page(&mut self) -> Result<String> {
        let row = *ROWS
            .get(self.pages / PAGES.len())
            .context("A weapon has more than 1296 pages of 26 frames; give it fewer variants")?;
        let page = PAGES[self.pages % PAGES.len()];
        self.pages += 1;
        Ok(format!("{}{}{}", self.code, row as char, page as char))
    }

    /// Writes `image` as the sprite `name` frame `letter`, offset by `offset` before cropping.
    fn write(&self, image: &RgbaImage, name: &str, letter: char, offset: (i32, i32)) -> Result<()> {
        let path = self
            .folder
            .join(format!("{name}{letter}0.png").to_lowercase());
        // Re-encoding drops Blender's extra chunks; putpng then adds the offset and crops.
        image
            .save(&path)
            .with_context(|| format!("Cannot write {}", path.display()))?;
        putpng(
            putpng::grab::grab(&path, &self.crc, offset.0, offset.1),
            &path,
        )?;
        putpng(putpng::crop::crop(&path, &self.crc), &path)?;
        self.compact(&path)
    }

    /// Re-encodes a cropped sprite as Doom's are, 8-bit: up to 255 colours of its own and one
    /// transparent, at the best compression, keeping putpng's offset.
    fn compact(&self, path: &Path) -> Result<()> {
        let grab = putpng::grab::read_grab(path)
            .map_err(|e| anyhow!("Cannot read the offset of {}: {e}", path.display()))?
            .unwrap_or_default();
        let image = image::open(path)?.to_rgba8();
        let (width, height) = image.dimensions();
        let opaque: Vec<u8> = image
            .pixels()
            .filter(|p| p.0[3] != 0)
            .flat_map(|p| [p.0[0], p.0[1], p.0[2], 255])
            .collect();
        // Index 0 is transparent; the colours come after it.
        let quantizer = color_quant::NeuQuant::new(10, 255, &opaque);
        let mut palette = vec![0u8, 0, 0];
        palette.extend(quantizer.color_map_rgb());
        let indices: Vec<u8> = image
            .pixels()
            .map(|p| {
                if p.0[3] == 0 {
                    0
                } else {
                    quantizer.index_of(&[p.0[0], p.0[1], p.0[2], 255]) as u8 + 1
                }
            })
            .collect();
        let mut bytes = vec![];
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(palette);
            encoder.set_trns(vec![0u8]);
            encoder.set_compression(png::Compression::Best);
            encoder.set_filter(png::FilterType::NoFilter);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&indices)?;
        }
        fs::write(path, bytes)?;
        putpng(
            putpng::grab::push_grab(path, &self.crc, grab.0, grab.1),
            path,
        )
    }

    /// The frames of one render (`folder` holds `0000.png`...; `count` of them) as a sequence.
    pub fn sequence(&mut self, folder: &Path, count: usize) -> Result<Vec<Frame>> {
        let mut names = vec![];
        let mut frames = vec![];
        for index in 0..count {
            let path = folder.join(format!("{index:04}.png"));
            let mut image = image::open(&path)
                .with_context(|| format!("Cannot read the render {}", path.display()))?
                .to_rgba8();
            despeckle(&mut image);
            if image.pixels().all(|p| p.0[3] == 0) {
                frames.push(Frame::Empty);
                continue;
            }
            while names.len() <= index / 26 {
                names.push(self.next_page()?);
            }
            let name = names[index / 26].clone();
            let letter = (b'A' + (index % 26) as u8) as char;
            self.write(&image, &name, letter, WEAPON_OFFSET)?;
            let area = image.pixels().filter(|p| p.0[3] != 0).count();
            frames.push(Frame::Sprite { name, letter, area });
        }
        Ok(frames)
    }

    /// A picture standing on the ground (the pickup), its feet at the middle of its bottom.
    pub fn standing(&mut self, path: &Path) -> Result<Frame> {
        let image = image::open(path)
            .with_context(|| format!("Cannot read {}", path.display()))?
            .to_rgba8();
        ensure!(
            image.pixels().any(|p| p.0[3] != 0),
            "{} is empty",
            path.display()
        );
        let name = self.next_page()?;
        let (width, height) = image.dimensions();
        self.write(&image, &name, 'A', (width as i32 / 2, height as i32))?;
        let area = (width * height) as usize;
        Ok(Frame::Sprite {
            name,
            letter: 'A',
            area,
        })
    }

    /// The wall drawing: the game's chalk image scaled down, centred.
    pub fn chalk(&mut self, path: &Path) -> Result<Frame> {
        let source =
            image::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
        let (width, height) = source.dimensions();
        let scaled = imageops::resize(
            &source.to_rgba8(),
            (width / CHALK_SHRINK).max(1),
            (height / CHALK_SHRINK).max(1),
            imageops::FilterType::Lanczos3,
        );
        let name = self.next_page()?;
        let (width, height) = scaled.dimensions();
        self.write(&scaled, &name, 'A', (width as i32 / 2, height as i32 / 2))?;
        let area = (width * height) as usize;
        Ok(Frame::Sprite {
            name,
            letter: 'A',
            area,
        })
    }
}

/// The sprite names already in the pk3 folders `folders` (four characters, upper case).
pub fn existing(folders: &[PathBuf]) -> Result<Vec<String>> {
    let mut names = vec![];
    for folder in folders {
        let sprites = folder.join("sprites");
        if !sprites.exists() {
            continue;
        }
        let mut stack = vec![sprites];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    && stem.len() >= 4
                {
                    names.push(stem[..4].to_uppercase());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specks_go_and_the_weapon_stays() {
        let mut image = RgbaImage::new(20, 10);
        for x in 0..12 {
            for y in 4..8 {
                image.put_pixel(x, y, image::Rgba([9, 9, 9, 255]));
            }
        }
        image.put_pixel(17, 1, image::Rgba([9, 9, 9, 255]));
        image.put_pixel(18, 1, image::Rgba([9, 9, 9, 255]));
        despeckle(&mut image);
        assert_eq!(image.get_pixel(17, 1).0[3], 0);
        assert_eq!(image.get_pixel(5, 5).0[3], 255);
    }

    #[test]
    fn pages_go_past_36_renders() {
        let folder = std::env::temp_dir().join(format!("doomify-pages-{}", std::process::id()));
        let mut sprites = Sprites::new("ww", folder.clone()).unwrap();
        let names: Vec<_> = (0..40).map(|_| sprites.next_page().unwrap()).collect();
        assert_eq!(names[0], "WWA0");
        assert_eq!(names[35], "WWAZ");
        assert_eq!(names[36], "WWB0");
        sprites.pages = ROWS.len() * PAGES.len();
        assert!(sprites.next_page().is_err());
        fs::remove_dir_all(folder).ok();
    }
}
