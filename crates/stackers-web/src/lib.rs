//! wasm-bindgen API used by `web/`: the disc/TIM viewer (milestone 1) and the game session (milestone 2).

mod front;
mod music;
mod screen;
mod session;
mod sfx;
mod sprites;
mod set_match;
mod trial;
pub use front::FrontEnd;
pub use music::Music;
pub use screen::{Screen, SCREEN_STRIDE};
pub use session::Session;
pub use sfx::{Sfx, SfxSample};
pub use sprites::{decode_tim, decode_tim_semi, Sprites};
pub use set_match::Match;
pub use trial::Trial;

use stackers_formats::{iso::Disc, tim::Tim};
use wasm_bindgen::prelude::*;

/// Holds the raw Track 01 image supplied by the user's file picker.
#[wasm_bindgen]
pub struct GameDisc {
    raw: Vec<u8>,
    paths: Vec<String>,
}

#[wasm_bindgen]
pub struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    clut_count: u32,
    origin: (u32, u32),
}

impl Image {
    pub(crate) fn new(tim: &Tim, clut: u32) -> Image {
        Image::semi(tim, clut, false)
    }

    pub(crate) fn semi(tim: &Tim, clut: u32, semi: bool) -> Image {
        Image {
            width: tim.width,
            height: tim.height,
            rgba: tim.to_rgba_semi(clut as usize, semi),
            clut_count: tim.cluts.len() as u32,
            origin: tim.page_origin(),
        }
    }
}

#[wasm_bindgen]
impl Image {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Texture-page u of the image's first column: subtract it from a sprite's u.
    #[wasm_bindgen(getter)]
    pub fn u0(&self) -> u32 {
        self.origin.0
    }
    /// Texture-page v of the image's first row.
    #[wasm_bindgen(getter)]
    pub fn v0(&self) -> u32 {
        self.origin.1
    }
    #[wasm_bindgen(getter)]
    pub fn clut_count(&self) -> u32 {
        self.clut_count
    }
    /// RGBA8 pixels, ready for `ImageData`.
    pub fn rgba(&self) -> Vec<u8> {
        self.rgba.clone()
    }
}

pub(crate) fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
impl GameDisc {
    /// Parse the Track 01 `.bin` (MODE2/2352). Fails if it isn't the Tecmo Stackers data track.
    #[wasm_bindgen(constructor)]
    pub fn new(raw: Vec<u8>) -> Result<GameDisc, JsError> {
        let paths = {
            let disc = Disc::open(&raw).map_err(js_err)?;
            if disc.find("SLUS_003.15").is_none() {
                return Err(JsError::new("This doesn't look like Tecmo Stackers (USA): SLUS_003.15 missing"));
            }
            disc.entries().iter().filter(|e| !e.is_cdda(&disc)).map(|e| e.path.clone()).collect()
        };
        Ok(GameDisc { raw, paths })
    }

    /// All data-file paths on the disc.
    pub fn files(&self) -> Vec<String> {
        self.paths.clone()
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, JsError> {
        Disc::open(&self.raw).map_err(js_err)?.read_path(path).map_err(js_err)
    }

    /// Decode a TIM file to RGBA using palette row `clut`.
    pub fn tim(&self, path: &str, clut: u32) -> Result<Image, JsError> {
        let tim = Tim::parse(&self.read(path)?).map_err(js_err)?;
        Ok(Image::new(&tim, clut))
    }
}
