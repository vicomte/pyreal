//! The target marker is colourised with retail's integer hue and saturation arithmetic and keeps
//! its alpha. No dats, no device.

use pyreal_ui_logic::icon::SurfaceOp;

#[test]
fn colorize_preserves_alpha_and_value_with_retails_integer_hue_and_saturation() {
    // The original 1..256 integer HSV conversion gives 86, while multiplication gives 85, for
    // this grey/gold pair.
    assert_eq!(
        SurfaceOp::Colorize(0xFFFF_AB00).apply(0x8080_8080),
        0x8080_5600
    );
    assert_ne!(
        SurfaceOp::Colorize(0xFFFF_AB00).apply(0x8080_8080),
        SurfaceOp::Multiply(0xFFFF_AB00).apply(0x8080_8080)
    );
    for (color, white, grey) in [
        (0xFFFF_0000, 0xFFFF_0000, 0x8080_0000),
        (0xFFFF_FF00, 0xFFFF_FF00, 0x8080_8000),
        (0xFF00_FF00, 0xFF00_FF00, 0x8000_8000),
        (0xFF00_FFFF, 0xFF00_FFFF, 0x8000_8080),
        (0xFF00_00FF, 0xFF00_00FF, 0x8000_0080),
        (0xFFFF_00FF, 0xFFFF_00FF, 0x8080_0080),
        (0xFFFF_FFFF, 0xFFFF_FFFF, 0x8080_8080),
    ] {
        assert_eq!(SurfaceOp::Colorize(color).apply(0xFFFF_FFFF), white);
        assert_eq!(SurfaceOp::Colorize(color).apply(0x8080_8080), grey);
        assert_eq!(SurfaceOp::Colorize(color).apply(0x0100_0000), 0x0100_0000);
    }
}
