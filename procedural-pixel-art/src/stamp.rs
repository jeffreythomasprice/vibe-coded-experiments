//! Hand-editable pixel stamps, stored as text in `assets/stamps/`.
//!
//! ```text
//! material=hair anchor=2,1
//! .##.
//! #34#
//! ```
//!
//! `.` is empty, `#` outline, `1`-`4` a shade of the header's material (1 darkest),
//! `w` white and `k` black. A creature's head also needs a few colours beyond its
//! material: `e`/`E` dim and bright eye, `n`/`N` dark and light horn, and `r` the
//! inside of the mouth. The anchor is the stamp position that gets pinned to
//! the part's centre. Lines starting with `;` are comments.

use crate::creature::Facing;
use crate::palette::Material;
use crate::pixbuf::Pixel;

#[derive(Debug)]
pub struct Stamp {
    pub width: i32,
    pub height: i32,
    pub anchor: (i32, i32),
    pixels: Vec<Option<Pixel>>,
}

impl Stamp {
    pub fn get(&self, x: i32, y: i32) -> Option<Pixel> {
        self.pixels[(y * self.width + x) as usize]
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.is_empty() && !l.starts_with(';'));

        let header = lines.next().ok_or("empty stamp")?;
        let (mut material, mut anchor) = (None, None);
        for field in header.split_whitespace() {
            match field.split_once('=') {
                Some(("material", name)) => material = Some(parse_material(name)?),
                Some(("anchor", value)) => {
                    let (x, y) = value.split_once(',').ok_or("anchor needs x,y")?;
                    let number = |s: &str| s.parse::<i32>().map_err(|e| format!("anchor: {e}"));
                    anchor = Some((number(x)?, number(y)?));
                }
                _ => return Err(format!("unknown header field {field:?}")),
            }
        }
        let material = material.ok_or("header is missing material=")?;
        let anchor = anchor.ok_or("header is missing anchor=")?;

        let rows: Vec<&str> = lines.collect();
        let width = rows.first().ok_or("stamp has no rows")?.chars().count();
        let mut pixels = Vec::with_capacity(width * rows.len());
        for (y, row) in rows.iter().enumerate() {
            if row.chars().count() != width {
                return Err(format!("row {y} is not {width} wide"));
            }
            for c in row.chars() {
                let flat = |material| Some(Pixel { material, shade: 0 });
                let shaded = |material, shade| Some(Pixel { material, shade });
                pixels.push(match c {
                    '.' => None,
                    '#' => Some(Pixel::OUTLINE),
                    'w' => flat(Material::White),
                    'k' => flat(Material::Black),
                    'r' => flat(Material::Maw),
                    'e' => shaded(Material::Eye, 1),
                    'E' => shaded(Material::Eye, 3),
                    'n' => shaded(Material::Horn, 1),
                    'N' => shaded(Material::Horn, 3),
                    '1'..='4' => Some(Pixel {
                        material,
                        shade: c as u8 - b'1',
                    }),
                    _ => return Err(format!("row {y}: unknown pixel {c:?}")),
                });
            }
        }

        Ok(Self {
            width: width as i32,
            height: rows.len() as i32,
            anchor,
            pixels,
        })
    }
}

fn parse_material(name: &str) -> Result<Material, String> {
    match name {
        "skin" => Ok(Material::Skin),
        "hair" => Ok(Material::Hair),
        "shirt" => Ok(Material::Shirt),
        "boots" => Ok(Material::Boots),
        "hat" => Ok(Material::Hat),
        "fur" => Ok(Material::Fur),
        "fire" => Ok(Material::Fire),
        _ => Err(format!("unknown material {name:?}")),
    }
}

/// One stamp per drawn facing; west reuses `side`, mirrored.
#[derive(Debug)]
pub struct Views {
    pub south: Stamp,
    pub north: Stamp,
    pub side: Stamp,
}

impl Views {
    /// The stamp that shows a part from the given direction.
    pub fn facing(&self, facing: Facing) -> &Stamp {
        match facing {
            Facing::South => &self.south,
            Facing::North => &self.north,
            Facing::East | Facing::West => &self.side,
        }
    }
}

#[derive(Debug)]
pub struct Stamps {
    pub face_south: Stamp,
    pub face_side: Stamp,
    pub hair: Views,
    pub hat: Views,
    pub hound_head: Views,
    /// Flicker frames.
    pub flame_large: [Stamp; 3],
    pub flame_small: [Stamp; 3],
}

macro_rules! load {
    ($name:literal) => {
        Stamp::parse(include_str!(concat!("../assets/stamps/", $name, ".txt")))
            .map_err(|e| format!("{}: {e}", $name))?
    };
}

impl Stamps {
    pub fn load() -> Result<Self, String> {
        Ok(Self {
            face_south: load!("face_south"),
            face_side: load!("face_side"),
            hair: Views {
                south: load!("hair_south"),
                north: load!("hair_north"),
                side: load!("hair_side"),
            },
            hat: Views {
                south: load!("hat_south"),
                north: load!("hat_north"),
                side: load!("hat_side"),
            },
            hound_head: Views {
                south: load!("hound_head_south"),
                north: load!("hound_head_north"),
                side: load!("hound_head_side"),
            },
            flame_large: [
                load!("flame_large_0"),
                load!("flame_large_1"),
                load!("flame_large_2"),
            ],
            flame_small: [
                load!("flame_small_0"),
                load!("flame_small_1"),
                load!("flame_small_2"),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_pixel_kind() {
        let s = Stamp::parse("; note\nmaterial=hair anchor=1,0\n.#1\n4wk\n").unwrap();
        assert_eq!((s.width, s.height, s.anchor), (3, 2, (1, 0)));
        assert_eq!(s.get(0, 0), None);
        assert_eq!(s.get(1, 0), Some(Pixel::OUTLINE));
        assert_eq!(s.get(0, 1).unwrap().shade, 3);
        assert_eq!(s.get(2, 1).unwrap().material, Material::Black);
    }

    #[test]
    fn rejects_ragged_rows() {
        assert!(Stamp::parse("material=hat anchor=0,0\n..\n.\n").is_err());
    }

    #[test]
    fn bundled_stamps_load() {
        Stamps::load().unwrap();
    }
}
