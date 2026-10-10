//! 오목(자유 룰): 15x15, 가로·세로·대각선 5목 이상이면 승리.

use crate::coord;
use serde::{Deserialize, Serialize};

pub const SIZE: usize = 15;
const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OmokState {
    pub size: usize,
    pub board: Vec<u8>,
    pub to_play: u8,
    pub history: Vec<String>,
    pub last: Option<usize>,
    /// 1 흑 승, 2 백 승, 3 무승부
    pub winner: Option<u8>,
    pub win_line: Vec<usize>,
}

impl Default for OmokState {
    fn default() -> Self {
        Self::new()
    }
}

impl OmokState {
    pub fn new() -> Self {
        OmokState {
            size: SIZE,
            board: vec![0; SIZE * SIZE],
            to_play: 1,
            history: vec![],
            last: None,
            winner: None,
            win_line: vec![],
        }
    }

    fn at(&self, x: i32, y: i32) -> Option<u8> {
        if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
            None
        } else {
            Some(self.board[y as usize * SIZE + x as usize])
        }
    }

    /// idx에 color 돌이 있다고 보고, 방향 (dx,dy)로 이어진 돌 목록
    fn line_through(&self, idx: usize, color: u8, (dx, dy): (i32, i32)) -> Vec<usize> {
        let (x0, y0) = ((idx % SIZE) as i32, (idx / SIZE) as i32);
        let mut line = vec![idx];
        for sign in [1, -1] {
            let (mut x, mut y) = (x0 + dx * sign, y0 + dy * sign);
            while self.at(x, y) == Some(color) {
                line.push(y as usize * SIZE + x as usize);
                x += dx * sign;
                y += dy * sign;
            }
        }
        line
    }

    pub fn play_coord(&mut self, mv: &str) -> Result<(), String> {
        if self.winner.is_some() {
            return Err("이미 끝난 게임입니다".into());
        }
        let idx = coord::parse(mv, SIZE).ok_or_else(|| format!("잘못된 좌표: {mv}"))?;
        if self.board[idx] != 0 {
            return Err(format!("{mv}: 이미 돌이 있는 자리입니다"));
        }
        let color = self.to_play;
        self.board[idx] = color;
        self.last = Some(idx);
        self.history.push(format!("{} {}", if color == 1 { 'B' } else { 'W' }, coord::format(idx, SIZE)));
        for d in DIRS {
            let line = self.line_through(idx, color, d);
            if line.len() >= 5 {
                self.winner = Some(color);
                self.win_line = line;
                return Ok(());
            }
        }
        if self.board.iter().all(|&c| c != 0) {
            self.winner = Some(3);
        }
        self.to_play = 3 - color;
        Ok(())
    }

    pub fn ascii(&self) -> String {
        coord::ascii(&self.board, SIZE)
    }

    /// 한 방향 패턴 점수: 연속 돌 수와 열린 끝 수로 평가
    fn dir_score(&self, idx: usize, color: u8, (dx, dy): (i32, i32)) -> f64 {
        let (x0, y0) = ((idx % SIZE) as i32, (idx / SIZE) as i32);
        let mut count = 1;
        let mut open = 0;
        for sign in [1, -1] {
            let (mut x, mut y) = (x0 + dx * sign, y0 + dy * sign);
            while self.at(x, y) == Some(color) {
                count += 1;
                x += dx * sign;
                y += dy * sign;
            }
            if self.at(x, y) == Some(0) {
                open += 1;
            }
        }
        match (count, open) {
            (c, _) if c >= 5 => 1_000_000.0,
            (4, 2) => 50_000.0,
            (4, 1) => 6_000.0,
            (3, 2) => 5_000.0,
            (3, 1) => 400.0,
            (2, 2) => 300.0,
            (2, 1) => 40.0,
            (1, 2) => 10.0,
            _ => 1.0,
        }
    }

    fn point_score(&self, idx: usize, color: u8) -> f64 {
        DIRS.iter().map(|&d| self.dir_score(idx, color, d)).sum()
    }

    fn near_stone(&self, idx: usize) -> bool {
        let (x0, y0) = ((idx % SIZE) as i32, (idx / SIZE) as i32);
        (-2..=2).any(|dy| (-2..=2).any(|dx| matches!(self.at(x0 + dx, y0 + dy), Some(c) if c != 0)))
    }

    /// 다음 수로 5목이 되는 자리(양쪽 모두). Claude에게 힌트로 전달한다.
    pub fn urgent_points(&self) -> Vec<String> {
        let mut out = vec![];
        for i in 0..self.board.len() {
            if self.board[i] != 0 {
                continue;
            }
            for color in [1u8, 2] {
                if DIRS.iter().any(|&d| self.line_through(i, color, d).len() >= 5) {
                    out.push(format!("{} {}", if color == self.to_play { "내가 두면 승리:" } else { "상대 승리 저지 필요:" }, coord::format(i, SIZE)));
                }
            }
        }
        out
    }

    /// Claude 응답을 쓸 수 없을 때의 휴리스틱 대체 수
    pub fn fallback_move(&self) -> String {
        let me = self.to_play;
        let mut best: Option<(f64, usize)> = None;
        for i in 0..self.board.len() {
            if self.board[i] != 0 {
                continue;
            }
            if self.last.is_some() && !self.near_stone(i) {
                continue;
            }
            let center = {
                let (x, y) = ((i % SIZE) as f64, (i / SIZE) as f64);
                -((x - 7.0).abs() + (y - 7.0).abs()) * 0.5
            };
            let s = self.point_score(i, me) * 1.1 + self.point_score(i, 3 - me) + center + coord::hash_noise(i, self.history.len());
            if best.map_or(true, |(b, _)| s > b) {
                best = Some((s, i));
            }
        }
        best.map(|(_, i)| coord::format(i, SIZE)).unwrap_or_else(|| "H8".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_in_row_wins() {
        let mut st = OmokState::new();
        for m in ["A1", "A2", "B1", "B2", "C1", "C2", "D1", "D2", "E1"] {
            st.play_coord(m).unwrap();
        }
        assert_eq!(st.winner, Some(1));
        assert_eq!(st.win_line.len(), 5);
        assert!(st.play_coord("F5").is_err());
    }

    #[test]
    fn fallback_blocks_four() {
        let mut st = OmokState::new();
        // 흑 4개 연속 (H8~L8), 백은 엉뚱한 곳
        for m in ["H8", "A1", "J8", "A3", "K8", "A5", "L8"] {
            st.play_coord(m).unwrap();
        }
        let mv = st.fallback_move();
        assert!(mv == "G8" || mv == "M8", "got {mv}");
        assert_eq!(st.urgent_points().len(), 2);
    }
}
