//! World boundaries for the `map` chart (FEA-CH-76, ARC-11): Natural Earth 1:110m countries,
//! public domain, packed by `scripts/build_world.py` and compiled into the engine, so a map never
//! asks a server for its shapes and the offline file works. Projections are plain formulas.

use std::sync::OnceLock;

static PACKED: &[u8] = include_bytes!("../data/world-110m.bin");

/// One country: its codes, its names and its outer rings in degrees (longitude, latitude).
pub struct Country {
    pub iso3: String,
    pub iso2: String,
    pub name: String,
    pub long_name: String,
    pub rings: Vec<Vec<[f64; 2]>>,
}

pub struct World {
    pub countries: Vec<Country>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> u8 {
        let b = self.bytes[self.at];
        self.at += 1;
        b
    }
    fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }
    fn i16(&mut self) -> i16 {
        self.u16() as i16
    }
    fn text(&mut self) -> String {
        let n = self.u8() as usize;
        let s = String::from_utf8_lossy(&self.bytes[self.at..self.at + n]).into_owned();
        self.at += n;
        s
    }
    fn varint(&mut self) -> i64 {
        let (mut value, mut shift) = (0u64, 0);
        loop {
            let b = self.u8();
            value |= u64::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        ((value >> 1) as i64) ^ -((value & 1) as i64)
    }
}

/// The packed countries, read once.
pub fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let mut r = Reader { bytes: PACKED, at: 0 };
        let n = r.u16() as usize;
        let mut countries = Vec::with_capacity(n);
        for _ in 0..n {
            let (iso3, iso2, name, long_name) = (r.text(), r.text(), r.text(), r.text());
            let rings = (0..r.u8())
                .map(|_| {
                    let count = r.u16() as usize;
                    let mut at = [i64::from(r.i16()), i64::from(r.i16())];
                    let mut ring = vec![[at[0] as f64 / 100.0, at[1] as f64 / 100.0]];
                    for _ in 1..count {
                        at[0] += r.varint();
                        at[1] += r.varint();
                        ring.push([at[0] as f64 / 100.0, at[1] as f64 / 100.0]);
                    }
                    ring
                })
                .collect();
            countries.push(Country {
                iso3,
                iso2,
                name,
                long_name,
                rings,
            });
        }
        World { countries }
    })
}

/// Names authors commonly write that Natural Earth spells differently.
const ALIASES: &[(&str, &str)] = &[
    ("usa", "USA"),
    ("us", "USA"),
    ("united states", "USA"),
    ("uk", "GBR"),
    ("great britain", "GBR"),
    ("britain", "GBR"),
    ("russia", "RUS"),
    ("czechia", "CZE"),
    ("czech republic", "CZE"),
    ("south korea", "KOR"),
    ("north korea", "PRK"),
    ("ivory coast", "CIV"),
    ("swaziland", "SWZ"),
    ("macedonia", "MKD"),
    ("burma", "MMR"),
];

impl World {
    /// The country a region name or code stands for: ISO alpha-3 or alpha-2 code, short or long
    /// name, or a common alias, without regard to case.
    pub fn find(&self, key: &str) -> Option<usize> {
        let k = key.trim().to_lowercase();
        if k.is_empty() {
            return None;
        }
        let k = ALIASES
            .iter()
            .find(|(alias, _)| *alias == k)
            .map_or(k, |(_, code)| code.to_lowercase());
        self.countries.iter().position(|c| {
            c.iso3.to_lowercase() == k
                || c.iso2.to_lowercase() == k
                || c.name.to_lowercase() == k
                || c.long_name.to_lowercase() == k
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    NaturalEarth,
    EqualEarth,
    Mercator,
    Equirectangular,
}

impl Projection {
    pub fn parse(name: &str) -> Option<Projection> {
        Some(match name {
            "natural-earth" => Projection::NaturalEarth,
            "equal-earth" => Projection::EqualEarth,
            "mercator" => Projection::Mercator,
            "equirectangular" => Projection::Equirectangular,
            _ => return None,
        })
    }

    /// Longitude and latitude in degrees to plane coordinates with y up, in radians' scale.
    pub fn project(self, lon: f64, lat: f64) -> [f64; 2] {
        let (l, p) = (lon.to_radians(), lat.to_radians());
        match self {
            Projection::Equirectangular => [l, p],
            Projection::Mercator => {
                // Poles stop at 85 degrees, where the projection diverges.
                let p = p.clamp(-85f64.to_radians(), 85f64.to_radians());
                [l, (std::f64::consts::FRAC_PI_4 + p / 2.0).tan().ln()]
            }
            Projection::EqualEarth => {
                let theta = (3f64.sqrt() / 2.0 * p.sin()).asin();
                let t2 = theta * theta;
                let t6 = t2 * t2 * t2;
                let (a1, a2, a3, a4) = (1.340264, -0.081106, 0.000893, 0.003796);
                let x = 2.0 * 3f64.sqrt() * l * theta.cos()
                    / (3.0 * (a1 + 3.0 * a2 * t2 + t6 * (7.0 * a3 + 9.0 * a4 * t2)));
                let y = theta * (a1 + a2 * t2 + t6 * (a3 + a4 * t2));
                [x, y]
            }
            Projection::NaturalEarth => {
                let p2 = p * p;
                let p4 = p2 * p2;
                let x = l * (0.8707 - 0.131979 * p2 + p4 * (-0.013791 + p4 * (0.003971 * p2 - 0.001529 * p4)));
                let y = p * (1.007226 + p2 * (0.015085 + p4 * (-0.044475 + 0.028874 * p2 - 0.005916 * p4)));
                [x, y]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packed_world_reads_back() {
        let w = world();
        assert!(w.countries.len() > 170);
        let cz = &w.countries[w.find("Czechia").expect("Czechia")];
        assert_eq!(cz.iso3, "CZE");
        let ring = &cz.rings[0];
        let (lon, lat) = (
            ring.iter().map(|p| p[0]).sum::<f64>() / ring.len() as f64,
            ring.iter().map(|p| p[1]).sum::<f64>() / ring.len() as f64,
        );
        assert!(
            (12.0..19.0).contains(&lon) && (48.0..51.5).contains(&lat),
            "{lon} {lat}"
        );
        assert_eq!(w.find("us"), w.find("United States of America"));
        assert!(w.find("Atlantis").is_none());
    }

    #[test]
    fn projections_agree_at_the_origin_and_stay_finite() {
        for p in [
            Projection::NaturalEarth,
            Projection::EqualEarth,
            Projection::Mercator,
            Projection::Equirectangular,
        ] {
            assert!(p.project(0.0, 0.0).iter().all(|v| v.abs() < 1e-12));
            for (lon, lat) in [(180.0, 90.0), (-180.0, -90.0), (10.0, 50.0)] {
                assert!(p.project(lon, lat).iter().all(|v| v.is_finite()));
            }
            // North is up in the plane, east is right.
            assert!(p.project(0.0, 40.0)[1] > 0.0 && p.project(40.0, 0.0)[0] > 0.0);
        }
    }
}
