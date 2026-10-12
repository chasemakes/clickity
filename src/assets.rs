//! Assets compiled into the binary: Clickity's icons and fonts, layered over GPUI Kit's icon set.

use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

macro_rules! embed {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("clickity/", $name), include_bytes!(concat!("../assets/icons/", $name)))),*]
    };
}

const ICONS: &[(&str, &[u8])] = embed![
    "clickity-mark.svg",
    "gear.svg",
    "minimize.svg",
    "maximize.svg",
    "close.svg",
    "chevron-up.svg",
    "chevron-down.svg",
    "mouse.svg",
    "mouse-left-fill.svg",
    "mouse-middle-fill.svg",
    "mouse-right-fill.svg",
    "infinity.svg",
    "play.svg",
    "stop.svg",
];

/// Static instances of the variable fonts: GPUI's Linux text system matches faces by
/// weight and never applies a variable font's `wght` axis.
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/Manrope-400.ttf"),
    include_bytes!("../assets/fonts/Manrope-620.ttf"),
    include_bytes!("../assets/fonts/Manrope-650.ttf"),
    include_bytes!("../assets/fonts/Manrope-700.ttf"),
    include_bytes!("../assets/fonts/Manrope-740.ttf"),
    include_bytes!("../assets/fonts/Manrope-760.ttf"),
    include_bytes!("../assets/fonts/Manrope-780.ttf"),
    include_bytes!("../assets/fonts/Manrope-800.ttf"),
    include_bytes!("../assets/fonts/RobotoCondensed-620.ttf"),
    include_bytes!("../assets/fonts/RobotoCondensed-650.ttf"),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut entries: Vec<SharedString> = ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect();
        entries.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(entries)
    }
}
