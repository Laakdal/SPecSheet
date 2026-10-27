//! CityHash64, version 1.0.3: a port of Google's CityHash (MIT, Copyright
//! (c) 2011 Google, Inc.; notice in `THIRD_PARTY_NOTICES.md`).
//!
//! HashFS archives key every entry by `CityHash64(path)`. The game uses the
//! 1.0.3 revision of the algorithm, which differs from the current v1.1 for
//! inputs of up to 64 bytes (see `docs/formats/hashfs.md`), so the common
//! crates implementing v1.1 cannot be used.

const K0: u64 = 0xc3a5_c85c_97cb_3127;
const K1: u64 = 0xb492_b66f_be98_f273;
const K2: u64 = 0x9ae1_6a3b_2f90_404f;
const K3: u64 = 0xc949_d7c7_509e_6557;
const K_MUL: u64 = 0x9ddf_ea08_eb38_2d69;

/// Hashes `data` with CityHash64 v1.0.3.
pub fn city_hash_64(data: &[u8]) -> u64 {
    let len = data.len();
    if len <= 16 {
        return hash_len_0_to_16(data);
    }
    if len <= 32 {
        return hash_len_17_to_32(data);
    }
    if len <= 64 {
        return hash_len_33_to_64(data);
    }

    let mut x = fetch64(data, len - 40);
    let mut y = fetch64(data, len - 16).wrapping_add(fetch64(data, len - 56));
    let mut z = hash_len_16(
        fetch64(data, len - 48).wrapping_add(len as u64),
        fetch64(data, len - 24),
    );
    let mut v = weak_hash_len_32_with_seeds(data, len - 64, len as u64, z);
    let mut w = weak_hash_len_32_with_seeds(data, len - 32, y.wrapping_add(K1), x);
    x = x.wrapping_mul(K1).wrapping_add(fetch64(data, 0));

    let mut offset = 0;
    let mut remaining = (len - 1) & !63;
    loop {
        x = rotate(
            x.wrapping_add(y)
                .wrapping_add(v.0)
                .wrapping_add(fetch64(data, offset + 8)),
            37,
        )
        .wrapping_mul(K1);
        y = rotate(
            y.wrapping_add(v.1).wrapping_add(fetch64(data, offset + 48)),
            42,
        )
        .wrapping_mul(K1);
        x ^= w.1;
        y = y.wrapping_add(v.0).wrapping_add(fetch64(data, offset + 40));
        z = rotate(z.wrapping_add(w.0), 33).wrapping_mul(K1);
        v = weak_hash_len_32_with_seeds(data, offset, v.1.wrapping_mul(K1), x.wrapping_add(w.0));
        w = weak_hash_len_32_with_seeds(
            data,
            offset + 32,
            z.wrapping_add(w.1),
            y.wrapping_add(fetch64(data, offset + 16)),
        );
        std::mem::swap(&mut z, &mut x);
        offset += 64;
        remaining -= 64;
        if remaining == 0 {
            break;
        }
    }
    hash_len_16(
        hash_len_16(v.0, w.0)
            .wrapping_add(shift_mix(y).wrapping_mul(K1))
            .wrapping_add(z),
        hash_len_16(v.1, w.1).wrapping_add(x),
    )
}

fn fetch64(data: &[u8], at: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&data[at..at + 8]);
    u64::from_le_bytes(buf)
}

fn fetch32(data: &[u8], at: usize) -> u64 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&data[at..at + 4]);
    u64::from(u32::from_le_bytes(buf))
}

fn rotate(value: u64, shift: u32) -> u64 {
    value.rotate_right(shift)
}

fn shift_mix(value: u64) -> u64 {
    value ^ (value >> 47)
}

fn hash_len_16(u: u64, v: u64) -> u64 {
    let mut a = (u ^ v).wrapping_mul(K_MUL);
    a ^= a >> 47;
    let mut b = (v ^ a).wrapping_mul(K_MUL);
    b ^= b >> 47;
    b.wrapping_mul(K_MUL)
}

fn hash_len_0_to_16(data: &[u8]) -> u64 {
    let len = data.len();
    if len > 8 {
        let a = fetch64(data, 0);
        let b = fetch64(data, len - 8);
        // `RotateByAtLeast1` in the reference; `len` is in 9..=16 here.
        return hash_len_16(a, b.wrapping_add(len as u64).rotate_right(len as u32)) ^ b;
    }
    if len >= 4 {
        let a = fetch32(data, 0);
        return hash_len_16((len as u64).wrapping_add(a << 3), fetch32(data, len - 4));
    }
    if len > 0 {
        let a = u64::from(data[0]);
        let b = u64::from(data[len >> 1]);
        let c = u64::from(data[len - 1]);
        let y = a.wrapping_add(b << 8);
        let z = (len as u64).wrapping_add(c << 2);
        return shift_mix(y.wrapping_mul(K2) ^ z.wrapping_mul(K3)).wrapping_mul(K2);
    }
    K2
}

fn hash_len_17_to_32(data: &[u8]) -> u64 {
    let len = data.len();
    let a = fetch64(data, 0).wrapping_mul(K1);
    let b = fetch64(data, 8);
    let c = fetch64(data, len - 8).wrapping_mul(K2);
    let d = fetch64(data, len - 16).wrapping_mul(K0);
    hash_len_16(
        rotate(a.wrapping_sub(b), 43)
            .wrapping_add(rotate(c, 30))
            .wrapping_add(d),
        a.wrapping_add(rotate(b ^ K3, 20))
            .wrapping_sub(c)
            .wrapping_add(len as u64),
    )
}

fn hash_len_33_to_64(data: &[u8]) -> u64 {
    let len = data.len();
    let mut z = fetch64(data, 24);
    let mut a = fetch64(data, 0).wrapping_add(
        (len as u64)
            .wrapping_add(fetch64(data, len - 16))
            .wrapping_mul(K0),
    );
    let mut b = rotate(a.wrapping_add(z), 52);
    let mut c = rotate(a, 37);
    a = a.wrapping_add(fetch64(data, 8));
    c = c.wrapping_add(rotate(a, 7));
    a = a.wrapping_add(fetch64(data, 16));
    let vf = a.wrapping_add(z);
    let vs = b.wrapping_add(rotate(a, 31)).wrapping_add(c);

    a = fetch64(data, 16).wrapping_add(fetch64(data, len - 32));
    z = fetch64(data, len - 8);
    b = rotate(a.wrapping_add(z), 52);
    c = rotate(a, 37);
    a = a.wrapping_add(fetch64(data, len - 24));
    c = c.wrapping_add(rotate(a, 7));
    a = a.wrapping_add(fetch64(data, len - 16));
    let wf = a.wrapping_add(z);
    let ws = b.wrapping_add(rotate(a, 31)).wrapping_add(c);

    let r = shift_mix(
        vf.wrapping_add(ws)
            .wrapping_mul(K2)
            .wrapping_add(wf.wrapping_add(vs).wrapping_mul(K0)),
    );
    shift_mix(r.wrapping_mul(K0).wrapping_add(vs)).wrapping_mul(K2)
}

fn weak_hash_len_32_with_seeds(data: &[u8], at: usize, a: u64, b: u64) -> (u64, u64) {
    let w = fetch64(data, at);
    let x = fetch64(data, at + 8);
    let y = fetch64(data, at + 16);
    let z = fetch64(data, at + 24);
    let mut a = a.wrapping_add(w);
    let mut b = rotate(b.wrapping_add(a).wrapping_add(z), 21);
    let c = a;
    a = a.wrapping_add(x).wrapping_add(y);
    b = b.wrapping_add(rotate(a, 44));
    (a.wrapping_add(z), b.wrapping_add(c))
}

#[cfg(test)]
mod tests {
    use super::city_hash_64;

    /// Hashes observed in shipped ETS2 1.60 archives (`version.scs`, root
    /// listing and its only file); see `docs/formats/hashfs.md`.
    #[test]
    fn matches_hashes_observed_in_game_archives() {
        assert_eq!(city_hash_64(b""), 0x9ae1_6a3b_2f90_404f);
        assert_eq!(city_hash_64(b"version.sii"), 0x02d3_3850_6339_918d);
    }
}
