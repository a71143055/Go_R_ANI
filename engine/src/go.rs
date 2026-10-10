//! 바둑 규칙: 착수, 따내기, 패, 자충수 금지, 중국식(집+돌) 계가.

use crate::coord;
use serde::{Deserialize, Serialize};

pub const EMPTY: u8 = 0;
pub const BLACK: u8 = 1;
pub const WHITE: u8 = 2;

pub fn other(c: u8) -> u8 {
    3 - c
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GoState {
    pub size: usize,
    pub board: Vec<u8>,
    /// 1 = 흑, 2 = 백
    pub to_play: u8,
    /// [흑이 따낸 돌 수, 백이 따낸 돌 수]
    pub captures: [u32; 2],
    /// 지금 착수할 수 없는 패 자리
    pub ko: Option<usize>,
    pub komi: f32,
    /// "B D4", "W pass" 형식의 수순
    pub history: Vec<String>,
    /// 연속 패스 횟수 (2가 되면 종국)
    pub passes: u32,
    pub last: Option<usize>,
}

#[derive(Serialize, Debug)]
pub struct Score {
    pub black: f32,
    pub white: f32,
    pub winner: String,
    pub margin: f32,
    /// 칸별 소유: 0 중립, 1 흑, 2 백
    pub ownership: Vec<u8>,
    /// 실제로 사석 처리된 좌표 (빈 점 등 잘못된 좌표는 제외)
    pub dead: Vec<String>,
}

impl GoState {
    pub fn new(size: usize, komi: f32) -> Result<Self, String> {
        if !(5..=19).contains(&size) {
            return Err("판 크기는 5~19여야 합니다".into());
        }
        Ok(GoState {
            size,
            board: vec![EMPTY; size * size],
            to_play: BLACK,
            captures: [0, 0],
            ko: None,
            komi,
            history: vec![],
            passes: 0,
            last: None,
        })
    }

    pub fn neighbors(&self, i: usize) -> impl Iterator<Item = usize> {
        let n = self.size;
        let (x, y) = (i % n, i / n);
        let mut v = [usize::MAX; 4];
        if x > 0 { v[0] = i - 1; }
        if x + 1 < n { v[1] = i + 1; }
        if y > 0 { v[2] = i - n; }
        if y + 1 < n { v[3] = i + n; }
        v.into_iter().filter(|&j| j != usize::MAX)
    }

    /// (그룹의 돌들, 활로 수)
    pub fn group(&self, board: &[u8], start: usize) -> (Vec<usize>, usize) {
        let color = board[start];
        let mut seen = vec![false; board.len()];
        let mut libs = vec![false; board.len()];
        let mut stack = vec![start];
        let mut stones = vec![];
        seen[start] = true;
        let mut nlib = 0;
        while let Some(p) = stack.pop() {
            stones.push(p);
            for q in self.neighbors(p) {
                if board[q] == EMPTY {
                    if !libs[q] {
                        libs[q] = true;
                        nlib += 1;
                    }
                } else if board[q] == color && !seen[q] {
                    seen[q] = true;
                    stack.push(q);
                }
            }
        }
        (stones, nlib)
    }

    /// 착수를 시뮬레이션한다. 성공 시 (새 판, 따낸 돌 수, 새 패 자리).
    fn simulate(&self, idx: usize, color: u8) -> Result<(Vec<u8>, u32, Option<usize>), &'static str> {
        if self.board[idx] != EMPTY {
            return Err("이미 돌이 있는 자리입니다");
        }
        if self.ko == Some(idx) {
            return Err("패: 바로 되따낼 수 없습니다");
        }
        let mut b = self.board.clone();
        b[idx] = color;
        let mut captured = vec![];
        for q in self.neighbors(idx) {
            if b[q] == other(color) {
                let (stones, libs) = self.group(&b, q);
                if libs == 0 {
                    for s in stones {
                        b[s] = EMPTY;
                        captured.push(s);
                    }
                }
            }
        }
        let (mine, libs) = self.group(&b, idx);
        if libs == 0 {
            return Err("자충수(착수 금지)입니다");
        }
        // 한 점을 따내고, 놓은 돌이 활로 1개짜리 단독 돌이면 패
        let ko = if captured.len() == 1 && mine.len() == 1 && libs == 1 { Some(captured[0]) } else { None };
        Ok((b, captured.len() as u32, ko))
    }

    pub fn is_legal(&self, idx: usize) -> bool {
        self.simulate(idx, self.to_play).is_ok()
    }

    pub fn play_coord(&mut self, mv: &str) -> Result<u32, String> {
        let color_ch = if self.to_play == BLACK { 'B' } else { 'W' };
        if mv.trim().eq_ignore_ascii_case("pass") {
            self.passes += 1;
            self.ko = None;
            self.last = None;
            self.history.push(format!("{color_ch} pass"));
            self.to_play = other(self.to_play);
            return Ok(0);
        }
        let idx = coord::parse(mv, self.size).ok_or_else(|| format!("잘못된 좌표: {mv}"))?;
        let (b, n, ko) = self.simulate(idx, self.to_play).map_err(|e| format!("{mv}: {e}"))?;
        self.board = b;
        self.captures[(self.to_play - 1) as usize] += n;
        self.ko = ko;
        self.passes = 0;
        self.last = Some(idx);
        self.history.push(format!("{color_ch} {}", coord::format(idx, self.size)));
        self.to_play = other(self.to_play);
        Ok(n)
    }

    pub fn ascii(&self) -> String {
        coord::ascii(&self.board, self.size)
    }

    /// 빈 점 중 현재 차례에 둘 수 없는 곳 (패·자충)
    pub fn illegal_points(&self) -> Vec<String> {
        (0..self.board.len())
            .filter(|&i| self.board[i] == EMPTY && !self.is_legal(i))
            .map(|i| coord::format(i, self.size))
            .collect()
    }

    /// 사방이 내 돌(또는 판 끝)인 빈 점 = 대략적인 내 집눈
    fn is_own_eye(&self, i: usize, color: u8) -> bool {
        self.neighbors(i).all(|q| self.board[q] == color)
    }

    /// Claude 응답을 쓸 수 없을 때의 간단한 대체 수.
    pub fn fallback_move(&self) -> String {
        let color = self.to_play;
        let mut best: Option<(f64, usize)> = None;
        for i in 0..self.board.len() {
            if self.board[i] != EMPTY || self.is_own_eye(i, color) {
                continue;
            }
            let Ok((b, captured, _)) = self.simulate(i, color) else { continue };
            let (_, libs) = self.group(&b, i);
            let mut score = captured as f64 * 10.0 + libs.min(4) as f64;
            if libs == 1 {
                score -= 8.0; // 단수로 들어가는 수 회피
            }
            // 내 돌이 단수였다면 살리는 수 가산
            for q in self.neighbors(i) {
                if self.board[q] == color && self.group(&self.board, q).1 == 1 {
                    score += 6.0;
                }
            }
            // 가장자리 1선 회피, 3~4선 선호
            let n = self.size;
            let (x, y) = (i % n, i / n);
            let edge = x.min(y).min(n - 1 - x).min(n - 1 - y);
            score += match edge { 0 => -2.0, 1 => 0.0, 2 | 3 => 1.5, _ => 1.0 };
            score += coord::hash_noise(i, self.history.len());
            if best.map_or(true, |(s, _)| score > s) {
                best = Some((score, i));
            }
        }
        match best {
            Some((s, i)) if s > -3.0 => coord::format(i, self.size),
            _ => "pass".into(),
        }
    }

    /// 사석을 걷어낸 뒤 중국식 계가.
    pub fn score(&self, dead: &[String]) -> Score {
        let mut b = self.board.clone();
        let mut removed = vec![];
        for d in dead {
            if let Some(i) = coord::parse(d, self.size) {
                if b[i] != EMPTY {
                    b[i] = EMPTY;
                    removed.push(coord::format(i, self.size));
                }
            }
        }
        let mut owner = b.clone();
        let mut seen = vec![false; b.len()];
        for start in 0..b.len() {
            if b[start] != EMPTY || seen[start] {
                continue;
            }
            let mut region = vec![];
            let mut stack = vec![start];
            let mut border = 0u8; // 비트 1 = 흑과 접함, 비트 2 = 백과 접함
            seen[start] = true;
            while let Some(p) = stack.pop() {
                region.push(p);
                for q in self.neighbors(p) {
                    match b[q] {
                        EMPTY if !seen[q] => {
                            seen[q] = true;
                            stack.push(q);
                        }
                        BLACK => border |= 1,
                        WHITE => border |= 2,
                        _ => {}
                    }
                }
            }
            let o = match border { 1 => BLACK, 2 => WHITE, _ => EMPTY };
            for p in region {
                owner[p] = o;
            }
        }
        let black = owner.iter().filter(|&&c| c == BLACK).count() as f32;
        let white = owner.iter().filter(|&&c| c == WHITE).count() as f32 + self.komi;
        let (winner, margin) = if black > white { ("B", black - white) } else { ("W", white - black) };
        Score { black, white, winner: winner.into(), margin, ownership: owner, dead: removed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play_all(st: &mut GoState, moves: &[&str]) {
        for m in moves {
            st.play_coord(m).unwrap_or_else(|e| panic!("{m}: {e}"));
        }
    }

    #[test]
    fn capture_single_stone() {
        let mut st = GoState::new(9, 6.5).unwrap();
        // 흑이 E5 백돌을 포위
        play_all(&mut st, &["D5", "E5", "F5", "A1", "E4", "A2", "E6"]);
        assert_eq!(st.board[coord::parse("E5", 9).unwrap()], EMPTY);
        assert_eq!(st.captures, [1, 0]);
    }

    #[test]
    fn suicide_forbidden() {
        let mut st = GoState::new(9, 6.5).unwrap();
        play_all(&mut st, &["B9", "A1", "A8"]);
        // 백 A9는 자충수
        assert!(st.play_coord("A9").is_err());
        assert!(st.illegal_points().contains(&"A9".to_string()));
    }

    #[test]
    fn ko_rule() {
        let mut st = GoState::new(9, 6.5).unwrap();
        // 패 모양 만들기: 흑 D5 C4 D3, 백 E5 F4 E3 E4?  표준 패 모양
        play_all(&mut st, &["D5", "E5", "C4", "F4", "D3", "E3", "A1", "D4"]);
        // 흑 E4로 백 D4를 따냄 -> 패
        let n = st.play_coord("E4").unwrap();
        assert_eq!(n, 1);
        assert_eq!(st.ko, coord::parse("D4", 9));
        // 백이 곧바로 D4로 되따내기 금지
        assert!(st.play_coord("D4").is_err());
        // 다른 곳에 두고 나면 가능
        play_all(&mut st, &["J9", "J1", "D4"]);
    }

    #[test]
    fn scoring_empty_board_and_territory() {
        let st = GoState::new(9, 6.5).unwrap();
        let s = st.score(&[]);
        assert_eq!(s.black, 0.0);
        assert_eq!(s.winner, "W");

        let mut st = GoState::new(5, 0.5).unwrap();
        // 흑이 C열 전체를 차지 -> 판을 반으로 가름
        play_all(&mut st, &["C1", "D1", "C2", "D2", "C3", "D3", "C4", "D4", "C5", "D5"]);
        let s = st.score(&[]);
        assert_eq!(s.black, 15.0); // A,B,C 열
        assert_eq!(s.white, 10.5); // D,E 열 + 덤
        assert_eq!(s.winner, "B");
    }

    #[test]
    fn fallback_is_legal() {
        let mut st = GoState::new(9, 6.5).unwrap();
        for _ in 0..60 {
            let mv = st.fallback_move();
            st.play_coord(&mv).unwrap();
        }
    }
}
