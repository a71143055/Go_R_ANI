//! 줄 잇기 게임 규칙: 오목 · 펜테(따내기 오목) · 육목(Connect6).
//!
//! | 변형 | 판 | 승리 조건 | 특이 규칙 |
//! |---|---|---|---|
//! | 오목 | 15 | 5목 이상 | 없음 (자유 룰) |
//! | 펜테 | 19 | 5목 이상 또는 돌 10개(5쌍) 따내기 | 상대 돌 2개를 양쪽에서 막으면 따냄 |
//! | 육목 | 19 | 6목 이상 | 흑 첫 수만 1개, 이후 매 차례 2개씩 |

use crate::coord;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    Omok,
    Pente,
    Connect6,
}

impl Variant {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "omok" => Ok(Variant::Omok),
            "pente" => Ok(Variant::Pente),
            "connect6" => Ok(Variant::Connect6),
            _ => Err(format!("알 수 없는 게임: {s}")),
        }
    }
    pub fn win_len(self) -> usize {
        if self == Variant::Connect6 { 6 } else { 5 }
    }
    pub fn default_size(self) -> usize {
        if self == Variant::Omok { 15 } else { 19 }
    }
    pub fn name(self) -> &'static str {
        match self {
            Variant::Omok => "오목",
            Variant::Pente => "펜테",
            Variant::Connect6 => "육목",
        }
    }
}

/// 펜테에서 이기는 데 필요한 따낸 돌 수
pub const PENTE_CAPTURE_WIN: u32 = 10;

pub const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LineState {
    pub variant: Variant,
    pub size: usize,
    pub board: Vec<u8>,
    /// 1 흑, 2 백
    pub to_play: u8,
    /// "B H8" 형식, 돌 하나당 한 줄
    pub history: Vec<String>,
    /// 가장 최근 차례에 놓인 돌들
    pub last: Vec<usize>,
    /// 이번 차례에 이미 놓은 돌 수 (육목)
    pub placed_in_turn: u8,
    /// 1 흑 승, 2 백 승, 3 무승부
    pub winner: Option<u8>,
    pub win_line: Vec<usize>,
    /// [흑이 따낸 돌 수, 백이 따낸 돌 수] (펜테)
    pub captures: [u32; 2],
}

impl LineState {
    pub fn new(variant: Variant, size: Option<usize>) -> Result<Self, String> {
        let size = size.unwrap_or(variant.default_size());
        if !(9..=19).contains(&size) {
            return Err("판 크기는 9~19여야 합니다".into());
        }
        Ok(LineState {
            variant,
            size,
            board: vec![0; size * size],
            to_play: 1,
            history: vec![],
            last: vec![],
            placed_in_turn: 0,
            winner: None,
            win_line: vec![],
            captures: [0, 0],
        })
    }

    /// 이번 차례에 놓아야 하는 돌 수
    pub fn quota(&self) -> u8 {
        let stones_before_turn = self.history.len() - self.placed_in_turn as usize;
        if self.variant == Variant::Connect6 && stones_before_turn > 0 { 2 } else { 1 }
    }

    /// 이번 차례에 더 놓아야 하는 돌 수
    pub fn remaining_in_turn(&self) -> u8 {
        self.quota() - self.placed_in_turn
    }

    pub fn at(&self, x: i32, y: i32) -> Option<u8> {
        let n = self.size as i32;
        if x < 0 || y < 0 || x >= n || y >= n {
            None
        } else {
            Some(self.board[(y * n + x) as usize])
        }
    }

    /// p를 지나 (dx,dy) 방향으로 이어진 color 돌 (p 포함)
    pub fn line_through(&self, p: usize, color: u8, (dx, dy): (i32, i32)) -> Vec<usize> {
        let n = self.size as i32;
        let (x0, y0) = ((p % self.size) as i32, (p / self.size) as i32);
        let mut line = vec![p];
        for sign in [1, -1] {
            let (mut x, mut y) = (x0 + dx * sign, y0 + dy * sign);
            while self.at(x, y) == Some(color) {
                line.push((y * n + x) as usize);
                x += dx * sign;
                y += dy * sign;
            }
        }
        line
    }

    /// 펜테: p에 color를 놓았을 때 따내지는 돌들 (X O O X)
    pub fn pente_captures(&self, p: usize, color: u8) -> Vec<usize> {
        let n = self.size as i32;
        let opp = 3 - color;
        let (x0, y0) = ((p % self.size) as i32, (p / self.size) as i32);
        let mut out = vec![];
        for (dx, dy) in DIRS {
            for sign in [1, -1] {
                let (dx, dy) = (dx * sign, dy * sign);
                if self.at(x0 + dx, y0 + dy) == Some(opp)
                    && self.at(x0 + 2 * dx, y0 + 2 * dy) == Some(opp)
                    && self.at(x0 + 3 * dx, y0 + 3 * dy) == Some(color)
                {
                    out.push(((y0 + dy) * n + x0 + dx) as usize);
                    out.push(((y0 + 2 * dy) * n + x0 + 2 * dx) as usize);
                }
            }
        }
        out
    }

    /// 돌 하나를 놓는다. 따낸 돌들의 위치를 돌려준다.
    pub fn play_coord(&mut self, mv: &str) -> Result<Vec<usize>, String> {
        if self.winner.is_some() {
            return Err("이미 끝난 게임입니다".into());
        }
        let p = coord::parse(mv, self.size).ok_or_else(|| format!("잘못된 좌표: {mv}"))?;
        if self.board[p] != 0 {
            return Err(format!("{mv}: 이미 돌이 있는 자리입니다"));
        }
        let color = self.to_play;
        if self.placed_in_turn == 0 {
            self.last.clear();
        }
        let captured = if self.variant == Variant::Pente { self.pente_captures(p, color) } else { vec![] };
        self.board[p] = color;
        for &c in &captured {
            self.board[c] = 0;
        }
        self.captures[(color - 1) as usize] += captured.len() as u32;
        self.history.push(format!("{} {}", if color == 1 { 'B' } else { 'W' }, coord::format(p, self.size)));
        self.last.push(p);

        // 승리 판정
        for d in DIRS {
            let line = self.line_through(p, color, d);
            if line.len() >= self.variant.win_len() {
                self.winner = Some(color);
                self.win_line = line;
                return Ok(captured);
            }
        }
        if self.variant == Variant::Pente && self.captures[(color - 1) as usize] >= PENTE_CAPTURE_WIN {
            self.winner = Some(color);
            return Ok(captured);
        }
        if self.board.iter().all(|&c| c != 0) {
            self.winner = Some(3);
            return Ok(captured);
        }
        self.placed_in_turn += 1;
        if self.placed_in_turn >= self.quota() {
            self.placed_in_turn = 0;
            self.to_play = 3 - color;
        }
        Ok(captured)
    }

    pub fn ascii(&self) -> String {
        coord::ascii(&self.board, self.size)
    }

    /// 돌 하나로 승리 줄이 완성되는 자리(양쪽). 화면을 보지 않는 사용자에게 경고로 알려 준다.
    pub fn urgent_points(&self) -> Vec<String> {
        let mut out = vec![];
        for i in 0..self.board.len() {
            if self.board[i] != 0 {
                continue;
            }
            for color in [1u8, 2] {
                if DIRS.iter().any(|&d| self.line_through(i, color, d).len() >= self.variant.win_len()) {
                    let who = if color == self.to_play { "내가 두면 승리" } else { "상대 승리 저지 필요" };
                    out.push(format!("{who}: {}", coord::format(i, self.size)));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(st: &mut LineState, moves: &[&str]) {
        for m in moves {
            st.play_coord(m).unwrap_or_else(|e| panic!("{m}: {e}"));
        }
    }

    #[test]
    fn omok_five_wins() {
        let mut st = LineState::new(Variant::Omok, None).unwrap();
        play(&mut st, &["A1", "A2", "B1", "B2", "C1", "C2", "D1", "D2", "E1"]);
        assert_eq!(st.winner, Some(1));
        assert_eq!(st.win_line.len(), 5);
        assert!(st.play_coord("F5").is_err());
    }

    #[test]
    fn urgent_points_found() {
        let mut st = LineState::new(Variant::Omok, None).unwrap();
        play(&mut st, &["H8", "A1", "J8", "A3", "K8", "A5", "L8"]);
        assert_eq!(st.urgent_points().len(), 2);
    }

    #[test]
    fn pente_capture_pair() {
        let mut st = LineState::new(Variant::Pente, None).unwrap();
        // 흑 K10, 백 L10 M10, 흑 N10 → 백 두 점 따냄
        play(&mut st, &["K10", "L10", "A1", "M10"]);
        let cap = st.play_coord("N10").unwrap();
        assert_eq!(cap.len(), 2);
        assert_eq!(st.captures, [2, 0]);
        assert_eq!(st.at(11, 9), Some(0)); // L10 비었음
    }

    #[test]
    fn pente_moving_into_flank_is_safe() {
        let mut st = LineState::new(Variant::Pente, None).unwrap();
        // 흑 K10 N10, 백이 L10 M10에 들어가도 따내지지 않음
        play(&mut st, &["K10", "L10", "N10"]);
        let cap = st.play_coord("M10").unwrap();
        assert!(cap.is_empty());
        assert_eq!(st.captures, [0, 0]);
    }

    #[test]
    fn pente_wins_by_ten_captures() {
        let mut st = LineState::new(Variant::Pente, None).unwrap();
        st.captures = [8, 0];
        play(&mut st, &["K10", "L10", "A1", "M10"]);
        st.play_coord("N10").unwrap();
        assert_eq!(st.winner, Some(1));
    }

    #[test]
    fn connect6_turn_quota() {
        let mut st = LineState::new(Variant::Connect6, None).unwrap();
        assert_eq!(st.remaining_in_turn(), 1);
        play(&mut st, &["K10"]);
        assert_eq!(st.to_play, 2);
        assert_eq!(st.remaining_in_turn(), 2);
        play(&mut st, &["A1"]);
        assert_eq!(st.to_play, 2);
        assert_eq!(st.remaining_in_turn(), 1);
        play(&mut st, &["A2"]);
        assert_eq!(st.to_play, 1);
        assert_eq!(st.last.len(), 2);
    }

    #[test]
    fn connect6_six_wins_five_does_not() {
        let mut st = LineState::new(Variant::Connect6, None).unwrap();
        // 흑: K10 / L10 M10 / N10 O10 / P10 Q10 (중간에 백 2개씩)
        play(&mut st, &["K10", "A1", "A2", "L10", "M10", "A3", "A4", "N10", "O10"]);
        assert_eq!(st.winner, None); // 5목은 아직
        play(&mut st, &["A5", "A6", "P10"]);
        assert_eq!(st.winner, Some(1));
    }
}
