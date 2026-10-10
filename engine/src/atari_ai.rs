//! 따내기 바둑(아타리 고) AI와 단수 정보.
//!
//! 규칙: 바둑 규칙 그대로 두되, 돌을 하나라도 먼저 따내는 쪽이 이긴다.
//! AI: 활로(liberty) 기반 평가 + 알파-베타 반복 심화 탐색.

use crate::coord;
use crate::go::{GoState, EMPTY};
use serde::Serialize;
use std::time::{Duration, Instant};

const WIN: i64 = 1_000_000;

/// 판 위 모든 체인: (색, 돌들, 활로들)
pub fn chains(st: &GoState) -> Vec<(u8, Vec<usize>, Vec<usize>)> {
    let mut seen = vec![false; st.board.len()];
    let mut out = vec![];
    for p in 0..st.board.len() {
        if st.board[p] == EMPTY || seen[p] {
            continue;
        }
        let c = st.board[p];
        let mut stones = vec![p];
        let mut libs = vec![];
        seen[p] = true;
        let mut i = 0;
        while i < stones.len() {
            let s = stones[i];
            i += 1;
            for q in st.neighbors(s) {
                if st.board[q] == c && !seen[q] {
                    seen[q] = true;
                    stones.push(q);
                } else if st.board[q] == EMPTY && !libs.contains(&q) {
                    libs.push(q);
                }
            }
        }
        out.push((c, stones, libs));
    }
    out
}

#[derive(Serialize, Debug)]
pub struct AtariInfo {
    /// 1 흑, 2 백
    pub color: u8,
    pub stones: Vec<String>,
    pub liberty: String,
}

/// 단수(활로 1개)에 몰린 체인들 — 화면을 보지 않는 사용자에게 경고로 알려 준다
pub fn ataris(st: &GoState) -> Vec<AtariInfo> {
    chains(st)
        .into_iter()
        .filter(|(_, _, libs)| libs.len() == 1)
        .map(|(c, stones, libs)| AtariInfo {
            color: c,
            stones: stones.iter().map(|&s| coord::format(s, st.size)).collect(),
            liberty: coord::format(libs[0], st.size),
        })
        .collect()
}

struct Search {
    deadline: Instant,
    nodes: u64,
    stop: bool,
}

/// 차례인 편 관점의 정적 평가
fn eval(st: &GoState) -> i64 {
    let me = st.to_play;
    let (mut min_me, mut min_op) = (99i64, 99i64);
    let (mut sum_me, mut sum_op) = (0i64, 0i64);
    for (c, stones, libs) in chains(st) {
        let l = libs.len() as i64;
        if c == me {
            min_me = min_me.min(l);
            sum_me += l * 2 - stones.len() as i64 / 2;
        } else {
            if l == 1 {
                return WIN / 2; // 다음 수에 따낼 수 있음
            }
            min_op = min_op.min(l);
            sum_op += l * 2 - stones.len() as i64 / 2;
        }
    }
    let danger = |m: i64| match m { 1 => -400, 2 => -60, 3 => -15, _ => 0 };
    danger(min_me) - danger(min_op) + (sum_me - sum_op)
}

/// 후보 수: 따내기 > 단수 살리기 > 단수 치기 > 활로 줄이기 > 그 외. 자충은 뒤로.
fn candidates(st: &GoState, limit: usize) -> Vec<(usize, bool)> {
    let me = st.to_play;
    let ch = chains(st);
    let mut lib_of = vec![99usize; st.board.len()];
    let mut owner = vec![0u8; st.board.len()];
    for (c, stones, libs) in &ch {
        for &s in stones {
            lib_of[s] = libs.len();
            owner[s] = *c;
        }
    }
    let n = st.size;
    let mut v: Vec<(i64, usize, bool)> = vec![];
    for p in 0..st.board.len() {
        if st.board[p] != EMPTY {
            continue;
        }
        let Ok((b, captured, _)) = st.simulate(p, me) else { continue };
        if !captured.is_empty() {
            v.push((WIN, p, true));
            continue;
        }
        let mut s = 0i64;
        for q in st.neighbors(p) {
            if st.board[q] == EMPTY {
                continue;
            }
            let l = lib_of[q] as i64;
            if owner[q] == me {
                if l == 1 { s += 500; } else if l == 2 { s += 40; }
            } else {
                s += match l { 2 => 300, 3 => 60, _ => 10 };
            }
        }
        let (_, my_libs) = st.group(&b, p);
        if my_libs == 1 {
            s -= 1000; // 자충(스스로 단수)
        }
        s += my_libs as i64 * 5;
        let (x, y) = (p % n, p / n);
        if x.min(y).min(n - 1 - x).min(n - 1 - y) == 0 {
            s -= 8;
        }
        v.push((s, p, false));
    }
    v.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    v.truncate(limit);
    v.into_iter().map(|(_, p, cap)| (p, cap)).collect()
}

impl Search {
    fn negamax(&mut self, st: &GoState, depth: u32, mut alpha: i64, beta: i64) -> i64 {
        self.nodes += 1;
        if self.nodes & 255 == 0 && Instant::now() >= self.deadline {
            self.stop = true;
        }
        if depth == 0 || self.stop {
            return eval(st);
        }
        let cands = candidates(st, if depth >= 3 { 8 } else { 12 });
        if cands.is_empty() {
            return 0;
        }
        let mut best = -WIN * 2;
        for (p, capture) in cands {
            let v = if capture {
                WIN + depth as i64
            } else {
                let mut next = st.clone();
                if next.play_coord(&coord::format(p, st.size)).is_err() {
                    continue;
                }
                -self.negamax(&next, depth - 1, -beta, -alpha)
            };
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
pub struct AtariMove {
    #[serde(rename = "move")]
    pub mv: String,
    pub depth: u32,
    pub comment: String,
}

/// level 1(하): 따낼 수 있으면 따내고, 아니면 상위 4개 후보 중 무작위 (수읽기 없음)
/// level 2(중): 3수 앞까지, level 3(상): 시간 안에서 최대한 깊게
pub fn genmove(st: &GoState, ms: u64, level: crate::coord::Level) -> AtariMove {
    let root = candidates(st, 20);
    if root.is_empty() {
        return AtariMove { mv: "pass".into(), depth: 0, comment: "둘 곳이 없어 패스합니다.".into() };
    }
    let mut best = root[0].0;
    let mut best_score = 0;
    let mut done = 0;
    if level <= 1 && !root[0].1 {
        let mut rng = crate::go_ai::Rng::new(coord::time_seed());
        best = root[rng.below(root.len().min(4))].0;
    } else if !root[0].1 {
        let max_depth = if level == 2 { 3 } else { 14 };
        let mut s = Search { deadline: Instant::now() + Duration::from_millis(ms.clamp(50, 30_000)), nodes: 0, stop: false };
        let mut depth = 1;
        while depth <= max_depth {
            let mut alpha = -WIN * 2;
            let mut iter_best = None;
            let mut order = root.clone();
            if let Some(i) = order.iter().position(|e| e.0 == best) {
                order.swap(0, i);
            }
            for &(p, _) in &order {
                let mut next = st.clone();
                if next.play_coord(&coord::format(p, st.size)).is_err() {
                    continue;
                }
                let v = -s.negamax(&next, depth - 1, -WIN * 2, -alpha);
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
            depth += 1;
        }
    }

    // 해설
    let me = st.to_play;
    let mut after = st.clone();
    let captured = after.play_coord(&coord::format(best, st.size)).map(|c| c.len()).unwrap_or(0);
    let mut comment = if captured > 0 {
        format!("돌 {captured}개를 따냅니다!")
    } else {
        let before = ataris(st);
        let now = ataris(&after);
        if before.iter().any(|a| a.color == me) && !now.iter().any(|a| a.color == me) {
            "단수에 몰린 돌을 달아나게 합니다.".into()
        } else if now.iter().any(|a| a.color != me) {
            "상대 돌을 단수로 몰아붙입니다.".into()
        } else {
            "상대 돌의 활로를 줄여 갑니다.".into()
        }
    };
    if captured == 0 && best_score >= WIN {
        comment.push_str(" 이제 따내는 수순이 보입니다.");
    } else if best_score <= -WIN {
        comment.push_str(" 형세가 매우 불리합니다.");
    }
    AtariMove { mv: coord::format(best, st.size), depth: done, comment }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(size: usize, moves: &[&str]) -> GoState {
        let mut s = GoState::new(size, 0.0).unwrap();
        for m in moves {
            s.play_coord(m).unwrap();
        }
        s
    }

    #[test]
    fn captures_when_possible() {
        let s = st(7, &["D4", "D5", "C5", "A1", "E5", "A2"]); // 백 D5 단수 (활로 D6)
        let m = genmove(&s, 300, 3);
        assert_eq!(m.mv, "D6", "{m:?}");
    }

    #[test]
    fn escapes_atari() {
        // 백 D4가 흑 C4 E4 D3에 둘러싸여 단수 → 백은 D5로 달아나야 함
        let s = st(7, &["C4", "D4", "E4", "A1", "D3"]);
        let m = genmove(&s, 500, 3);
        assert_eq!(m.mv, "D5", "{m:?}");
    }

    #[test]
    fn atari_info() {
        let s = st(7, &["C4", "D4", "E4", "A1", "D3"]);
        let a = ataris(&s);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].liberty, "D5");
    }
}
