//! 사활 문제(묘수풀이): 문제집 + 풀이기.
//!
//! 풀이기는 문제에 정해진 영역 안에서만 수를 두며 끝까지 읽는다(메모이제이션 + 깊이 제한).
//! - 따내기(Capture): 목표 돌을 따내면 성공, 목표 돌들이 모두 활로 3개 이상으로 달아나면 실패
//! - 잡기(Kill): 목표 돌을 따내면 성공, 목표 체인이 무조건 삶(벤슨 판정)이 되면 실패
//! - 살기(Live): 내 목표 체인이 무조건 삶이 되면 성공, 따내지면 실패
//! 깊이 안에 성공하지 못하면 실패로 본다.

use crate::coord;
use crate::go::{GoState, EMPTY};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Goal {
    Capture,
    Kill,
    Live,
}

pub struct Problem {
    pub id: &'static str,
    pub title: &'static str,
    pub level: &'static str,
    pub description: &'static str,
    pub size: usize,
    pub black: &'static [&'static str],
    pub white: &'static [&'static str],
    pub goal: Goal,
    /// 따내기/잡기: 상대 목표 돌(들), 살기: 내 목표 돌
    pub targets: &'static [&'static str],
    /// 수를 둘 수 있는 영역 (왼쪽 아래, 오른쪽 위)
    pub region: (&'static str, &'static str),
    /// 대표 정답 (힌트·검증용)
    pub answer: &'static str,
    pub depth: u32,
}

/// 모든 문제는 흑 차례, 사용자가 흑을 둔다.
pub const PROBLEMS: &[Problem] = &[
    Problem {
        id: "atari-1",
        title: "단수 따내기",
        level: "입문",
        description: "백 한 점이 단수입니다. 따내세요.",
        size: 9,
        black: &["D5", "F5", "E4"],
        white: &["E5"],
        goal: Goal::Capture,
        targets: &["E5"],
        region: ("C3", "G7"),
        answer: "E6",
        depth: 4,
    },
    Problem {
        id: "double-atari",
        title: "양단수",
        level: "입문",
        description: "백 두 점이 각각 활로가 두 개입니다. 한 수로 둘 다 단수를 쳐서 하나를 따내세요.",
        size: 9,
        black: &["C4", "D3", "G4", "F3"],
        white: &["D4", "F4"],
        goal: Goal::Capture,
        targets: &["D4", "F4"],
        region: ("B2", "H6"),
        answer: "E4",
        depth: 6,
    },
    Problem {
        id: "snapback",
        title: "환격",
        level: "초급",
        description: "아래쪽 변의 백 네 점을 잡으세요. 한 점을 버리는 수가 열쇠입니다.",
        size: 9,
        black: &["A3", "B3", "C3", "D3", "D2", "D1"],
        white: &["A2", "B2", "C2", "C1"],
        goal: Goal::Capture,
        targets: &["C2"],
        region: ("A1", "E3"),
        answer: "B1",
        depth: 6,
    },
    Problem {
        id: "net",
        title: "장문",
        level: "초급",
        description: "가운데 백 한 점이 달아나려 합니다. 바로 단수 치지 말고 그물을 씌워 잡으세요.",
        size: 9,
        black: &["C4", "D3", "F3", "C6"],
        white: &["D4"],
        goal: Goal::Capture,
        targets: &["D4"],
        region: ("B2", "G7"),
        answer: "E5",
        depth: 8,
    },
    Problem {
        id: "kill-straight3",
        title: "직삼 급소",
        level: "초급",
        description: "왼쪽 아래 귀의 백은 아래쪽에 세 칸짜리 집이 일자로 있습니다. 급소에 두어 백을 잡으세요.",
        size: 9,
        black: &["A3", "B3", "C3", "D3", "E3", "E2", "E1"],
        white: &["A2", "B2", "C2", "D2", "D1"],
        goal: Goal::Kill,
        targets: &["B2"],
        region: ("A1", "E3"),
        answer: "B1",
        depth: 8,
    },
    Problem {
        id: "live-straight3",
        title: "직삼 살기",
        level: "초급",
        description: "왼쪽 아래 귀의 흑은 아래쪽에 세 칸짜리 집이 일자로 있습니다. 두 집을 만들어 사세요.",
        size: 9,
        black: &["A2", "B2", "C2", "D2", "D1"],
        white: &["A3", "B3", "C3", "D3", "E3", "E2", "E1"],
        goal: Goal::Live,
        targets: &["B2"],
        region: ("A1", "E3"),
        answer: "B1",
        depth: 8,
    },
    Problem {
        id: "kill-bent3",
        title: "곡삼 급소",
        level: "중급",
        description: "귀의 백은 꺾인 세 칸 집을 가지고 있습니다. 급소를 찾아 백을 잡으세요.",
        size: 9,
        black: &["A4", "B4", "C4", "C3", "D3", "D2", "D1"],
        white: &["A3", "B3", "B2", "C2", "C1"],
        goal: Goal::Kill,
        targets: &["B2"],
        region: ("A1", "D4"),
        answer: "A1",
        depth: 8,
    },
];

pub fn find(id: &str) -> Result<&'static Problem, String> {
    PROBLEMS.iter().find(|p| p.id == id).ok_or_else(|| format!("없는 문제: {id}"))
}

impl Problem {
    pub fn initial_state(&self) -> GoState {
        let mut st = GoState::new(self.size, 0.0).unwrap();
        for c in self.black {
            st.board[coord::parse(c, self.size).unwrap()] = 1;
        }
        for c in self.white {
            st.board[coord::parse(c, self.size).unwrap()] = 2;
        }
        st.to_play = 1;
        st
    }

    pub fn goal_text(&self) -> &'static str {
        match self.goal {
            Goal::Capture => "흑 먼저, 백을 따내세요",
            Goal::Kill => "흑 먼저, 백을 잡으세요",
            Goal::Live => "흑 먼저, 사세요",
        }
    }

    fn region_points(&self) -> Vec<usize> {
        let a = coord::parse(self.region.0, self.size).unwrap();
        let b = coord::parse(self.region.1, self.size).unwrap();
        let n = self.size;
        let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
        let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
        let mut v = vec![];
        for y in y0..=y1 {
            for x in x0..=x1 {
                v.push(y * n + x);
            }
        }
        v
    }
}

// ------------------------------------------------------------------ 벤슨 판정

/// color의 체인 중 무조건 살아 있는 체인의 돌 집합 (벤슨 알고리즘)
pub fn benson_alive(st: &GoState, color: u8) -> Vec<bool> {
    let len = st.board.len();
    // 체인
    let mut chain_id = vec![usize::MAX; len];
    let mut chains: Vec<Vec<usize>> = vec![];
    for p in 0..len {
        if st.board[p] != color || chain_id[p] != usize::MAX {
            continue;
        }
        let id = chains.len();
        let mut stones = vec![p];
        chain_id[p] = id;
        let mut i = 0;
        while i < stones.len() {
            let s = stones[i];
            i += 1;
            for q in st.neighbors(s) {
                if st.board[q] == color && chain_id[q] == usize::MAX {
                    chain_id[q] = id;
                    stones.push(q);
                }
            }
        }
        chains.push(stones);
    }
    // 영역: color가 아닌 칸들의 연결 요소
    let mut region_id = vec![usize::MAX; len];
    let mut regions: Vec<Vec<usize>> = vec![];
    for p in 0..len {
        if st.board[p] == color || region_id[p] != usize::MAX {
            continue;
        }
        let id = regions.len();
        let mut pts = vec![p];
        region_id[p] = id;
        let mut i = 0;
        while i < pts.len() {
            let s = pts[i];
            i += 1;
            for q in st.neighbors(s) {
                if st.board[q] != color && region_id[q] == usize::MAX {
                    region_id[q] = id;
                    pts.push(q);
                }
            }
        }
        regions.push(pts);
    }
    // 영역별: 경계 체인들, 그리고 각 체인에 대해 "영역의 모든 빈 점이 그 체인의 활로인가"
    let border: Vec<Vec<usize>> = regions
        .iter()
        .map(|pts| {
            let mut b: Vec<usize> = pts
                .iter()
                .flat_map(|&p| st.neighbors(p).filter(|&q| st.board[q] == color).map(|q| chain_id[q]).collect::<Vec<_>>())
                .collect();
            b.sort();
            b.dedup();
            b
        })
        .collect();
    let vital = |r: usize, c: usize| -> bool {
        regions[r]
            .iter()
            .filter(|&&p| st.board[p] == EMPTY)
            .all(|&p| st.neighbors(p).any(|q| st.board[q] == color && chain_id[q] == c))
    };
    let mut chain_alive = vec![true; chains.len()];
    let mut region_ok = vec![true; regions.len()];
    loop {
        let mut changed = false;
        for c in 0..chains.len() {
            if !chain_alive[c] {
                continue;
            }
            let healthy = (0..regions.len()).filter(|&r| region_ok[r] && border[r].contains(&c) && vital(r, c)).count();
            if healthy < 2 {
                chain_alive[c] = false;
                changed = true;
            }
        }
        for r in 0..regions.len() {
            if region_ok[r] && border[r].iter().any(|&c| !chain_alive[c]) {
                region_ok[r] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut alive = vec![false; len];
    for (c, stones) in chains.iter().enumerate() {
        if chain_alive[c] {
            for &s in stones {
                alive[s] = true;
            }
        }
    }
    alive
}

// ------------------------------------------------------------------ 풀이기

pub struct Solver<'a> {
    pub problem: &'a Problem,
    region: Vec<usize>,
    targets: Vec<usize>,
    memo: HashMap<(Vec<u8>, u8, Option<usize>, u32), bool>,
}

impl<'a> Solver<'a> {
    pub fn new(problem: &'a Problem) -> Self {
        Solver {
            problem,
            region: problem.region_points(),
            targets: problem.targets.iter().map(|c| coord::parse(c, problem.size).unwrap()).collect(),
            memo: HashMap::new(),
        }
    }

    fn libs(st: &GoState, p: usize) -> usize {
        st.group(&st.board, p).1
    }

    /// Some(true): 흑(사용자) 목표 달성, Some(false): 실패 확정
    pub fn status(&self, st: &GoState) -> Option<bool> {
        match self.problem.goal {
            Goal::Capture => {
                if self.targets.iter().any(|&t| st.board[t] != 2) {
                    return Some(true);
                }
                if st.to_play == 1 && self.targets.iter().all(|&t| Self::libs(st, t) >= 3) {
                    return Some(false);
                }
                None
            }
            Goal::Kill => {
                let t = self.targets[0];
                if st.board[t] != 2 {
                    return Some(true);
                }
                if benson_alive(st, 2)[t] {
                    return Some(false);
                }
                None
            }
            Goal::Live => {
                let t = self.targets[0];
                if st.board[t] != 1 {
                    return Some(false);
                }
                if benson_alive(st, 1)[t] {
                    return Some(true);
                }
                None
            }
        }
    }

    fn moves(&self, st: &GoState) -> Vec<String> {
        let mut v: Vec<(i32, String)> = self
            .region
            .iter()
            .filter(|&&p| st.board[p] == EMPTY && st.is_legal(p))
            .map(|&p| {
                // 목표 돌 주변(활로) 우선
                let near = self.targets.iter().any(|&t| st.neighbors(p).any(|q| st.board[q] != EMPTY && st.group(&st.board, q).0.contains(&t)));
                (if near { 0 } else { 1 }, coord::format(p, st.size))
            })
            .collect();
        v.sort();
        let mut out: Vec<String> = v.into_iter().map(|e| e.1).collect();
        if st.to_play == 2 {
            out.push("pass".into());
        }
        out
    }

    /// 현재 국면에서 흑(사용자)이 목표를 이룰 수 있는가
    pub fn black_wins(&mut self, st: &GoState, depth: u32) -> bool {
        if let Some(r) = self.status(st) {
            return r;
        }
        if depth == 0 {
            return false;
        }
        let key = (st.board.clone(), st.to_play, st.ko, depth);
        if let Some(&r) = self.memo.get(&key) {
            return r;
        }
        let black = st.to_play == 1;
        let mut result = !black; // 흑: 하나라도 이기면 true / 백: 하나라도 막으면 false
        for mv in self.moves(st) {
            let mut next = st.clone();
            if next.play_coord(&mv).is_err() {
                continue;
            }
            let w = self.black_wins(&next, depth - 1);
            if black && w {
                result = true;
                break;
            }
            if !black && !w {
                result = false;
                break;
            }
        }
        self.memo.insert(key, result);
        result
    }

    /// 흑 차례에서 이기는 수들
    pub fn winning_moves(&mut self, st: &GoState, depth: u32) -> Vec<String> {
        let mut out = vec![];
        for mv in self.moves(st) {
            let mut next = st.clone();
            if next.play_coord(&mv).is_ok() && self.black_wins(&next, depth.saturating_sub(1)) {
                out.push(mv);
            }
        }
        out
    }

    /// 백 차례에서 가장 끈질긴 응수: 흑을 막는 수가 있으면 그것, 없으면 따내기·활로 늘리기 위주
    pub fn best_defense(&mut self, st: &GoState, depth: u32) -> String {
        let moves = self.moves(st);
        for mv in &moves {
            let mut next = st.clone();
            if next.play_coord(mv).is_ok() && !self.black_wins(&next, depth.saturating_sub(1)) {
                return mv.clone();
            }
        }
        // 모두 진다면: 따내기 > 목표 돌 활로 최대화
        let mut best = (i64::MIN, "pass".to_string());
        for mv in moves {
            let mut next = st.clone();
            let Ok(captured) = next.play_coord(&mv) else { continue };
            let libs: usize = self.targets.iter().filter(|&&t| next.board[t] != EMPTY).map(|&t| Self::libs(&next, t)).sum();
            let score = captured.len() as i64 * 100 + libs as i64 * 10 - if mv == "pass" { 50 } else { 0 };
            if score > best.0 {
                best = (score, mv);
            }
        }
        best.1
    }
}

#[derive(Serialize, Debug)]
pub struct ProblemInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub level: &'static str,
    pub description: &'static str,
    pub goal: Goal,
    pub goal_text: &'static str,
    pub region: [&'static str; 2],
}

pub fn info(p: &Problem) -> ProblemInfo {
    ProblemInfo {
        id: p.id,
        title: p.title,
        level: p.level,
        description: p.description,
        goal: p.goal,
        goal_text: p.goal_text(),
        region: [p.region.0, p.region.1],
    }
}

#[derive(Serialize, Debug)]
pub struct Reply {
    /// "solved" | "failed" | "continue"
    pub status: &'static str,
    /// 백의 응수 (없으면 None)
    #[serde(rename = "move")]
    pub mv: Option<String>,
    pub comment: String,
}

/// 사용자가 흑을 둔 직후 국면(백 차례)을 받아 판정하고 백의 응수를 고른다.
pub fn reply(p: &Problem, st: &GoState) -> Reply {
    let mut solver = Solver::new(p);
    if let Some(r) = solver.status(st) {
        return if r {
            Reply { status: "solved", mv: None, comment: "정답입니다! 목표를 이루었습니다.".into() }
        } else {
            Reply { status: "failed", mv: None, comment: "실패했습니다. 다시 도전해 보세요.".into() }
        };
    }
    let still = solver.black_wins(st, p.depth);
    let mv = solver.best_defense(st, p.depth);
    let mut after = st.clone();
    let _ = after.play_coord(&mv);
    let status_after = solver.status(&after);
    let (status, comment) = match (still, status_after) {
        (_, Some(false)) => ("failed", "백이 버텼습니다. 실패입니다. 다시 도전해 보세요."),
        (true, _) => ("continue", "좋습니다. 백이 응수했습니다. 계속하세요."),
        (false, _) => ("failed", "이 수로는 목표를 이룰 수 없습니다. 다시 도전해 보세요."),
    };
    Reply { status, mv: Some(mv), comment: comment.into() }
}

/// 흑 차례 국면에서 정답 수들
pub fn hint(p: &Problem, st: &GoState) -> Vec<String> {
    let mut solver = Solver::new(p);
    solver.winning_moves(st, p.depth)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 모든 문제: 처음 국면에서 흑이 이길 수 있고, 대표 정답이 이기는 수에 포함되어야 한다
    #[test]
    fn all_problems_are_valid() {
        for p in PROBLEMS {
            let st = p.initial_state();
            let mut solver = Solver::new(p);
            assert_eq!(solver.status(&st), None, "{}: 시작부터 판정됨", p.id);
            let wins = solver.winning_moves(&st, p.depth);
            assert!(wins.contains(&p.answer.to_string()), "{}: 정답 {} 이(가) 이기는 수 {:?}에 없음", p.id, p.answer, wins);
            // 문제다운지: 아무 수나 이기면 안 된다
            let total = solver.moves(&st).len();
            assert!(wins.len() * 2 < total, "{}: 이기는 수가 너무 많음 {:?}", p.id, wins);
        }
    }

    #[test]
    fn wrong_move_fails_straight3() {
        let p = find("kill-straight3").unwrap();
        let mut st = p.initial_state();
        st.play_coord("A1").unwrap();
        let r = reply(p, &st);
        assert_eq!(r.status, "failed", "{r:?}");
    }

    #[test]
    fn right_move_then_solved() {
        let p = find("atari-1").unwrap();
        let mut st = p.initial_state();
        st.play_coord("E6").unwrap();
        assert_eq!(reply(p, &st).status, "solved");
    }

    #[test]
    fn benson_two_eyes() {
        let p = find("live-straight3").unwrap();
        let mut st = p.initial_state();
        st.play_coord("B1").unwrap();
        assert!(benson_alive(&st, 1)[coord::parse("B2", 9).unwrap()]);
    }
}
