pub const BACKGROUND: [u8; 3] = [41, 43, 48];

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h % 360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// Stable flat colour for a material name; the empty name is grey.
pub fn material_rgb8(name: &str) -> [u8; 3] {
    if name.is_empty() {
        return [158, 158, 158];
    }
    let h = fnv1a(name);
    let hue = (h % 360) as f64;
    let lightness = 0.45 + ((h >> 16) & 0xff) as f64 / 255.0 * 0.2;
    hsl_to_rgb(hue, 0.5, lightness).map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
}

pub fn to_f32(rgb: [u8; 3]) -> [f32; 3] {
    rgb.map(|c| f32::from(c) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_distinct() {
        assert_eq!(
            material_rgb8("base/concrete_01"),
            material_rgb8("base/concrete_01")
        );
        assert_ne!(
            material_rgb8("base/concrete_01"),
            material_rgb8("base/concrete_02")
        );
        assert_eq!(material_rgb8(""), [158, 158, 158]);
        assert_eq!(material_rgb8("a"), [57, 142, 172]);
    }
}
