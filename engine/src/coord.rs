//! 바둑식 좌표 표기: 열 A~T (I 제외), 행은 아래쪽이 1.
//! 내부 인덱스는 `y * size + x`, y=0이 맨 윗줄.

pub const COLS: &[u8] = b"ABCDEFGHJKLMNOPQRST";

pub fn format(idx: usize, size: usize) -> String {
    let (x, y) = (idx % size, idx / size);
    format!("{}{}", COLS[x] as char, size - y)
}

pub fn parse(s: &str, size: usize) -> Option<usize> {
    let s = s.trim().to_ascii_uppercase();
    let mut chars = s.chars();
    let c = chars.next()? as u8;
    let x = COLS.iter().position(|&k| k == c)?;
    let row: usize = chars.as_str().parse().ok()?;
    if x >= size || row == 0 || row > size {
        return None;
    }
    Some((size - row) * size + x)
}

pub fn ascii(board: &[u8], size: usize) -> String {
    let header: String = std::iter::once("   ".to_string())
        .chain(COLS[..size].iter().map(|&c| format!("{} ", c as char)))
        .collect();
    let mut out = vec![header.trim_end().to_string()];
    for y in 0..size {
        let n = size - y;
        let row: Vec<&str> = (0..size)
            .map(|x| match board[y * size + x] {
                1 => "X",
                2 => "O",
                _ => ".",
            })
            .collect();
        out.push(format!("{n:>2} {} {n:>2}", row.join(" ")));
    }
    out.push(out[0].clone());
    out.join("\n")
}

/// 결정적 의사난수 (대체 수의 동점 처리용)
pub fn hash_noise(a: usize, b: usize) -> f64 {
    let mut h = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h % 1000) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        for size in [9, 13, 15, 19] {
            for i in 0..size * size {
                assert_eq!(parse(&format(i, size), size), Some(i));
            }
        }
        assert_eq!(parse("I5", 19), None);
        assert_eq!(parse("K10", 9), None);
        assert_eq!(format(0, 9), "A9");
    }
}
