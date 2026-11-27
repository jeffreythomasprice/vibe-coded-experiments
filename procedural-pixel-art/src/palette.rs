//! Materials and their colour ramps. Art is stored as (material, shade) pairs and
//! only turned into colours at the very end, which is what makes recolouring free.

use crate::rng::Rng;

pub type Rgb = [u8; 3];

/// Four shades, darkest first.
pub type Ramp = [Rgb; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Outline,
    White,
    Black,
    Skin,
    Hair,
    Shirt,
    Boots,
    Hat,
    Fur,
    Fire,
    Eye,
    Horn,
    Maw,
}

const OUTLINE: Rgb = [0x1a, 0x12, 0x1c];
const WHITE: Rgb = [0xff, 0xf8, 0xf0];
const BLACK: Rgb = [0x24, 0x18, 0x28];
const MAW: Rgb = [0x8c, 0x1c, 0x28];

const fn rgb(hex: u32) -> Rgb {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

const fn ramp(a: u32, b: u32, c: u32, d: u32) -> Ramp {
    [rgb(a), rgb(b), rgb(c), rgb(d)]
}

const SKIN: [Ramp; 5] = [
    ramp(0xb86f50, 0xd9946c, 0xf2bd91, 0xffdcb8),
    ramp(0x9c5a3c, 0xc07a52, 0xdea071, 0xf0c096),
    ramp(0x6e3b2a, 0x8f5238, 0xb4714c, 0xcf9066),
    ramp(0x45261f, 0x5e3629, 0x7d4b36, 0x9c6448),
    ramp(0xc2807a, 0xe6a89c, 0xf9cfc0, 0xffe8dc),
];

const HAIR: [Ramp; 6] = [
    ramp(0x3a1f1a, 0x5c3222, 0x84502e, 0xac7440),
    ramp(0x17141f, 0x28243a, 0x3d3a57, 0x5c5a7d),
    ramp(0x8a5a1e, 0xc08a28, 0xe8bc44, 0xffe27a),
    ramp(0x5e1a18, 0x92301e, 0xc4502a, 0xe87c3c),
    ramp(0x54586a, 0x82889a, 0xb2b8c6, 0xdde2ea),
    ramp(0x23406a, 0x2f66a0, 0x4896cc, 0x7cc8e8),
];

const CLOTH: [Ramp; 6] = [
    ramp(0x1c2a5e, 0x28468c, 0x3a6cbc, 0x5c9ce0),
    ramp(0x5a1428, 0x8c2034, 0xc23440, 0xe86058),
    ramp(0x143c2c, 0x1e603a, 0x38904a, 0x70bc58),
    ramp(0x7a4410, 0xb06c14, 0xde9c20, 0xf8cc4c),
    ramp(0x34184e, 0x522878, 0x7c40a4, 0xaa6cc8),
    ramp(0x3c4048, 0x5c6470, 0x8a94a0, 0xbcc6cc),
];

const BOOTS: [Ramp; 3] = [
    ramp(0x2a1a14, 0x442a1c, 0x643e26, 0x865834),
    ramp(0x141418, 0x24242c, 0x3a3a46, 0x555564),
    ramp(0x3c2c24, 0x6a5038, 0x987850, 0xc0a070),
];

// All near black, but spread enough that the shading still reads.
const FUR: [Ramp; 3] = [
    ramp(0x17131c, 0x211c28, 0x302a3a, 0x4a4258),
    ramp(0x161214, 0x231a1c, 0x342628, 0x52383a),
    ramp(0x121418, 0x1c2026, 0x2a303a, 0x424a58),
];

/// Matching (flame, eye) colours, so one hound burns one way.
const HELLFIRE: [(Ramp, Ramp); 4] = [
    // Orange, listed twice to make it the usual one.
    (
        ramp(0x9c1c10, 0xe0481c, 0xfc9420, 0xffe468),
        ramp(0x8c1c10, 0xd8381c, 0xfc7418, 0xffb830),
    ),
    // Blue.
    (
        ramp(0x20307c, 0x2c70d0, 0x48c0f0, 0xd0fcff),
        ramp(0x1c4890, 0x2884d8, 0x48c8f8, 0xb0f4ff),
    ),
    // Green.
    (
        ramp(0x14582c, 0x2c9c30, 0x88dc30, 0xf0ff90),
        ramp(0x206020, 0x40a828, 0x90e828, 0xe0ff60),
    ),
    (
        ramp(0x9c1c10, 0xe0481c, 0xfc9420, 0xffe468),
        ramp(0x8c1c10, 0xd8381c, 0xfc7418, 0xffb830),
    ),
];

const HORN: [Ramp; 3] = [
    ramp(0x4c4040, 0x74645c, 0xa49080, 0xd4c4a8),
    ramp(0x3c1418, 0x641c20, 0x902c2c, 0xbc4c40),
    ramp(0x34343c, 0x54545c, 0x7c7c84, 0xaaaab0),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    skin: Ramp,
    hair: Ramp,
    shirt: Ramp,
    boots: Ramp,
    hat: Ramp,
    fur: Ramp,
    fire: Ramp,
    eye: Ramp,
    horn: Ramp,
}

impl Palette {
    /// What every palette starts from. Each kind of creature then picks the
    /// materials it actually draws with.
    const BASE: Self = Self {
        skin: SKIN[0],
        hair: HAIR[0],
        shirt: CLOTH[0],
        boots: BOOTS[0],
        hat: CLOTH[1],
        fur: FUR[0],
        fire: HELLFIRE[0].0,
        eye: HELLFIRE[0].1,
        horn: HORN[0],
    };

    pub fn human(rng: &mut Rng) -> Self {
        Self {
            skin: rng.pick(&SKIN),
            hair: rng.pick(&HAIR),
            shirt: rng.pick(&CLOTH),
            boots: rng.pick(&BOOTS),
            hat: rng.pick(&CLOTH),
            ..Self::BASE
        }
    }

    pub fn hound(rng: &mut Rng) -> Self {
        let fur = rng.pick(&FUR);
        let (fire, eye) = rng.pick(&HELLFIRE);
        Self {
            fur,
            fire,
            eye,
            horn: rng.pick(&HORN),
            ..Self::BASE
        }
    }

    /// `shade` is 0 (darkest) to 3 (lightest); the flat materials ignore it.
    pub fn color(&self, material: Material, shade: u8) -> Rgb {
        let ramp = match material {
            Material::Outline => return OUTLINE,
            Material::White => return WHITE,
            Material::Black => return BLACK,
            Material::Maw => return MAW,
            Material::Skin => &self.skin,
            Material::Hair => &self.hair,
            Material::Shirt => &self.shirt,
            Material::Boots => &self.boots,
            Material::Hat => &self.hat,
            Material::Fur => &self.fur,
            Material::Fire => &self.fire,
            Material::Eye => &self.eye,
            Material::Horn => &self.horn,
        };
        ramp[usize::from(shade.min(3))]
    }
}
