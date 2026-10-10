//! 오목 AI: 5칸 창(window) 평가 + 알파-베타 반복 심화 탐색.
//!
//! 판의 모든 "가로·세로·대각선 5칸 창"마다 흑/백 돌 수를 세어 두고, 한쪽 색만 있는 창에
//! 돌 수에 따른 점수를 준다. 착수 시 그 점을 지나는 창(최대 20개)만 갱신하므로 빠르다.

use crate::coord;
use crate::omok::{OmokState, SIZE};
use serde::Serialize;
use std::time::{Duration, Instant};

const N: usize = SIZE;
const VAL: [i64; 6] = [0, 1, 12, 160, 3_000, 10_000_000];
const WIN: i64 = 1_000_000_000;

struct Search {
    board: Vec<u8>,
    pt_win: Vec<Vec<usize>>,
    cnt: Vec<[u8; 3]>,
    /// 흑 기준 평가값
    score: i64,
    won: bool,
    nodes: u64,
    deadline: Instant,
    stop: bool,
}

fn wval(c: [u8; 3]) -> i64 {
    match (c[1], c[2]) {
        (b, 0) => VAL[b as usize],
        (0, w) => -VAL[w as usize],
        _ => 0,
    }
}

impl Search {
    fn new(st: &OmokState, ms: u64) -> Self {
        let mut windows = vec![];
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
                    let (ex, ey) = (x + dx * 4, y + dy * 4);
                    if ex < 0 || ey < 0 || ex >= N as i32 || ey >= N as i32 {
                        continue;
                    }
                    let mut w = [0usize; 5];
                    for (k, cell) in w.iter_mut().enumerate() {
                        *cell = ((y + dy * k as i32) * N as i32 + x + dx * k as i32) as usize;
                    }
                    windows.push(w);
                }
            }
        }
        let mut pt_win = vec![vec![]; N * N];
        for (i, w) in windows.iter().enumerate() {
            for &p in w {
                pt_win[p].push(i);
            }
        }
        let mut s = Search {
            board: vec![0; N * N],
            cnt: vec![[0; 3]; windows.len()],
            pt_win,
            score: 0,
            won: false,
            nodes: 0,
            deadline: Instant::now() + Duration::from_millis(ms),
            stop: false,
        };
        for (p, &c) in st.board.iter().enumerate() {
            if c != 0 {
                s.place(p, c);
            }
        }
        s.won = false;
        s
    }

    fn place(&mut self, p: usize, c: u8) {
        self.board[p] = c;
        for &wi in &self.pt_win[p] {
            self.score -= wval(self.cnt[wi]);
            self.cnt[wi][c as usize] += 1;
            self.score += wval(self.cnt[wi]);
            if self.cnt[wi][c as usize] == 5 {
                self.won = true;
            }
        }
    }

    fn unplace(&mut self, p: usize, c: u8) {
        self.board[p] = 0;
        for &wi in &self.pt_win[p] {
            self.score -= wval(self.cnt[wi]);
            self.cnt[wi][c as usize] -= 1;
            self.score += wval(self.cnt[wi]);
        }
        self.won = false;
    }

    /// p에 c를 두면 늘어나는 c의 창 점수
    fn gain(&self, p: usize, c: u8) -> i64 {
        let o = (3 - c) as usize;
        self.pt_win[p]
            .iter()
            .map(|&wi| {
                let k = self.cnt[wi];
                if k[o] == 0 { VAL[k[c as usize] as usize + 1] - VAL[k[c as usize] as usize] } else { 0 }
            })
            .sum()
    }

    /// p를 지나는 창 중 (상대 돌 없는) c의 최대 돌 수
    fn max_line(&self, p: usize, c: u8) -> u8 {
        let o = (3 - c) as usize;
        self.pt_win[p]
            .iter()
            .filter(|&&wi| self.cnt[wi][o] == 0)
            .map(|&wi| self.cnt[wi][c as usize])
            .max()
            .unwrap_or(0)
    }

    fn near_stone(&self, p: usize) -> bool {
        let (x0, y0) = ((p % N) as i32, (p / N) as i32);
        for dy in -2..=2 {
            for dx in -2..=2 {
                let (x, y) = (x0 + dx, y0 + dy);
                if x >= 0 && y >= 0 && x < N as i32 && y < N as i32 && self.board[y as usize * N + x as usize] != 0 {
                    return true;
                }
            }
        }
        false
    }

    /// 공격 + 수비 가치로 정렬한 후보 수
    fn candidates(&self, side: u8, limit: usize) -> Vec<usize> {
        let mut v: Vec<(i64, usize)> = (0..N * N)
            .filter(|&p| self.board[p] == 0 && self.near_stone(p))
            .map(|p| (self.gain(p, side) * 11 / 10 + self.gain(p, 3 - side), p))
            .collect();
        v.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        // 상대의 5목 위협이 있으면 막는 수만 본다
        if v.first().is_some_and(|f| f.0 >= VAL[5] - VAL[4]) {
            let must: Vec<(i64, usize)> = v.iter().copied().filter(|e| e.0 >= VAL[5] - VAL[4]).collect();
            return must.into_iter().take(limit).map(|e| e.1).collect();
        }
        v.truncate(limit);
        v.into_iter().map(|e| e.1).collect()
    }

    fn eval(&self, side: u8) -> i64 {
        if side == 1 { self.score } else { -self.score }
    }

    fn negamax(&mut self, depth: u32, mut alpha: i64, beta: i64, side: u8) -> i64 {
        self.nodes += 1;
        if self.nodes & 1023 == 0 && Instant::now() >= self.deadline {
            self.stop = true;
        }
        if depth == 0 || self.stop {
            return self.eval(side);
        }
        let cands = self.candidates(side, if depth >= 4 { 9 } else { 12 });
        if cands.is_empty() {
            return 0;
        }
        let mut best = -WIN * 2;
        for p in cands {
            self.place(p, side);
            let v = if self.won { WIN + depth as i64 } else { -self.negamax(depth - 1, -beta, -alpha, 3 - side) };
            self.unplace(p, side);
            if v > best {
                best = v;
            }
            if best > alpha {
                alpha = best;
            }
            if alpha >= beta || self.stop {
                break;
            }
        }
        best
    }
}

#[derive(Serialize, Debug)]
pub struct OmokMove {
    #[serde(rename = "move")]
    pub mv: String,
    pub depth: u32,
    pub nodes: u64,
    pub comment: String,
}

pub fn genmove(st: &OmokState, ms: u64) -> OmokMove {
    let side = st.to_play;
    let center = 7 * N + 7;
    if st.board.iter().all(|&c| c == 0) {
        return OmokMove { mv: coord::format(center, N), depth: 0, nodes: 0, comment: "중앙에서 시작합니다.".into() };
    }
    let mut s = Search::new(st, ms.clamp(50, 30_000));
    let root = s.candidates(side, 16);
    let mut best = root[0];
    let mut best_score = 0;
    let mut done_depth = 0;
    for depth in (2..=12).step_by(2) {
        let mut alpha = -WIN * 2;
        let mut iter_best = None;
        // 이전 반복의 최선 수를 먼저 탐색
        let mut order = root.clone();
        if let Some(i) = order.iter().position(|&p| p == best) {
            order.swap(0, i);
        }
        for &p in &order {
            s.place(p, side);
            let v = if s.won { WIN + depth as i64 } else { -s.negamax(depth - 1, -WIN * 2, -alpha, 3 - side) };
            s.unplace(p, side);
            if s.stop {
                break;
            }
            if v > alpha {
                alpha = v;
                iter_best = Some((p, v));
            }
        }
        if s.stop {
            break;
        }
        if let Some((p, v)) = iter_best {
            best = p;
            best_score = v;
            done_depth = depth;
        }
        if best_score.abs() >= WIN {
            break; // 승패가 결정됨
        }
    }

    // 해설
    let own = s.max_line(best, side) + 1;
    let opp = s.max_line(best, 3 - side);
    let mut comment = if own >= 5 {
        "5목을 완성합니다!".to_string()
    } else if opp >= 4 {
        "상대의 4를 막습니다.".into()
    } else if own == 4 {
        "4를 만들어 압박합니다.".into()
    } else if opp == 3 {
        "상대의 3이 열리기 전에 막습니다.".into()
    } else if own == 3 {
        "3을 만들어 공격을 시작합니다.".into()
    } else {
        "다음 공격을 위해 자리를 잡습니다.".into()
    };
    if own < 5 && best_score >= WIN {
        comment.push_str(" 이제 필승 수순이 보입니다.");
    } else if best_score <= -WIN {
        comment.push_str(" 형세가 매우 불리합니다.");
    }
    OmokMove { mv: coord::format(best, N), depth: done_depth, nodes: s.nodes, comment }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(moves: &[&str]) -> OmokState {
        let mut s = OmokState::new();
        for m in moves {
            s.play_coord(m).unwrap();
        }
        s
    }

    #[test]
    fn takes_win() {
        let s = st(&["H8", "A1", "J8", "A3", "K8", "A5", "L8", "A7"]);
        let m = genmove(&s, 300);
        assert!(m.mv == "G8" || m.mv == "M8", "{m:?}");
        assert!(m.comment.contains("5목"));
    }

    #[test]
    fn blocks_four() {
        let s = st(&["H8", "A1", "J8", "A3", "K8", "A15", "L8"]);
        let m = genmove(&s, 300);
        assert!(m.mv == "G8" || m.mv == "M8", "{m:?}");
    }

    #[test]
    #[ignore]
    fn bench() {
        let s = st(&["H8", "J9", "J8", "G8", "K7"]);
        let m = genmove(&s, 1000);
        println!("omok 1s: {m:?}");
    }

    #[test]
    fn blocks_open_three() {
        // 흑 열린 3 (H8 J8 K8), 백은 막아야 함
        let s = st(&["H8", "A1", "J8", "A15", "K8"]);
        let m = genmove(&s, 500);
        assert!(["G8", "L8", "F8", "M8"].contains(&m.mv.as_str()), "{m:?}");
    }
}
