//! 头像位图缓存：第一次用到某个头像某档分辨率时解码，之后复用。

use std::collections::HashMap;

use tiny_skia::Pixmap;

use super::avatar::{Avatar, Resolution};

#[derive(Default)]
pub(super) struct AvatarCache {
    decoded: HashMap<(Avatar, Resolution), Option<Pixmap>>,
}

impl AvatarCache {
    /// 取能盖住 `target_px`（目标边长，像素）的头像位图；PNG 损坏返回 `None`（只会是素材被改坏了，测试会拦住）。
    pub(super) fn get(&mut self, avatar: Avatar, target_px: f32) -> Option<&Pixmap> {
        let resolution = Resolution::for_target(target_px);
        self.decoded
            .entry((avatar, resolution))
            .or_insert_with(|| decode(avatar, resolution))
            .as_ref()
    }
}

fn decode(avatar: Avatar, resolution: Resolution) -> Option<Pixmap> {
    match Pixmap::decode_png(avatar.png(resolution)) {
        Ok(pixmap) => Some(pixmap),
        Err(error) => {
            tracing::warn!(?avatar, ?resolution, %error, "头像 PNG 解码失败");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_every_asset_to_the_documented_size() {
        let mut cache = AvatarCache::default();
        for avatar in [Avatar::Boy, Avatar::Girl] {
            for (target, side) in [(40.0, 40), (80.0, 80), (120.0, 120), (300.0, 512)] {
                let pixmap = cache.get(avatar, target).expect("头像应能解码");
                assert_eq!((pixmap.width(), pixmap.height()), (side, side));
            }
        }
    }

    #[test]
    fn assets_are_circular_with_transparent_corners() {
        let mut cache = AvatarCache::default();
        for avatar in [Avatar::Boy, Avatar::Girl] {
            let pixmap = cache.get(avatar, 80.0).unwrap();
            assert_eq!(pixmap.pixel(0, 0).unwrap().alpha(), 0);
            assert_eq!(pixmap.pixel(40, 40).unwrap().alpha(), 255);
        }
    }
}
