# /// script
# requires-python = ">=3.11"
# ///
"""Pack Natural Earth 1:110m country boundaries (public domain) for the `map` chart.

Usage: uv run richview-chart/scripts/build_world.py countries.geojson richview-chart/data/world-110m.bin

Natural Earth: https://www.naturalearthdata.com/about/terms-of-use/ (no restrictions).
Layout (all integers little-endian):
  u16 countries
  per country: u8 length + UTF-8 for each of iso3, iso2, name, long name; u8 rings;
    per ring: u16 vertices; first vertex as i16 longitude and latitude in 0.01 degrees; every
    further vertex as two zigzag LEB128 varints, the change in 0.01 degrees. Every ring is an
    outer boundary; holes are not drawn at this scale.
"""
import json
import struct
import sys


def zigzag(n: int) -> int:
    return (n << 1) ^ (n >> 63)


def varint(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def text(s: str) -> bytes:
    raw = s.encode()[:255]
    return bytes([len(raw)]) + raw


def main(src: str, dst: str) -> None:
    features = json.load(open(src))["features"]
    out = bytearray()
    kept = []
    for f in features:
        p = f["properties"]
        iso3 = p.get("ADM0_A3") or p.get("ISO_A3") or ""
        iso2 = p.get("ISO_A2_EH") if p.get("ISO_A2_EH") not in (None, "-99") else (p.get("POSTAL") or "")
        geom = f["geometry"]
        polys = geom["coordinates"] if geom["type"] == "MultiPolygon" else [geom["coordinates"]]
        rings = [poly[0] for poly in polys]
        kept.append((iso3, iso2, p.get("NAME") or "", p.get("NAME_LONG") or "", rings))
    out += struct.pack("<H", len(kept))
    for iso3, iso2, name, long_name, rings in kept:
        for s in (iso3, iso2, name, long_name):
            out += text(s)
        out += bytes([len(rings)])
        for ring in rings:
            pts = [(round(x * 100), round(y * 100)) for x, y in ring]
            dedup = [pts[0]]
            for q in pts[1:]:
                if q != dedup[-1]:
                    dedup.append(q)
            if dedup[0] == dedup[-1] and len(dedup) > 1:
                dedup.pop()
            out += struct.pack("<H", len(dedup))
            out += struct.pack("<hh", *dedup[0])
            for a, b in zip(dedup, dedup[1:]):
                out += varint(zigzag(b[0] - a[0])) + varint(zigzag(b[1] - a[1]))
    open(dst, "wb").write(out)
    print(f"{len(kept)} countries, {len(out)} bytes")


if __name__ == "__main__":
    main(*sys.argv[1:3])
