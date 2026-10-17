//! The launcher logo: a looping animation of Tux at the tavern counter.
//!
//! The animation plays through GTK's media backend (GStreamer). The space
//! stays empty until it is ready, so the window does not switch pictures on
//! start; when it cannot be played, the original drawing of the logo is
//! shown instead. It pauses while the window is minimised or the game runs,
//! so it costs nothing when nobody looks at it.

use super::components::embedded_image;
use relm4::gtk::{self, gdk, glib, prelude::*};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

const ANIMATION: &[u8] = include_bytes!("../../data/logo/logo-loop.webm");
/// The drawn logo, shown when the animation cannot be played.
const ORIGINAL: &[u8] = include_bytes!("../../data/icons/logo.png");
/// Logo size in the window; both pictures have twice this size for HiDPI.
const SIZE: i32 = 184;

pub struct Logo {
    pub image: gtk::Image,
    /// Holds the media file for the life of the window; without this
    /// reference it would be dropped before it starts playing.
    animation: Option<Rc<Animation>>,
}

struct Animation {
    media: gtk::MediaFile,
    minimized: Cell<bool>,
    game_running: Cell<bool>,
}

impl Animation {
    fn apply(&self) {
        if self.minimized.get() || self.game_running.get() {
            self.media.pause();
        } else {
            self.media.play();
        }
    }
}

impl Logo {
    pub fn new(cache_dir: &Path) -> Self {
        let image = gtk::Image::new();
        image.set_pixel_size(SIZE);
        // Keeps the layout steady while the animation loads.
        image.set_size_request(SIZE, SIZE);
        let animation = match write_animation(cache_dir) {
            Ok(path) => Some(animate(&image, &path)),
            Err(error) => {
                tracing::warn!(%error, "logo animation unavailable; showing the drawn logo");
                show_original(&image);
                None
            }
        };
        Self { image, animation }
    }

    pub fn set_game_running(&self, running: bool) {
        if let Some(animation) = &self.animation {
            if animation.game_running.replace(running) != running {
                animation.apply();
            }
        }
    }
}

fn show_original(image: &gtk::Image) {
    image.set_paintable(embedded_image(ORIGINAL).paintable().as_ref());
}

fn animate(image: &gtk::Image, path: &Path) -> Rc<Animation> {
    let media = gtk::MediaFile::for_filename(path);
    media.set_loop(true);
    media.set_muted(true);
    media.connect_prepared_notify(glib::clone!(
        #[weak]
        image,
        move |media| {
            if media.is_prepared() && media.error().is_none() {
                image.set_paintable(Some(media));
            }
        }
    ));
    media.connect_error_notify(glib::clone!(
        #[weak]
        image,
        move |media| {
            if let Some(error) = media.error() {
                tracing::warn!(%error, "logo animation cannot be played; showing the drawn logo");
                show_original(&image);
            }
        }
    ));
    // A backend that fails at once (or no backend at all) reports the error
    // before the handlers above are connected.
    if let Some(error) = media.error() {
        tracing::warn!(%error, "logo animation cannot be played; showing the drawn logo");
        show_original(image);
    } else if media.is_prepared() {
        image.set_paintable(Some(&media));
    }
    let animation = Rc::new(Animation {
        media,
        minimized: Cell::new(false),
        game_running: Cell::new(false),
    });

    // Follow the window's minimised state once the window exists.
    let watching = Cell::new(false);
    image.connect_map(glib::clone!(
        #[weak]
        animation,
        move |image| {
            if watching.replace(true) {
                return;
            }
            let Some(toplevel) = image
                .native()
                .and_then(|native| native.surface())
                .and_downcast::<gdk::Toplevel>()
            else {
                return;
            };
            toplevel.connect_state_notify(glib::clone!(
                #[weak]
                animation,
                move |toplevel| {
                    let minimized = toplevel.state().contains(gdk::ToplevelState::MINIMIZED);
                    if animation.minimized.replace(minimized) != minimized {
                        animation.apply();
                    }
                }
            ));
        }
    ));

    animation.apply();
    animation
}

/// GTK's media backend plays files, not memory, so the embedded animation is
/// written to the cache directory once per version of the file.
fn write_animation(cache_dir: &Path) -> std::io::Result<PathBuf> {
    let path = cache_dir.join(format!("logo-loop-{:x}.webm", fingerprint(ANIMATION)));
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() == ANIMATION.len() as u64) {
        return Ok(path);
    }
    std::fs::create_dir_all(cache_dir)?;
    let partial = path.with_extension("webm.part");
    std::fs::write(&partial, ANIMATION)?;
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

/// FNV-1a; only tells versions of the embedded file apart.
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
