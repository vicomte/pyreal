//! The bits a pixel of each source picture format takes, by the format's raw id: what an image
//! record's byte count is checked against and what a picture decoder reads by.

/// `DXT1`, four bits a pixel in blocks of four by four.
const DXT1: u32 = 0x3154_5844;
/// `DXT2` to `DXT5`, eight bits a pixel in blocks of four by four.
const DXT2: u32 = 0x3254_5844;
const DXT5: u32 = 0x3554_5844;
/// The raw JPEG marker: the record's byte count is the file's.
const RAW_JPEG: u32 = 500;

/// The bits one pixel of `format` takes: 0 for a raw JPEG, `None` for a format the client does not
/// know.
#[must_use]
pub const fn bits_per_pixel(format: u32) -> Option<u32> {
    Some(match format {
        20 | 242 | 243 => 24,
        21 | 22 | 31 | 32 | 33 | 35 | 71 | 75 | 77 | 79 | 240 | 241 => 32,
        23 | 24 | 25 | 26 | 30 | 60 | 70 | 73 | 80 | 101 => 16,
        28 | 41 | 244 => 8,
        DXT1 => 4,
        DXT2..=DXT5 => 8,
        RAW_JPEG => 0,
        _ => return None,
    })
}

/// Whether `format` is stored in blocks of four by four.
#[must_use]
pub const fn is_block_compressed(format: u32) -> bool {
    matches!(format, DXT1..=DXT5)
}

/// The bytes a `width` by `height` picture of `format` takes, a block format in whole blocks;
/// `None` for a raw JPEG (its file's length) and a format the client does not know.
#[must_use]
pub fn image_bytes(format: u32, width: u32, height: u32) -> Option<u64> {
    let bpp = u64::from(bits_per_pixel(format)?);
    if bpp == 0 {
        return None;
    }
    let (w, h) = (u64::from(width), u64::from(height));
    Some(if is_block_compressed(format) {
        w.div_ceil(4) * h.div_ceil(4) * bpp * 2
    } else {
        (bpp * w).div_ceil(8) * h
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_eight_bytes_in_dxt1_and_sixteen_in_the_others() {
        assert_eq!(image_bytes(DXT1, 4, 4), Some(8));
        assert_eq!(image_bytes(DXT5, 5, 4), Some(32));
        assert_eq!(image_bytes(21, 3, 2), Some(24));
        assert_eq!(image_bytes(RAW_JPEG, 64, 64), None);
        assert_eq!(image_bytes(1234, 4, 4), None);
    }
}
