//! 줄 잇기 게임 AI (오목·펜테·육목): 창(window) 평가 + 알파-베타 반복 심화 탐색.
//!
//! 판의 모든 "가로·세로·대각선 L칸 창"(L = 승리 줄 길이)마다 흑/백 돌 수를 세어 두고,
//! 한쪽 색만 있는 창에 돌 수에 따른 점수를 준다. 착수·따내기 시 그 점을 지나는 창만 갱신한다.
//! 육목처럼 한 차례에 돌을 2개 놓는 경우, 같은 편이 이어서 두는 수는 부호를 바꾸지 않고 탐색한다.

use crate::coord;
use crate::linegame::{LineState, Variant, PENTE_CAPTURE_WIN};
use serde::Serialize;
use std::time::{Duration, Instant};

const WIN: i64 = 1_000_000_000;

struct Search {
    variant: Variant,
    n: usize,
    len: usize,
    val: Vec<i64>,
    board: Vec<u8>,
    pt_win: Vec<Vec<usize>>,
    cnt: Vec<[u8; 3]>,
    /// 흑 기준 평가값 (창 점수)
    score: i64,
    captures: [u32; 2],
    won: bool,
    side: u8,
    placed: u8,
    stones: usize,
    nodes: u64,
    deadline: Instant,
    stop: bool,
}

struct Undo {
    p: usize,
    captured: Vec<usize>,
    side: u8,
    placed: u8,
}

fn cap_value(stones: u32) -> i64 {
    // 펜테: 따낸 쌍 수에 따른 가치
    match stones / 2 {
        0 => 0,
        1 => 400,
        2 => 1_000,
        3 => 3_000,
        4 => 12_000,
        _ => WIN / 2,
    }
}

impl Search {
    fn new(st: &LineState, ms: u64) -> Self {
        let n = st.size;
        let len = st.variant.win_len();
        let val: Vec<i64> = if len == 6 {
            vec![0, 1, 10, 100, 5_000, 20_000, 100_000_000]
        } else {
            vec![0, 1, 12, 160, 3_000, 10_000_000]
        };
        let mut windows: Vec<Vec<usize>> = vec![];
        for y in 0..n as i32 {
            for x in 0..n as i32 {
                for (dx, dy) in crate::linegame::DIRS {
                    let (ex, ey) = (x + dx * (len as i32 - 1), y + dy * (len as i32 - 1));
                    if ex < 0 || ey < 0 || ex >= n as i32 || ey >= n as i32 {
                        continue;
                    }
                    windows.push((0..len as i32).map(|k| ((y + dy * k) * n as i32 + x + dx * k) as usize).collect());
                }
            }
        }
        let mut pt_win = vec![vec![]; n * n];
        for (i, w) in windows.iter().enumerate() {
            for &p in w {
                pt_win[p].push(i);
            }
        }
        let mut s = Search {
            variant: st.variant,
            n,
            len,
            val,
            board: vec![0; n * n],
            cnt: vec![[0; 3]; windows.len()],
            pt_win,
            score: 0,
            captures: st.captures,
            won: false,
            side: st.to_play,
            placed: st.placed_in_turn,
            stones: st.history.len(),
            nodes: 0,
            deadline: Instant::now() + Duration::from_millis(ms),
            stop: false,
        };
        for (p, &c) in st.board.iter().enumerate() {
            if c != 0 {
                s.set(p, c);
            }
        }
        s.won = false;
        s
    }

    fn wval(&self, c: [u8; 3]) -> i64 {
        match (c[1], c[2]) {
            (b, 0) => self.val[b as usize],
            (0, w) => -self.val[w as usize],
            _ => 0,
        }
    }

    fn set(&mut self, p: usize, c: u8) {
        self.board[p] = c;
        for i in 0..self.pt_win[p].len() {
            let wi = self.pt_win[p][i];
            self.score -= self.wval(self.cnt[wi]);
            self.cnt[wi][c as usize] += 1;
            self.score += self.wval(self.cnt[wi]);
            if self.cnt[wi][c as usize] as usize == self.len {
                self.won = true;
            }
        }
    }

    fn clear(&mut self, p: usize) {
        let c = self.board[p];
        self.board[p] = 0;
        for i in 0..self.pt_win[p].len() {
            let wi = self.pt_win[p][i];
            self.score -= self.wval(self.cnt[wi]);
            self.cnt[wi][c as usize] -= 1;
            self.score += self.wval(self.cnt[wi]);
        }
    }

    fn at(&self, x: i32, y: i32) -> Option<u8> {
        let n = self.n as i32;
        if x < 0 || y < 0 || x >= n || y >= n { None } else { Some(self.board[(y * n + x) as usize]) }
    }

    fn pente_captures(&self, p: usize, c: u8) -> Vec<usize> {
        let n = self.n as i32;
        let (x0, y0) = ((p % self.n) as i32, (p / self.n) as i32);
        let mut out = vec![];
        for (dx, dy) in crate::linegame::DIRS {
            for sign in [1, -1] {
                let (dx, dy) = (dx * sign, dy * sign);
                if self.at(x0 + dx, y0 + dy) == Some(3 - c)
                    && self.at(x0 + 2 * dx, y0 + 2 * dy) == Some(3 - c)
                    && self.at(x0 + 3 * dx, y0 + 3 * dy) == Some(c)
                {
                    out.push(((y0 + dy) * n + x0 + dx) as usize);
                    out.push(((y0 + 2 * dy) * n + x0 + 2 * dx) as usize);
                }
            }
        }
        out
    }

    fn quota(&self) -> u8 {
        if self.variant == Variant::Connect6 && self.stones - self.placed as usize > 0 { 2 } else { 1 }
    }

    /// 현재 차례 편이 p에 둔다
    fn make(&mut self, p: usize) -> Undo {
        let c = self.side;
        let undo = Undo { p, captured: vec![], side: self.side, placed: self.placed };
        let captured = if self.variant == Variant::Pente { self.pente_captures(p, c) } else { vec![] };
        self.set(p, c);
        for &q in &captured {
            self.clear(q);
        }
        self.captures[(c - 1) as usize] += captured.len() as u32;
        if self.variant == Variant::Pente && self.captures[(c - 1) as usize] >= PENTE_CAPTURE_WIN {
            self.won = true;
        }
        self.stones += 1;
        self.placed += 1;
        if self.placed >= self.quota() {
            self.placed = 0;
            self.side = 3 - c;
        }
        Undo { captured, ..undo }
    }

    fn unmake(&mut self, u: Undo) {
        let c = u.side;
        self.clear(u.p);
        for &q in &u.captured {
            self.set(q, 3 - c);
        }
        self.captures[(c - 1) as usize] -= u.captured.len() as u32;
        self.stones -= 1;
        self.side = u.side;
        self.placed = u.placed;
        self.won = false;
    }

    /// p에 c를 두면 늘어나는 c의 창 점수
    fn gain(&self, p: usize, c: u8) -> i64 {
        let o = (3 - c) as usize;
        self.pt_win[p]
            .iter()
            .map(|&wi| {
                let k = self.cnt[wi];
                if k[o] == 0 { self.val[k[c as usize] as usize + 1] - self.val[k[c as usize] as usize] } else { 0 }
            })
            .sum()
    }

    fn max_line(&self, p: usize, c: u8) -> u8 {
        let o = (3 - c) as usize;
        self.pt_win[p].iter().filter(|&&wi| self.cnt[wi][o] == 0).map(|&wi| self.cnt[wi][c as usize]).max().unwrap_or(0)
    }

    fn near_stone(&self, p: usize) -> bool {
        let (x0, y0) = ((p % self.n) as i32, (p / self.n) as i32);
        (-2..=2).any(|dy| (-2..=2).any(|dx| matches!(self.at(x0 + dx, y0 + dy), Some(c) if c != 0)))
    }

    fn candidates(&self, limit: usize) -> Vec<usize> {
        let side = self.side;
        let mut v: Vec<(i64, usize)> = (0..self.n * self.n)
            .filter(|&p| self.board[p] == 0 && self.near_stone(p))
            .map(|p| {
                let mut s = self.gain(p, side) * 11 / 10 + self.gain(p, 3 - side);
                if self.variant == Variant::Pente {
                    let caps = self.pente_captures(p, side).len() as u32;
                    if caps > 0 {
                        let mine = self.captures[(side - 1) as usize];
                        s += cap_value(mine + caps) - cap_value(mine) + 300 * caps as i64;
                    }
                    // 상대가 이 자리에서 따낼 수 있으면 미리 막는 가치
                    let theirs = self.pente_captures(p, 3 - side).len() as u32;
                    if theirs > 0 {
                        let t = self.captures[(2 - side) as usize];
                        s += (cap_value(t + theirs) - cap_value(t)) / 2;
                    }
                }
                (s, p)
            })
            .collect();
        v.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        // 오목: 상대의 5목 위협이 있으면 막는 수만 본다 (펜테는 따내기로도 막을 수 있어 제외)
        if self.variant == Variant::Omok {
            let must = self.val[5] - self.val[4];
            if v.first().is_some_and(|f| f.0 >= must) {
                v.retain(|e| e.0 >= must);
            }
        }
        v.truncate(limit);
        v.into_iter().map(|e| e.1).collect()
    }

    fn eval(&self) -> i64 {
        let mut s = self.score;
        if self.variant == Variant::Pente {
            s += cap_value(self.captures[0]) - cap_value(self.captures[1]);
        }
        if self.side == 1 { s } else { -s }
    }

    /// 현재 차례 편(side) 관점의 값
    fn negamax(&mut self, depth: u32, mut alpha: i64, beta: i64) -> i64 {
        self.nodes += 1;
        if self.nodes & 1023 == 0 && Instant::now() >= self.deadline {
            self.stop = true;
        }
        if depth == 0 || self.stop {
            return self.eval();
        }
        let width = if self.variant == Variant::Connect6 { if depth >= 3 { 7 } else { 10 } } else if depth >= 4 { 9 } else { 12 };
        let cands = self.candidates(width);
        if cands.is_empty() {
            return 0;
        }
        let mut best = -WIN * 2;
        for p in cands {
            let me = self.side;
            let u = self.make(p);
            let v = if self.won {
                WIN + depth as i64
            } else if self.side == me {
                self.negamax(depth - 1, alpha, beta) // 같은 편이 한 수 더 (육목)
            } else {
                -self.negamax(depth - 1, -beta, -alpha)
            };
            self.unmake(u);
            best = best.max(v);
            alpha = alpha.max(best);
            if alpha >= beta || self.stop {
                break;
            }
        }
        best
    }
}

#[derive(Serialize, Debug)]
pub struct LineMove {
    /// 이번 차례에 둘 돌들 (육목은 최대 2개)
    pub moves: Vec<String>,
    pub depth: u32,
    pub nodes: u64,
    pub comment: String,
}

/// 남은 차례 몫의 돌(1~2개)을 고른다
pub fn genmove(st: &LineState, ms: u64) -> LineMove {
    let n = st.size;
    let center = (n / 2) * n + n / 2;
    if st.board.iter().all(|&c| c == 0) {
        return LineMove { moves: vec![coord::format(center, n)], depth: 0, nodes: 0, comment: "중앙에서 시작합니다.".into() };
    }
    let mut work = st.clone();
    let mut moves = vec![];
    let mut comments = vec![];
    let mut total_nodes = 0;
    let mut depth_done = 0;
    let remaining = st.remaining_in_turn();
    for k in 0..remaining {
        // 차례 시간을 남은 돌 수로 나눠 쓴다
        let budget = ms / (remaining - k) as u64;
        let (p, d, nodes, comment) = best_single(&work, budget.max(30));
        total_nodes += nodes;
        depth_done = depth_done.max(d);
        let mv = coord::format(p, n);
        let captured = work.play_coord(&mv).map(|c| c.len()).unwrap_or(0);
        moves.push(mv);
        comments.push(if captured > 0 { format!("돌 {captured}개를 따냅니다. {comment}") } else { comment });
        if work.winner.is_some() {
            break;
        }
    }
    comments.dedup();
    LineMove { moves, depth: depth_done, nodes: total_nodes, comment: comments.join(" ") }
}

fn best_single(st: &LineState, ms: u64) -> (usize, u32, u64, String) {
    let mut s = Search::new(st, ms.clamp(30, 30_000));
    let side = s.side;
    let root = s.candidates(16);
    if root.is_empty() {
        let p = st.board.iter().position(|&c| c == 0).unwrap_or(0);
        return (p, 0, 0, "둘 곳을 찾습니다.".into());
    }
    let mut best = root[0];
    let mut best_score = 0;
    let mut done = 0;
    let max_depth = if st.variant == Variant::Connect6 { 9 } else { 12 };
    let mut depth = 2;
    while depth <= max_depth {
        let mut alpha = -WIN * 2;
        let mut iter_best = None;
        let mut order = root.clone();
        if let Some(i) = order.iter().position(|&p| p == best) {
            order.swap(0, i);
        }
        for &p in &order {
            let u = s.make(p);
            let v = if s.won {
                WIN + depth as i64
            } else if s.side == side {
                s.negamax(depth - 1, alpha, WIN * 2)
            } else {
                -s.negamax(depth - 1, -WIN * 2, -alpha)
            };
            s.unmake(u);
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
            done = depth;
        }
        if best_score.abs() >= WIN {
            break;
        }
        depth += if st.variant == Variant::Connect6 { 1 } else { 2 };
    }

    // 해설
    let len = s.len as u8;
    let own = s.max_line(best, side) + 1;
    let opp = s.max_line(best, 3 - side);
    let mut comment = if own >= len {
        format!("{len}목을 완성합니다!")
    } else if opp + 1 >= len {
        format!("상대의 {opp}을 막습니다.")
    } else if own + 1 >= len {
        format!("{own}을 만들어 압박합니다.")
    } else if opp + 2 >= len {
        format!("상대의 {opp}이 커지기 전에 막습니다.")
    } else if own + 2 >= len {
        format!("{own}을 만들어 공격을 시작합니다.")
    } else {
        "다음 공격을 위해 자리를 잡습니다.".to_string()
    };
    if own < len && best_score >= WIN {
        comment.push_str(" 이제 필승 수순이 보입니다.");
    } else if best_score <= -WIN {
        comment.push_str(" 형세가 매우 불리합니다.");
    }
    (best, done, s.nodes, comment)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(v: Variant, moves: &[&str]) -> LineState {
        let mut s = LineState::new(v, None).unwrap();
        for m in moves {
            s.play_coord(m).unwrap();
        }
        s
    }

    #[test]
    fn omok_takes_win() {
        let s = st(Variant::Omok, &["H8", "A1", "J8", "A3", "K8", "A5", "L8", "A7"]);
        let m = genmove(&s, 300);
        assert!(m.moves == ["G8"] || m.moves == ["M8"], "{m:?}");
        assert!(m.comment.contains("5목"));
    }

    #[test]
    fn omok_blocks_four() {
        let s = st(Variant::Omok, &["H8", "A1", "J8", "A3", "K8", "A15", "L8"]);
        let m = genmove(&s, 300);
        assert!(m.moves == ["G8"] || m.moves == ["M8"], "{m:?}");
    }

    #[test]
    fn omok_blocks_open_three() {
        let s = st(Variant::Omok, &["H8", "A1", "J8", "A15", "K8"]);
        let m = genmove(&s, 500);
        assert!(["G8", "L8", "F8", "M8"].contains(&m.moves[0].as_str()), "{m:?}");
    }

    #[test]
    fn pente_takes_capture_win() {
        // 흑이 이미 8개 따냄, 다음 따내기로 승리
        let mut s = st(Variant::Pente, &["K10", "L10", "A1", "M10"]);
        s.captures = [8, 0];
        let m = genmove(&s, 300);
        assert_eq!(m.moves, ["N10"], "{m:?}");
    }

    #[test]
    fn connect6_plays_two_and_wins() {
        // 흑 4개 연속 (K10~N10), 흑 차례에 2개를 놓으면 6목
        let s = st(Variant::Connect6, &["K10", "A1", "A3", "L10", "M10", "A5", "A7", "N10", "B19"]);
        // 위 수순: 흑 K10 / 백 A1 A3 / 흑 L10 M10 / 백 A5 A7 / 흑 N10 B19 → 이제 백 차례
        let mut s = s;
        s.play_coord("T1").unwrap();
        s.play_coord("T3").unwrap(); // 백이 엉뚱한 곳 → 흑 차례
        let m = genmove(&s, 500);
        assert_eq!(m.moves.len(), 2, "{m:?}");
        let mut after = s.clone();
        for mv in &m.moves {
            after.play_coord(mv).unwrap();
        }
        assert_eq!(after.winner, Some(1), "{m:?}");
    }

    #[test]
    fn connect6_blocks_four() {
        // 백 차례, 흑 4개 연속 (K10~N10) — 막지 않으면 흑이 다음 차례 6목
        let s = st(Variant::Connect6, &["K10", "A1", "A3", "L10", "M10", "A5", "A7", "N10", "O10"]);
        let m = genmove(&s, 800);
        let blocks = ["J10", "P10", "H10", "Q10"];
        assert!(m.moves.iter().any(|mv| blocks.contains(&mv.as_str())), "{m:?}");
    }
}
